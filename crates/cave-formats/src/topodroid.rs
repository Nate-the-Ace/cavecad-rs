//! TopoDroid's own archive: the ZIP its survey Export offers first, which holds the whole survey in
//! one file -- the shots and fixes (`survey.sql`), the version they were written by (`manifest`),
//! and every sketch, one binary `.tdr` per plot.
//!
//! Read as TopoDroid reads it back (`DataHelper.loadFromFile`, `DrawingIO.doLoadDataStream`, in the
//! topodroid fork), field for field and version for version: a field a version did not write yet is
//! skipped exactly where TopoDroid skips it. Which shots count, and how a leg's repeated readings
//! become one leg, follows TopoDroid's own Therion export (`TDExporter.exportSurveyAsTh`), so this
//! reads the same survey that export would write.
//!
//! The zip itself is opened by the caller (the browser already has an unzipper): this module takes
//! the files in it, by name.

use crate::js::JsObj;
use crate::model::{Cs, Fixed, Shot, Survey, Trip, normalize_azimuth};
use crate::th2::{Area, Line, Point, Scale, Scrap, Seg, Sketch, StationTie};

/// A plot (TopoDroid's word for one sketch) and what was read of it.
#[derive(Clone, Debug)]
pub struct Plot {
    /// The plot's name in the survey (`1p`, `1s`, `xx3`).
    pub name: String,
    /// The view, as a Therion projection: "plan", "extended", "elevation" (TopoDroid's projected
    /// profile), or "none" (a cross section).
    pub projection: &'static str,
    /// The sketch, in Therion's terms (TopoDroid names its symbols as Therion does). `None` when the
    /// archive has no file for the plot (TopoDroid writes none for a plot never drawn on).
    pub sketch: Option<Sketch>,
    /// TopoDroid's plot type (`PlotType`: 0 section at a station seen from the plan, 3 horizontal
    /// section of a leg, 5 section of a leg, 7 section at a station seen from the profile, ...).
    pub kind: i64,
    /// The station it starts at; for a leg's section, the leg's first station.
    pub start: String,
    /// For a leg's section, the leg's other station.
    pub view: String,
    /// A section's facing: the bearing it looks along (degrees, TopoDroid's own north) and its
    /// tilt (degrees up).
    pub azimuth: f64,
    #[allow(missing_docs)]
    pub clino: f64,
    /// Where along its leg a leg's section is cut (0 at `start`, 1 at `view`), when TopoDroid
    /// recorded it.
    pub intercept: Option<f64>,
}

/// What an archive holds.
#[derive(Clone, Debug)]
pub struct Archive {
    /// The survey data.
    pub survey: Survey,
    /// Its plots, in the order the survey lists them; deleted ones left out.
    pub plots: Vec<Plot>,
    /// Degrees added to every bearing TopoDroid stores (declination, less the grid's convergence
    /// where a fix put the cave on a grid): a plot's facing turns by as much.
    pub bearing_shift: f64,
}

/// Whether these files (names inside a zip) are a TopoDroid archive.
pub fn is_archive(names: &[&str]) -> bool {
    let has = |want: &str| names.iter().any(|n| base(n) == want);
    has("survey.sql") && has("manifest")
}

fn base(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Reads an archive from its files (name in the zip, bytes).
pub fn read_archive(files: &[(String, Vec<u8>)]) -> Result<Archive, String> {
    let find = |want: &str| files.iter().find(|(n, _)| base(n) == want);
    let Some((_, sql)) = find("survey.sql") else {
        return Err("This zip is not a TopoDroid survey: it has no survey.sql in it.".into());
    };
    let manifest = find("manifest").map(|(_, b)| String::from_utf8_lossy(b).to_string());
    let db_version = manifest
        .as_deref()
        .and_then(|m| m.lines().nth(1))
        .and_then(|l| l.trim().parse::<i64>().ok())
        .unwrap_or(i64::MAX);
    let sql = String::from_utf8_lossy(sql);
    let rows = read_rows(&sql);
    let mut survey = to_survey(&rows, db_version)?;
    let mut plots = Vec::new();
    for row in rows.iter().filter(|r| r.table == "plots") {
        let mut v = row.values();
        let (_sid, _id) = (v.long(-1), v.long(-1));
        let name = v.string();
        let mut kind = v.long(-1);
        if db_version <= 20 && kind == 3 {
            kind = 5;
        }
        let status = v.long(0);
        let start = v.string();
        let view = v.string();
        let (_xoff, _yoff, _zoom) = (v.double(0.0), v.double(0.0), v.double(1.0));
        let azimuth = v.double(0.0);
        let clino = if db_version > 20 { v.double(0.0) } else { 0.0 };
        let _hide = if db_version > 24 {
            v.string()
        } else {
            String::new()
        };
        let _nick = if db_version > 30 {
            v.string()
        } else {
            String::new()
        };
        let _orientation = if db_version > 32 { v.long(0) } else { 0 };
        let _maxscrap = if db_version > 41 { v.long(0) } else { 0 };
        let intercept = if db_version > 42 {
            v.double(-1.0)
        } else {
            -1.0
        };
        if status != NORMAL {
            continue;
        }
        let projection = match kind {
            1 => "plan",
            2 => "extended",
            8 | 9 => "elevation",
            _ => "none",
        };
        let file = format!("{}-{name}.tdr", survey.cave_name);
        let sketch = files
            .iter()
            .find(|(n, _)| base(n) == file)
            .or_else(|| {
                // a survey renamed after drawing: the file still ends in the plot's name
                let tail = format!("-{name}.tdr");
                files.iter().find(|(n, _)| base(n).ends_with(&tail))
            })
            .map(|(_, b)| read_tdr(b, &name, projection));
        plots.push(Plot {
            name,
            projection,
            sketch,
            kind,
            start,
            view,
            azimuth,
            clino,
            intercept: (0.0..=1.0).contains(&intercept).then_some(intercept),
        });
    }
    if survey.shots.is_empty() {
        survey.add_finding(
            "warning",
            "topodroid-empty",
            "The TopoDroid survey has no shots in it.".into(),
        );
    }
    // what every bearing was turned by: the declination, less the convergence where it applies
    let bearing_shift = survey
        .shots
        .iter()
        .find_map(|s| s.declination)
        .unwrap_or(survey.declination);
    Ok(Archive {
        survey,
        plots,
        bearing_shift,
    })
}

/// A shot's, plot's or fix's status: TopoDroid keeps deleted ones, marked.
const NORMAL: i64 = 0;
/// A shot's flags (`DBlock.FLAG_*`).
const FLAG_SURFACE: i64 = 1;
const FLAG_DUPLICATE: i64 = 2;
const FLAG_COMMENTED: i64 = 4;
/// A shot's leg column (`LegType`): another reading of the leg before it.
const LEG_EXTRA: i64 = 1;
const LEG_XSPLAY: i64 = 2;
/// TopoDroid's "no declination set": anything at or above it (it writes 1080).
const DECLINATION_MAX: f64 = 720.0;
/// Two readings closer than this (summed relative difference) are one leg read twice
/// (`TDSetting.mCloseDistance`, its default).
const CLOSE_DISTANCE: f64 = 0.05;

/// One `INSERT into <table> values( ... );` line.
struct Row {
    table: String,
    body: String,
}

impl Row {
    fn values(&self) -> Values {
        Values {
            tokens: tokens(&self.body),
            at: 0,
        }
    }
}

fn read_rows(sql: &str) -> Vec<Row> {
    sql.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, ' ');
            let (_insert, _into) = (parts.next()?, parts.next()?);
            let table = parts.next()?.to_string();
            let rest = parts.next()?;
            let open = rest.find('(')? + 1;
            let close = rest.rfind(')')?;
            (close >= open).then(|| Row {
                table,
                body: rest[open..close].to_string(),
            })
        })
        .collect()
}

/// TopoDroid's `Scanline`: values split at commas and spaces, a quoted value running to its
/// closing quote (TopoDroid writes a `"` inside one as ESC).
fn tokens(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = body.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| *c == ' ' || *c == ',') {
            chars.next();
        }
        let Some(&c) = chars.peek() else { break };
        if c == '"' || c == '\'' {
            chars.next();
            let s: String = chars.by_ref().take_while(|x| *x != c).collect();
            out.push(s.replace('\u{1b}', "\""));
        } else {
            let mut s = String::new();
            while let Some(&x) = chars.peek() {
                if x == ' ' || x == ',' {
                    break;
                }
                s.push(x);
                chars.next();
            }
            out.push(s);
        }
    }
    out
}

/// A row's values read in order, each with TopoDroid's default when missing or unreadable.
struct Values {
    tokens: Vec<String>,
    at: usize,
}

impl Values {
    fn next(&mut self) -> Option<&str> {
        let t = self.tokens.get(self.at)?;
        self.at += 1;
        Some(t)
    }
    fn string(&mut self) -> String {
        self.next().unwrap_or_default().to_string()
    }
    fn long(&mut self, default: i64) -> i64 {
        self.next()
            .and_then(|t| t.parse::<i64>().ok())
            .unwrap_or(default)
    }
    fn double(&mut self, default: f64) -> f64 {
        self.next()
            .and_then(|t| t.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    }
}

/// One shot as TopoDroid stored it.
#[derive(Clone)]
struct Reading {
    from: String,
    to: String,
    length: f64,
    bearing: f64,
    clino: f64,
    flag: i64,
    leg: i64,
    comment: String,
}

impl Reading {
    fn vector(&self) -> [f64; 3] {
        let (b, c) = (self.bearing.to_radians(), self.clino.to_radians());
        [
            self.length * b.sin() * c.cos(),
            self.length * b.cos() * c.cos(),
            self.length * c.sin(),
        ]
    }

    /// `DBlock.checkRelativeDistance`: the same leg read again.
    fn close_to(&self, other: &Reading) -> bool {
        let (a, b) = (self.vector(), other.vector());
        let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        self.length > 0.0
            && other.length > 0.0
            && d / self.length + d / other.length < CLOSE_DISTANCE
    }
}

fn to_survey(rows: &[Row], db_version: i64) -> Result<Survey, String> {
    let Some(head) = rows.iter().find(|r| r.table == "surveys") else {
        return Err("The TopoDroid survey.sql names no survey.".into());
    };
    let mut v = head.values();
    let _sid = v.long(-1);
    let name = v.string();
    let date = v.string();
    let team = v.string();
    let decl = if db_version > 14 { v.double(0.0) } else { 0.0 };
    let comment = v.string();
    let declination = (decl.abs() < DECLINATION_MAX).then_some(decl);

    let mut survey = Survey {
        name: String::new(),
        cave_name: name.clone(),
        date: date.replace('.', "-"),
        team: team.clone(),
        distance_unit: Some("m".into()),
        declination: declination.unwrap_or(0.0),
        declination_source: if declination.is_some() {
            "file".into()
        } else {
            String::new()
        },
        ..Survey::default()
    };
    survey.trips.push(Trip {
        name: name.clone(),
        date: date.replace('.', "-"),
        team,
        declination: declination.unwrap_or(0.0),
        declination_source: survey.declination_source.clone(),
        distance_unit: Some("m".into()),
        start_note: comment,
        ..Trip::default()
    });
    // the shots TopoDroid exports: not deleted, in the order they were taken
    let mut readings: Vec<(i64, Reading)> = rows
        .iter()
        .filter(|r| r.table == "shots")
        .filter_map(|r| {
            let mut v = r.values();
            let (_sid, id) = (v.long(-1), v.long(-1));
            let from = v.string();
            let to = v.string();
            let (length, bearing, clino) = (v.double(0.0), v.double(0.0), v.double(0.0));
            let (_roll, _acc, _mag, _dip) =
                (v.double(0.0), v.double(1.0), v.double(1.0), v.double(90.0));
            let _extend = v.long(1);
            let flag = v.long(0);
            let leg = v.long(0);
            let status = v.long(0);
            let comment = v.string();
            (status == NORMAL).then_some((
                id,
                Reading {
                    from,
                    to,
                    length,
                    bearing,
                    clino,
                    flag,
                    leg,
                    comment,
                },
            ))
        })
        .collect();
    readings.sort_by_key(|(id, _)| *id);

    let mut legs: Vec<(Reading, Vec<Reading>)> = Vec::new(); // a leg and its other readings
    let mut open: Option<usize> = None; // the leg still taking readings
    let mut shots: Vec<(Shot, i64)> = Vec::new();
    let mut order: Vec<Item> = Vec::new();
    for (_, r) in readings {
        match (r.from.is_empty(), r.to.is_empty()) {
            (true, true) => {
                if let Some(i) = open
                    && (r.leg == LEG_EXTRA || r.close_to(&legs[i].0))
                {
                    legs[i].1.push(r);
                }
            }
            (false, false) => {
                legs.push((r, Vec::new()));
                open = Some(legs.len() - 1);
                order.push(Item::Leg(legs.len() - 1));
            }
            _ => {
                open = None;
                shots.push((splay(&r), r.flag));
                order.push(Item::Splay(shots.len() - 1));
            }
        }
    }
    let mut out = Vec::new();
    let mut commented = 0;
    for item in order {
        let (s, flag) = match item {
            Item::Splay(i) => shots[i].clone(),
            Item::Leg(i) => {
                let (first, more) = &legs[i];
                (leg(first, more), first.flag)
            }
        };
        if flag & FLAG_COMMENTED != 0 {
            commented += 1;
            continue;
        }
        out.push(s);
    }
    if commented > 0 {
        survey.add_finding(
            "info",
            "topodroid-commented",
            format!(
                "{commented} shot{} marked commented-out in TopoDroid {} left out, as TopoDroid's own exports leave {} out.",
                if commented == 1 { "" } else { "s" },
                if commented == 1 { "was" } else { "were" },
                if commented == 1 { "it" } else { "them" }
            ),
        );
    }
    // the first fix TopoDroid has: GPS longitude/latitude and the height above the geoid
    let fix = rows.iter().filter(|r| r.table == "fixeds").find_map(|r| {
        let mut v = r.values();
        let (_sid, _id) = (v.long(-1), v.long(-1));
        let station = v.string();
        let (lng, lat, _h_ell, h_geo) = (
            v.double(f64::NAN),
            v.double(f64::NAN),
            v.double(0.0),
            v.double(f64::NAN),
        );
        let _comment = v.string();
        let status = v.long(0);
        (status == NORMAL && lat.abs() <= 90.0 && lng.abs() <= 180.0)
            .then_some((station, lat, lng, h_geo))
    });

    // TopoDroid's bearings are magnetic. With no declination set, the one for the survey's date
    // at its fix, as Therion works out for TopoDroid's own Therion export; with no fix either,
    // none (TopoDroid draws it so too).
    let decl = match (declination, &fix) {
        (Some(d), _) => d,
        (None, Some((_, lat, lng, _))) => {
            let year = cave_geomag::decimal_year_of(&survey.date);
            match year.and_then(|y| cave_geomag::declination(*lat, *lng, y)) {
                Some(d) => {
                    survey.declination = d;
                    survey.declination_source = "computed".into();
                    survey.trips[0].declination = d;
                    survey.trips[0].declination_source = "computed".into();
                    survey.add_finding(
                        "info",
                        "topodroid-declination-computed",
                        format!(
                            "The TopoDroid survey sets no magnetic declination, so the one for its date at its fix was used: {d:.2} degrees."
                        ),
                    );
                    d
                }
                None => 0.0,
            }
        }
        (None, None) => {
            survey.add_finding(
                "warning",
                "topodroid-no-declination",
                "The TopoDroid survey sets no magnetic declination and has no fix to work one out from, so its bearings are used as read, from magnetic north.".into(),
            );
            0.0
        }
    };
    for s in &mut out {
        if decl != 0.0 && s.azimuth.is_finite() && !s.azimuth_omitted {
            s.azimuth = normalize_azimuth(s.azimuth + decl);
        }
    }
    survey.shots = out;
    if let Some(f) = fix {
        place(&mut survey, f);
    }
    Ok(survey)
}

enum Item {
    Leg(usize),
    Splay(usize),
}

/// A leg from its readings: the mean of them as vectors, as TopoDroid's `AverageLeg` takes it.
fn leg(first: &Reading, more: &[Reading]) -> Shot {
    let mut sum = first.vector();
    for r in more {
        let v = r.vector();
        sum = [sum[0] + v[0], sum[1] + v[1], sum[2] + v[2]];
    }
    let n = (1 + more.len()) as f64;
    let [x, y, z] = [sum[0] / n, sum[1] / n, sum[2] / n];
    let (length, azimuth, inclination) = if more.is_empty() {
        (first.length, first.bearing, first.clino)
    } else {
        let h = x.hypot(y);
        (
            (h * h + z * z).sqrt(),
            normalize_azimuth(x.atan2(y).to_degrees()),
            z.atan2(h).to_degrees(),
        )
    };
    let surface = first.flag & FLAG_SURFACE != 0;
    Shot {
        from: first.from.clone(),
        to: first.to.clone(),
        distance: length,
        azimuth,
        inclination,
        exclude_from_length: surface || first.flag & FLAG_DUPLICATE != 0,
        exclude_from_plot: surface,
        notes: first.comment.clone(),
        ..Shot::default()
    }
}

/// A splay. One written only with its far station (`. 7`) is a shot taken toward that station:
/// turned round, it is a splay from it.
fn splay(r: &Reading) -> Shot {
    let (from, azimuth, inclination) = if r.from.is_empty() {
        (r.to.clone(), normalize_azimuth(r.bearing + 180.0), -r.clino)
    } else {
        (r.from.clone(), r.bearing, r.clino)
    };
    Shot {
        from,
        to: String::new(),
        distance: r.length,
        azimuth,
        inclination,
        splay: true,
        notes: if r.leg == LEG_XSPLAY && r.comment.is_empty() {
            "cross-section splay".into()
        } else {
            r.comment.clone()
        },
        ..Shot::default()
    }
}

/// Puts the cave where its fix says: on the UTM grid of the fix's zone, bearings turned by the
/// grid's convergence so the survey lands on that grid (as Therion does with a grid fix).
fn place(survey: &mut Survey, (station, lat, lng, h_geo): (String, f64, f64, f64)) {
    let zone = cave_geo::frame::utm_zone(cave_geo::frame::LatLon { lat, lon: lng });
    let grid = Cs::parse(&format!(
        "UTM{}{}",
        zone.abs(),
        if zone < 0 { "S" } else { "N" }
    ));
    let Some((e, n)) = grid.at_latlon(lat, lng) else {
        return;
    };
    survey.fixed.set(
        &station,
        Fixed {
            x: Some(e),
            y: Some(n),
            z: h_geo.is_finite().then_some(h_geo),
        },
    );
    survey.fixed_frame = Some(cave_geo::datum::FixedFrame {
        zone,
        datum: cave_geo::datum::Datum::Wgs84,
    });
    let conv = grid.convergence(lat, lng);
    if conv != 0.0 {
        let decl = survey.declination;
        for s in &mut survey.shots {
            if s.azimuth.is_finite() && !s.azimuth_omitted {
                s.azimuth = normalize_azimuth(s.azimuth - conv);
            }
            s.declination = Some(decl - conv);
        }
    }
    if !h_geo.is_finite() {
        survey.add_finding(
            "warning",
            "topodroid-fix-height",
            format!("The fix on station {station} has no height, so the cave's elevations are not known."),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// .tdr: a plot's drawing, as `DataOutputStream` wrote it

/// Metres per sketch unit: TopoDroid draws 20 units to the metre (`DrawingUtil.SCALE_FIX`).
const METRES_PER_UNIT: f64 = 1.0 / 20.0;

struct Bytes<'a> {
    b: &'a [u8],
    at: usize,
}

impl Bytes<'_> {
    fn u8(&mut self) -> Option<u8> {
        let v = *self.b.get(self.at)?;
        self.at += 1;
        Some(v)
    }
    fn int(&mut self) -> Option<i32> {
        let s = self.b.get(self.at..self.at + 4)?;
        self.at += 4;
        Some(i32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn float(&mut self) -> Option<f64> {
        let s = self.b.get(self.at..self.at + 4)?;
        self.at += 4;
        Some(f32::from_be_bytes([s[0], s[1], s[2], s[3]]) as f64)
    }
    /// Java's `readUTF`: a 2-byte length, then modified UTF-8 (a NUL as C0 80).
    fn utf(&mut self) -> Option<String> {
        let s = self.b.get(self.at..self.at + 2)?;
        let n = u16::from_be_bytes([s[0], s[1]]) as usize;
        self.at += 2;
        let raw = self.b.get(self.at..self.at + n)?;
        self.at += n;
        let mut fixed = Vec::with_capacity(raw.len());
        let mut i = 0;
        while i < raw.len() {
            if raw[i] == 0xC0 && raw.get(i + 1) == Some(&0x80) {
                fixed.push(0);
                i += 2;
            } else {
                fixed.push(raw[i]);
                i += 1;
            }
        }
        Some(String::from_utf8_lossy(&fixed).to_string())
    }
    /// A point, with the two control points of a curve to it when it has them.
    fn seg(&mut self) -> Option<Seg> {
        let to = [self.float()?, self.float()?];
        if self.u8()? == 1 {
            let c1 = [self.float()?, self.float()?];
            let c2 = [self.float()?, self.float()?];
            Some(Seg {
                to,
                c1: Some(c1),
                c2: Some(c2),
            })
        } else {
            Some(Seg {
                to,
                c1: None,
                c2: None,
            })
        }
    }
}

/// TopoDroid's `-key value` option text as Therion options.
fn options(text: &str) -> JsObj<String> {
    let mut out = JsObj::new();
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut i = 0;
    while i < words.len() {
        if let Some(key) = words[i].strip_prefix('-') {
            let value = words.get(i + 1).filter(|w| !w.starts_with('-'));
            out.set(key, value.map(|v| v.to_string()).unwrap_or_default());
            i += if value.is_some() { 2 } else { 1 };
        } else {
            i += 1;
        }
    }
    out
}

/// Reads a plot's `.tdr` as one Therion scrap in sketch units (y up, as Therion's are), its
/// station names as the ties. What cannot be read past is reported and what came before it kept.
pub fn read_tdr(bytes: &[u8], plot: &str, projection: &'static str) -> Sketch {
    let mut sketch = Sketch::default();
    let mut scrap = Scrap {
        name: plot.to_string(),
        projection: Some(projection.to_string()),
        projection_angle: None,
        scale: Some(Scale {
            units_per_drawing: METRES_PER_UNIT,
            unit: "m".into(),
        }),
        options: JsObj::new(),
        stations: Vec::new(),
        lines: Vec::new(),
        areas: Vec::new(),
        points: Vec::new(),
        sections: Vec::new(),
    };
    let mut r = Bytes { b: bytes, at: 0 };
    let mut version: i32 = 0;
    let mut borders = 0;
    let flip = |s: Seg| Seg {
        to: [s.to[0], -s.to[1]],
        c1: s.c1.map(|c| [c[0], -c[1]]),
        c2: s.c2.map(|c| [c[0], -c[1]]),
    };
    let ok = (|| -> Option<()> {
        loop {
            let what = r.u8()?;
            match what {
                b'V' => version = r.int()?,
                b'I' => {
                    for _ in 0..4 {
                        r.float()?;
                    }
                    if r.int()? == 1 {
                        for _ in 0..4 {
                            r.float()?;
                        }
                    }
                }
                b'S' => {
                    let _name = r.utf()?;
                    let kind = r.int()?;
                    if kind == 8 {
                        let _dir = r.int()?;
                    }
                    for _ in 0..3 {
                        r.utf()?; // the symbols it uses
                    }
                }
                b'N' => {
                    r.int()?;
                }
                b'P' => {
                    let (x, y) = (r.float()?, r.float()?);
                    let name = r.utf()?;
                    if version >= 401147 {
                        r.utf()?;
                    }
                    let orientation = r.float()?;
                    let scale = r.int()?;
                    if version >= 401090 {
                        r.int()?;
                    }
                    if version >= 401160 {
                        r.int()?;
                    }
                    let text = if version >= 303066 {
                        Some(r.utf()?)
                    } else {
                        None
                    };
                    let opts = r.utf()?;
                    scrap.points.push(Point {
                        kind: name.clone(),
                        x,
                        y: -y,
                        orientation: (orientation != 0.0).then_some(orientation),
                        scale: point_scale(scale),
                        text: text.filter(|t| !t.is_empty()),
                        options: options(&opts),
                    });
                }
                b'T' => {
                    let (x, y) = (r.float()?, r.float()?);
                    let orientation = if version > 207043 { r.float()? } else { 0.0 };
                    let scale = r.int()?;
                    if version > 401090 {
                        r.int()?;
                    }
                    if version > 401160 {
                        r.int()?;
                    }
                    let text = r.utf()?;
                    let opts = r.utf()?;
                    scrap.points.push(Point {
                        kind: "label".into(),
                        x,
                        y: -y,
                        orientation: (orientation != 0.0).then_some(orientation),
                        scale: point_scale(scale),
                        text: Some(text),
                        options: options(&opts),
                    });
                }
                b'Y' => {
                    r.float()?;
                    r.float()?;
                    if version > 207043 {
                        r.float()?;
                    }
                    r.int()?;
                    if version >= 401090 {
                        r.int()?;
                    }
                    if version >= 401160 {
                        r.int()?;
                    }
                    r.utf()?;
                    r.utf()?;
                    r.int()?;
                    if version >= 602067 {
                        r.utf()?;
                        if r.int()? == 1 {
                            r.float()?;
                            r.float()?;
                            r.float()?;
                        }
                    }
                }
                b'Z' => {
                    r.float()?;
                    r.float()?;
                    if version > 207043 {
                        r.float()?;
                    }
                    r.int()?;
                    if version >= 401090 {
                        r.int()?;
                    }
                    if version > 401160 {
                        r.int()?;
                    }
                    r.utf()?;
                    r.utf()?;
                    r.int()?;
                }
                b'L' => {
                    let name = r.utf()?;
                    if version >= 401147 {
                        r.utf()?;
                    }
                    let closed = r.u8()? == 1;
                    let reversed = r.u8()? == 1;
                    r.int()?; // outline
                    if version >= 602055 {
                        r.int()?;
                    }
                    if version >= 401090 {
                        r.int()?;
                    }
                    if version >= 401160 {
                        r.int()?;
                    }
                    if version >= 604088 {
                        r.int()?;
                    }
                    let opts = r.utf()?;
                    let n = r.int()?.max(0);
                    let mut segs = Vec::with_capacity(n as usize);
                    for _ in 0..n {
                        segs.push(flip(r.seg()?));
                    }
                    // the first point is where the line starts, never a curve
                    if let Some(first) = segs.first_mut() {
                        first.c1 = None;
                        first.c2 = None;
                    }
                    let (kind, subtype) = match name.split_once(':') {
                        Some((k, s)) if k != "u" => (k.to_string(), Some(s.to_string())),
                        _ => (name.clone(), None),
                    };
                    scrap.lines.push(Line {
                        kind,
                        subtype,
                        closed,
                        reversed,
                        id: None,
                        options: options(&opts),
                        segs,
                    });
                }
                b'A' => {
                    let name = r.utf()?;
                    if version >= 401147 {
                        r.utf()?;
                    }
                    let _prefix = r.utf()?;
                    let _cnt = r.int()?;
                    let visible = r.u8()? == 1;
                    let _orientation = r.float()?;
                    if version >= 401090 {
                        r.int()?;
                    }
                    if version >= 401160 {
                        r.int()?;
                    }
                    if version >= 604096 {
                        r.int()?;
                    }
                    let opts = if version >= 604098 {
                        r.utf()?
                    } else {
                        String::new()
                    };
                    let n = r.int()?.max(0);
                    let mut segs = Vec::with_capacity(n as usize);
                    for _ in 0..n {
                        segs.push(flip(r.seg()?));
                    }
                    if let Some(first) = segs.first_mut() {
                        first.c1 = None;
                        first.c2 = None;
                    }
                    if segs.len() < 3 {
                        continue; // TopoDroid drops these too
                    }
                    // as TopoDroid's own Therion export writes it: a closed border, and the area by it
                    borders += 1;
                    let id = format!("{plot}-a{borders}");
                    let mut border_opts = JsObj::new();
                    if !visible {
                        border_opts.set("visibility", "off".into());
                    }
                    scrap.lines.push(Line {
                        kind: "border".into(),
                        subtype: None,
                        closed: true,
                        reversed: false,
                        id: Some(id.clone()),
                        options: border_opts,
                        segs,
                    });
                    scrap.areas.push(Area {
                        kind: name.clone(),
                        line_ids: vec![id],
                        options: options(&opts),
                    });
                }
                b'J' => {
                    r.int()?;
                    r.float()?;
                    r.float()?;
                    if version >= 401090 {
                        r.int()?;
                    }
                    if version >= 401160 {
                        r.int()?;
                    }
                }
                b'U' => {
                    let (x, y) = (r.float()?, r.float()?);
                    r.int()?;
                    if version >= 401090 {
                        r.int()?;
                    }
                    if version >= 401160 {
                        r.int()?;
                    }
                    let name = r.utf()?;
                    push_tie(&mut scrap, name, x, -y);
                }
                b'X' => {
                    let (x, y) = (r.float()?, r.float()?);
                    let name = r.utf()?;
                    if version >= 401090 {
                        r.int()?;
                    }
                    if version >= 401160 {
                        r.int()?;
                    }
                    if version >= 207038 && r.int()? != -1 {
                        r.float()?;
                        r.float()?;
                    }
                    push_tie(&mut scrap, name, x, -y);
                }
                b'F' => {} // the drawing is done; the station names follow
                b'E' => return Some(()),
                _ => return None,
            }
        }
    })();
    if ok.is_none() {
        sketch.add(
            "warning",
            "tdr-unreadable",
            format!(
                "The sketch {plot} could not be read to its end (TopoDroid file version {version}); what came before is kept."
            ),
        );
    }
    sketch.scraps.push(scrap);
    sketch
}

fn push_tie(scrap: &mut Scrap, name: String, x: f64, y: f64) {
    if !name.is_empty() && !scrap.stations.iter().any(|s| s.name == name) {
        scrap.stations.push(StationTie { name, x, y });
    }
}

/// TopoDroid's point sizes (`PointScale`: -2 to 2, 0 normal) as Therion's.
fn point_scale(scale: i32) -> Option<String> {
    match scale {
        -2 => Some("xs".into()),
        -1 => Some("s".into()),
        1 => Some("l".into()),
        2 => Some("xl".into()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes what Java's `DataOutputStream` would, for building `.tdr` files here.
    #[derive(Default)]
    struct Out(Vec<u8>);
    impl Out {
        fn tag(&mut self, c: u8) -> &mut Self {
            self.0.push(c);
            self
        }
        fn byte(&mut self, b: u8) -> &mut Self {
            self.0.push(b);
            self
        }
        fn int(&mut self, v: i32) -> &mut Self {
            self.0.extend(v.to_be_bytes());
            self
        }
        fn float(&mut self, v: f32) -> &mut Self {
            self.0.extend(v.to_be_bytes());
            self
        }
        fn utf(&mut self, s: &str) -> &mut Self {
            self.0.extend((s.len() as u16).to_be_bytes());
            self.0.extend(s.as_bytes());
            self
        }
        /// A line or area point with no curve.
        fn pt(&mut self, x: f32, y: f32) -> &mut Self {
            self.float(x).float(y).byte(0)
        }
    }

    /// A plan drawing as TopoDroid 6.4 writes it: a wall, a pool, a clay symbol, a label, and the
    /// two stations' names.
    fn tdr(version: i32) -> Vec<u8> {
        let v = version;
        let mut o = Out::default();
        // level and scrap, written from the versions that have them (`gt`: TopoDroid's labels test
        // with > where the rest test with >=)
        let lvl = |o: &mut Out, gt: bool| {
            if if gt { v > 401090 } else { v >= 401090 } {
                o.int(1);
            }
            if if gt { v > 401160 } else { v >= 401160 } {
                o.int(0);
            }
        };
        o.tag(b'V').int(v);
        o.tag(b'I')
            .float(0.0)
            .float(0.0)
            .float(100.0)
            .float(100.0)
            .int(0);
        o.tag(b'S')
            .utf("cave-1p")
            .int(1)
            .utf("clay")
            .utf("wall")
            .utf("water");
        o.tag(b'N').int(0);
        // line: name, group, closed, reversed, outline, lside, level, scrap, scale, options, points
        o.tag(b'L').utf("wall");
        if v >= 401147 {
            o.utf("");
        }
        o.byte(0).byte(0).int(0);
        if v >= 602055 {
            o.int(-1);
        }
        lvl(&mut o, false);
        if v >= 604088 {
            o.int(0);
        }
        o.utf("").int(2).pt(100.0, 100.0).pt(300.0, 100.0);
        // area: name, group, prefix, count, visible, orientation, level, scrap, scale, options, points
        o.tag(b'A').utf("water");
        if v >= 401147 {
            o.utf("");
        }
        o.utf("a").int(1).byte(1).float(0.0);
        lvl(&mut o, false);
        if v >= 604096 {
            o.int(0);
        }
        if v >= 604098 {
            o.utf("");
        }
        o.int(3).pt(150.0, 150.0).pt(170.0, 150.0).pt(160.0, 170.0);
        // point: x, y, name, group, orientation, scale, level, scrap, text, options
        o.tag(b'P').float(200.0).float(140.0).utf("clay");
        if v >= 401147 {
            o.utf("");
        }
        o.float(0.0).int(-2);
        lvl(&mut o, false);
        if v >= 303066 {
            o.utf("");
        }
        o.utf("");
        // label: x, y, orientation, scale, level, scrap, text, options
        o.tag(b'T').float(120.0).float(130.0);
        if v > 207043 {
            o.float(0.0);
        }
        o.int(0);
        lvl(&mut o, true);
        o.utf("Big Room").utf("");
        o.tag(b'F');
        // station names: x, y, name, level, scrap, x-section type
        for (x, name) in [(100.0, "0"), (300.0, "1")] {
            o.tag(b'X').float(x).float(120.0).utf(name);
            lvl(&mut o, false);
            if v >= 207038 {
                o.int(-1);
            }
        }
        o.tag(b'E');
        o.0
    }

    fn sql(fix: bool) -> String {
        let mut s = String::new();
        s += "INSERT into surveys values( 1, \"cave\", \"2024.06.01\", \"Ann, Bo\", 1080.0000, \"note\", \"0\", 0, 0, 0 );\n";
        // a leg read three times (the other two as blanks), a backwards splay, a deleted shot,
        // and a commented-out leg
        s += "INSERT into shots values( 1, 1, \"0\", \"1\", 10.00, 90.00, 0.00, 0, 0, 0, 0, 1, 0, 0, 0, \"\", 0, 0, 0, 0.00, \"\" );\n";
        s += "INSERT into shots values( 1, 2, \"\", \"\", 10.02, 90.10, 0.10, 0, 0, 0, 0, 2, 0, 1, 0, \"\", 0, 0, 0, 0.00, \"\" );\n";
        s += "INSERT into shots values( 1, 3, \"\", \"\", 9.98, 89.90, -0.10, 0, 0, 0, 0, 2, 0, 1, 0, \"\", 0, 0, 0, 0.00, \"\" );\n";
        s += "INSERT into shots values( 1, 4, \"\", \"1\", 2.00, 0.00, 10.00, 0, 0, 0, 0, 2, 0, 0, 0, \"\", 0, 0, 0, 0.00, \"\" );\n";
        s += "INSERT into shots values( 1, 5, \"1\", \"2\", 99.00, 0.00, 0.00, 0, 0, 0, 0, 1, 0, 0, 1, \"\", 0, 0, 0, 0.00, \"\" );\n";
        s += "INSERT into shots values( 1, 6, \"1\", \"3\", 5.00, 0.00, 0.00, 0, 0, 0, 0, 1, 4, 0, 0, \"\", 0, 0, 0, 0.00, \"\" );\n";
        s += "INSERT into plots values( 1, 1, \"1p\", 1, 0, \"0\", \"\", 0, 0, 1, 0, 0, \"\", \"\", 0, 0, -1.00 );\n";
        s += "INSERT into plots values( 1, 2, \"1s\", 2, 0, \"0\", \"\", 0, 0, 1, 0, 0, \"\", \"\", 0, 0, -1.00 );\n";
        if fix {
            // as TopoDroid 5 wrote it, a comma missing
            s += "INSERT into fixeds values( 1, 1, \"0\", -85.0, 37.0, 300.00, 270.00 \"\", 0, 2, \"\", 0.000000, 0.000000, 0.0, 2, 2 );\n";
        }
        s
    }

    fn archive(fix: bool, version: i32) -> Archive {
        read_archive(&[
            (
                "manifest".into(),
                b"6.4.10 604100\n61\ncave\n2024.06.01\n".to_vec(),
            ),
            ("survey.sql".into(), sql(fix).into_bytes()),
            ("cave-1p.tdr".into(), tdr(version)),
        ])
        .unwrap()
    }

    #[test]
    fn the_survey_is_read_as_topodroid_exports_it() {
        let s = archive(false, 604100).survey;
        assert_eq!(s.cave_name, "cave");
        assert_eq!(s.date, "2024-06-01");
        assert_eq!(s.unit(), "m");
        let legs: Vec<&Shot> = s.shots.iter().filter(|x| !x.splay).collect();
        // the deleted and the commented-out legs are left out
        assert_eq!(legs.len(), 1, "{legs:?}");
        let l = legs[0];
        assert_eq!((l.from.as_str(), l.to.as_str()), ("0", "1"));
        assert!(
            (l.distance - 10.0).abs() < 0.01 && (l.azimuth - 90.0).abs() < 0.01,
            "{l:?}"
        );
        // the splay written only with its far station turns round onto it
        let sp: Vec<&Shot> = s.shots.iter().filter(|x| x.splay).collect();
        assert_eq!(sp.len(), 1);
        assert_eq!(sp[0].from, "1");
        assert!((sp[0].azimuth - 180.0).abs() < 1e-9 && (sp[0].inclination + 10.0).abs() < 1e-9);
        assert!(s.findings().iter().any(|f| f.code == "topodroid-commented"));
        // no declination and no fix: bearings as read, and said so
        assert_eq!(s.declination, 0.0);
        assert!(
            s.findings()
                .iter()
                .any(|f| f.code == "topodroid-no-declination")
        );
        assert!(s.fixed_frame.is_none());
    }

    #[test]
    fn a_fix_places_the_cave_and_gives_its_declination() {
        let s = archive(true, 604100).survey;
        let f = s.fixed.get("0").expect("the fix");
        assert_eq!(f.z, Some(270.0));
        assert_eq!(s.fixed_frame.map(|f| f.zone), Some(16));
        // Kentucky, 2024: a few degrees west
        assert!(
            s.declination < -2.0 && s.declination > -8.0,
            "{}",
            s.declination
        );
        assert_eq!(s.declination_source, "computed");
    }

    #[test]
    fn a_drawing_is_read_in_every_version_layout() {
        for version in [207040, 401100, 501040, 602060, 604090, 604100] {
            let a = archive(false, version);
            let plan = a.plots.iter().find(|p| p.name == "1p").unwrap();
            assert_eq!(plan.projection, "plan");
            let sk = plan.sketch.as_ref().unwrap();
            assert!(sk.findings.is_empty(), "{version}: {:?}", sk.findings);
            let sc = &sk.scraps[0];
            let kinds: Vec<&str> = sc.lines.iter().map(|l| l.kind.as_str()).collect();
            assert_eq!(kinds, ["wall", "border"], "{version}");
            assert_eq!(sc.areas[0].kind, "water");
            assert_eq!(sc.areas[0].line_ids, [sc.lines[1].id.clone().unwrap()]);
            assert_eq!(sc.points.len(), 2, "{version}");
            assert_eq!(sc.points[0].scale.as_deref(), Some("xs"));
            assert_eq!(sc.points[1].text.as_deref(), Some("Big Room"));
            // y turned up, as Therion's are
            assert_eq!(sc.lines[0].segs[0].to, [100.0, -100.0]);
            let ties: Vec<(&str, f64, f64)> = sc
                .stations
                .iter()
                .map(|t| (t.name.as_str(), t.x, t.y))
                .collect();
            assert_eq!(ties, [("0", 100.0, -120.0), ("1", 300.0, -120.0)]);
            // the profile has no file in this archive
            assert!(
                a.plots
                    .iter()
                    .find(|p| p.name == "1s")
                    .unwrap()
                    .sketch
                    .is_none()
            );
        }
    }

    #[test]
    fn a_cut_short_drawing_keeps_what_came_before() {
        let mut b = tdr(604100);
        b.truncate(b.len() - 30);
        let sk = read_tdr(&b, "1p", "plan");
        assert_eq!(sk.findings.len(), 1);
        assert_eq!(sk.scraps[0].lines.len(), 2);
    }

    #[test]
    fn a_zip_without_survey_sql_is_not_one() {
        assert!(!is_archive(&["cave.th", "cave.th2"]));
        assert!(is_archive(&["x/manifest", "x/survey.sql"]));
        assert!(read_archive(&[("manifest".into(), b"6\n61\n".to_vec())]).is_err());
    }
}
