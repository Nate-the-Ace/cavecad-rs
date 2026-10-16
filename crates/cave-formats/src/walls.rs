//! Walls .srv: reader and writer.
//!
//! The reader began as a port of the old app's CsWalls.js and now follows Walls' own rules as Survex
//! worked them out against Walls32.exe (Survex's `src/datain.c`), since Walls' documentation is
//! incomplete: `#Units` in full (`Meters`/`Feet`, `D=`/`S=` per quantity, `A=`/`AB=`/`V=`/`VB=`
//! degrees, grads, mils or percent, `Order=` for polar and for rectangular data, `Decl=`, the
//! `INCA`/`INCAB`/`INCD`/`INCH`/`INCV`/`INCVB` increments, `TypeAB=`/`TypeVB=` corrected or normal,
//! `LRUD=` with its side order, `Case=`, `Prefix=` levels, `RECT`/`CT`, `TAPE=`, `Save`/`Restore`/
//! `Reset`, and `$name=` macros expanded as `$(name)`), names qualified with colons (`A:B1` replaces
//! the innermost prefix level, `::B1` the inner two), feet and inches (`5i6`, `i6`), unit suffixes on
//! any reading (`3m`, `10f`, `45g`, `12p`), degrees:minutes:seconds and quadrant bearings, FS/BS pairs
//! (`100/282`, `--/282`), instrument and target heights, `#Fix` in the rectangular order, `#Segment`
//! and inline `#S` flags (Compass's letters: `P` don't plot, `L` and `X` leave out of the length --
//! `X` marks duplicate data in Walls files -- `S` splay), and `#[ ... #]` blocks (kept, excluded).
//! LRUD in `<...>` or `*...*` with "--" as not measured, and station-only LRUD lines. Read past:
//! variance overrides, `#Flag`, `#Note`, `#Symbol`, `UV=`, `GRID=`, `FLAG=`. Latitude/longitude
//! `#Fix` needs the project's `.REF` and is reported, not guessed.
//!
//! Walls distances default to METRES. The model keeps ONE unit: the one in force at the first
//! measured value wins and later readings convert into it. Walls' LRUD default is the FROM station;
//! the writer declares LRUD=T so what it writes needs no shift.

use crate::js::{JsObj, split_lines, to_fixed, trim};
use crate::model::{
    Fixed, GeoRef, Lrud, Shot, Survey, convert, normalize_azimuth, parse_quadrant,
    unknown_elevation_note,
};
use std::collections::HashMap;

fn is_missing(t: &str) -> bool {
    !t.is_empty() && t.bytes().all(|b| b == b'-')
}

fn all_dashes(t: &str) -> bool {
    !t.is_empty() && t.bytes().all(|b| b == b'-')
}

/// The angle units Walls knows, as degrees per unit (percent is its own case).
#[derive(Clone, Copy, PartialEq)]
enum Angle {
    Deg,
    Grad,
    Mil,
    Percent,
}

impl Angle {
    fn from_word(w: &str, percent_ok: bool) -> Option<Angle> {
        match w.chars().next()?.to_ascii_uppercase() {
            'D' => Some(Angle::Deg),
            'G' => Some(Angle::Grad),
            'M' => Some(Angle::Mil),
            'P' if percent_ok => Some(Angle::Percent),
            _ => None,
        }
    }

    fn degrees(self, v: f64) -> f64 {
        match self {
            Angle::Deg => v,
            Angle::Grad => v * 0.9,
            Angle::Mil => v * 360.0 / 6400.0,
            Angle::Percent => (v / 100.0).atan().to_degrees(),
        }
    }
}

/// Everything `#Units` sets. Cloned on `Save`, put back on `Restore`.
#[derive(Clone)]
struct Options {
    /// Distances (D=) and LRUD sizes (S=): "m" or "ft".
    d_unit: &'static str,
    s_unit: &'static str,
    a: Angle,
    ab: Angle,
    v: Angle,
    vb: Angle,
    /// Polar order (D, A, V) and rectangular order (E, N, U).
    order_ct: Vec<char>,
    order_rect: Vec<char>,
    rect: bool,
    decl: f64,
    inca: f64,
    incab: f64,
    /// INCD and INCH, in their own unit.
    incd: (f64, &'static str),
    inch: (f64, &'static str),
    incv: f64,
    incvb: f64,
    typeab_c: bool,
    typevb_c: bool,
    case: char,
    /// Outermost first: PREFIX3, PREFIX2, PREFIX.
    prefix: [Option<String>; 3],
    lrud_to: bool,
    /// Which side each LRUD position is: indices into L, R, U, D.
    lrud_order: [usize; 4],
    /// Instrument-to-target taping (the default): heights add to the rise.
    tape_it: bool,
    /// Surface-to-surface taping: with no clino, the two heights are depths (diving data).
    tape_ss: bool,
    /// The #Segment letters in force.
    seg: String,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            d_unit: "m",
            s_unit: "m",
            a: Angle::Deg,
            ab: Angle::Deg,
            v: Angle::Deg,
            vb: Angle::Deg,
            order_ct: vec!['D', 'A', 'V'],
            order_rect: vec!['E', 'N', 'U'],
            rect: false,
            decl: 0.0,
            inca: 0.0,
            incab: 0.0,
            incd: (0.0, "m"),
            inch: (0.0, "m"),
            incv: 0.0,
            incvb: 0.0,
            typeab_c: false,
            typevb_c: false,
            case: 'M',
            prefix: [None, None, None],
            lrud_to: false,
            lrud_order: [0, 1, 2, 3],
            tape_it: true,
            tape_ss: false,
            seg: String::new(),
        }
    }
}

/// A distance as Walls writes one: `12.5`, `3m`, `10f`, `5i6` (5 ft 6 in), `i6` (6 in), with the
/// unit in force when it has no suffix. Returns (value, its unit) or `None`.
fn distance(tok: &str, unit: &'static str) -> Option<(f64, &'static str)> {
    let (neg, body) = match tok.as_bytes().first() {
        Some(b'-') => (true, &tok[1..]),
        Some(b'+') => (false, &tok[1..]),
        _ => (false, tok),
    };
    let sign = if neg { -1.0 } else { 1.0 };
    if let Some(inches) = body.strip_prefix(['i', 'I']) {
        let v: f64 = inches.parse().ok()?;
        return Some((sign * v / 12.0, "ft"));
    }
    let n = body
        .bytes()
        .take_while(|b| b.is_ascii_digit() || *b == b'.')
        .count();
    if n == 0 {
        return None;
    }
    let v: f64 = body[..n].parse().ok()?;
    let rest = &body[n..];
    match rest.chars().next().map(|c| c.to_ascii_lowercase()) {
        None => Some((sign * v, unit)),
        Some('m') if rest.len() == 1 => Some((sign * v, "m")),
        Some('f') if rest.len() == 1 => Some((sign * v, "ft")),
        Some('i') if !body[..n].contains('.') => {
            let inches = if rest.len() > 1 {
                rest[1..].parse::<f64>().ok()?
            } else {
                0.0
            };
            Some((sign * (v + inches / 12.0), "ft"))
        }
        _ => None,
    }
}

/// An angle as Walls writes one: `45`, `45.5d`, `50g`, `800m`, `12p` (clino only), `30:15:10`, or a
/// quadrant bearing (`N30E`, compass only). Degrees, or `None`.
fn angle(tok: &str, unit: Angle, compass: bool) -> Option<f64> {
    if compass && tok.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return parse_quadrant(tok);
    }
    let (neg, body) = match tok.as_bytes().first() {
        Some(b'-') => (true, &tok[1..]),
        Some(b'+') => (false, &tok[1..]),
        _ => (false, tok),
    };
    let sign = if neg { -1.0 } else { 1.0 };
    let n = body
        .bytes()
        .take_while(|b| b.is_ascii_digit() || *b == b'.')
        .count();
    if n == 0 {
        return None;
    }
    let v: f64 = body[..n].parse().ok()?;
    let rest = &body[n..];
    if let Some(dms) = rest.strip_prefix(':') {
        let mut parts = dms.split(':');
        let m: f64 = parts
            .next()
            .filter(|s| !s.is_empty())
            .map_or(Ok(0.0), str::parse)
            .ok()?;
        let s: f64 = parts
            .next()
            .filter(|s| !s.is_empty())
            .map_or(Ok(0.0), str::parse)
            .ok()?;
        return Some(sign * (v + m / 60.0 + s / 3600.0));
    }
    let unit = match rest {
        "" => unit,
        r if r.len() == 1 => Angle::from_word(r, !compass)?,
        _ => return None,
    };
    Some(unit.degrees(sign * v))
}

/// A FS/BS pair ("100/282", "--/282", "/282", "100"): (foresight, backsight) in degrees; a part that
/// is missing or "--" is `None`.
fn pair(tok: &str, fore: Angle, back: Angle, compass: bool) -> (Option<f64>, Option<f64>) {
    let mut parts = tok.splitn(2, '/');
    let one = |p: Option<&str>, unit| match p {
        None => None,
        Some(p) if p.is_empty() || is_missing(p) => None,
        Some(p) => angle(p, unit, compass),
    };
    (one(parts.next(), fore), one(parts.next(), back))
}

/// Splits on white space and commas (Walls treats a comma as a blank).
fn fields(s: &str) -> Vec<&str> {
    s.split(|c: char| c == ',' || crate::js::is_space(c))
        .filter(|t| !t.is_empty())
        .collect()
}

/// `$(name)` replaced by its macro's value.
fn expand(line: &str, macros: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(i) = rest.find("$(") {
        out.push_str(&rest[..i]);
        match rest[i + 2..].find(')') {
            Some(j) => {
                let name = &rest[i + 2..i + 2 + j];
                out.push_str(macros.get(name).map_or("", String::as_str));
                rest = &rest[i + 3 + j..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// The `#Units` options of one line, as (name, value) pairs in order. A quoted value is taken
/// whole; TYPEAB/TYPEVB keep their ",tolerance" tail.
fn option_list(s: &str) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    let blank = |c: char| c == ',' || crate::js::is_space(c);
    while i < b.len() {
        if blank(b[i]) {
            i += 1;
            continue;
        }
        if b[i] == ';' {
            break;
        }
        let start = i;
        while i < b.len() && !blank(b[i]) && b[i] != '=' && b[i] != ';' {
            i += 1;
        }
        let name: String = b[start..i].iter().collect();
        let mut j = i;
        while j < b.len() && crate::js::is_space(b[j]) {
            j += 1;
        }
        if j < b.len() && b[j] == '=' {
            i = j + 1;
            while i < b.len() && crate::js::is_space(b[i]) {
                i += 1;
            }
            let value = if i < b.len() && b[i] == '"' {
                let vs = i + 1;
                i = vs;
                while i < b.len() && b[i] != '"' {
                    i += 1;
                }
                let v: String = b[vs..i].iter().collect();
                i += 1;
                v
            } else {
                let keeps_commas =
                    matches!(name.to_ascii_uppercase().as_str(), "TYPEAB" | "TYPEVB");
                let vs = i;
                while i < b.len()
                    && !crate::js::is_space(b[i])
                    && b[i] != ';'
                    && (keeps_commas || b[i] != ',')
                {
                    i += 1;
                }
                b[vs..i].iter().collect()
            };
            out.push((name, Some(value)));
        } else {
            out.push((name, None));
        }
    }
    out
}

/// What reading one `.srv` keeps track of.
struct Reader {
    survey: Survey,
    o: Options,
    saved: Vec<Options>,
    macros: HashMap<String, String>,
    /// station -> (LRUD in the survey's unit, came from a splay line)
    station_lrud: JsObj<(Lrud, bool)>,
    exclude_depth: usize,
    folded_heights: bool,
    /// Stations fixed only by a floated fix so far.
    floated: Vec<String>,
    /// The project's geographic reference: a #Date then sets the declination from the field model,
    /// and every declination also takes out the grid convergence (the fixes are UTM).
    georef: Option<GeoRef>,
}

impl Reader {
    /// A distance in the survey's unit: the distance unit in force at the first one read decides
    /// it, whatever suffix that value carried.
    fn in_survey_unit(&mut self, (v, unit): (f64, &'static str)) -> f64 {
        if self.survey.distance_unit.is_none() {
            self.survey.distance_unit = Some(self.o.d_unit.into());
        }
        convert(v, unit, self.survey.unit())
    }

    fn dist(&mut self, tok: &str, unit: &'static str) -> Option<f64> {
        distance(tok, unit).map(|d| self.in_survey_unit(d))
    }

    /// A station name with the prefixes in force: each colon in it replaces one prefix level, from
    /// the innermost out, and an empty level is left out.
    fn station(&self, name: &str) -> String {
        if name.is_empty() || all_dashes(name) {
            return name.to_string();
        }
        let parts: Vec<&str> = name.split(':').collect();
        let explicit = (parts.len() - 1).min(3);
        let own = parts[parts.len() - 1];
        let own = match self.o.case {
            'U' => own.to_uppercase(),
            'L' => own.to_lowercase(),
            _ => own.to_string(),
        };
        // Leading empty levels are left out; an empty level inside is kept, so "A:::B1" and
        // "A:B1" stay two stations, as they are in Walls.
        let mut levels: Vec<String> = Vec::new();
        for i in 0..3 {
            let level = if i < 3 - explicit {
                self.o.prefix[i].clone()
            } else {
                let p = parts[parts.len() - 1 - explicit + (i - (3 - explicit))];
                (!p.is_empty()).then(|| p.to_string())
            };
            match level {
                Some(l) => levels.push(l),
                None if !levels.is_empty() => levels.push(String::new()),
                None => {}
            }
        }
        levels.push(own);
        levels.join(":")
    }

    fn units(&mut self, args: &str) {
        for (name, value) in option_list(args) {
            let up = name.to_ascii_uppercase();
            let v = value.clone().unwrap_or_default();
            let unit_of = |v: &str| -> Option<&'static str> {
                match v.chars().next()?.to_ascii_uppercase() {
                    'M' => Some("m"),
                    'F' => Some("ft"),
                    _ => None,
                }
            };
            match up.as_str() {
                "M" | "METERS" | "METRES" => (self.o.d_unit, self.o.s_unit) = ("m", "m"),
                "F" | "FEET" => (self.o.d_unit, self.o.s_unit) = ("ft", "ft"),
                "D" | "DISTANCE" => {
                    if let Some(u) = unit_of(&v) {
                        self.o.d_unit = u;
                    }
                }
                "S" => {
                    if let Some(u) = unit_of(&v) {
                        self.o.s_unit = u;
                    }
                }
                "A" | "AZIMUTH" => self.o.a = Angle::from_word(&v, false).unwrap_or(self.o.a),
                "AB" => self.o.ab = Angle::from_word(&v, false).unwrap_or(self.o.ab),
                "V" | "VERTICAL" => self.o.v = Angle::from_word(&v, true).unwrap_or(self.o.v),
                "VB" => self.o.vb = Angle::from_word(&v, true).unwrap_or(self.o.vb),
                "O" | "ORDER" => {
                    let order: Vec<char> = v.to_ascii_uppercase().chars().collect();
                    if order.iter().all(|c| "DAV".contains(*c)) && (2..=3).contains(&order.len()) {
                        self.o.order_ct = order;
                    } else if order.iter().all(|c| "ENU".contains(*c))
                        && (2..=3).contains(&order.len())
                    {
                        self.o.order_rect = order;
                    }
                }
                "DECL" | "DECLINATION" => {
                    let d = angle(&v, Angle::Deg, false).unwrap_or(0.0)
                        - self.georef.map_or(0.0, |g| g.convergence());
                    self.o.decl = d;
                    if self.survey.declination_source.is_empty() {
                        self.survey.declination = d;
                        self.survey.declination_source = "file".into();
                    }
                }
                "INCA" => self.o.inca = angle(&v, self.o.a, false).unwrap_or(0.0),
                "INCAB" => self.o.incab = angle(&v, self.o.ab, false).unwrap_or(0.0),
                "INCV" => self.o.incv = angle(&v, self.o.v, false).unwrap_or(0.0),
                "INCVB" => self.o.incvb = angle(&v, self.o.vb, false).unwrap_or(0.0),
                "INCD" => self.o.incd = distance(&v, self.o.d_unit).unwrap_or((0.0, "m")),
                "INCH" => self.o.inch = distance(&v, self.o.d_unit).unwrap_or((0.0, "m")),
                "TYPEAB" => self.o.typeab_c = v.to_ascii_uppercase().starts_with('C'),
                "TYPEVB" => self.o.typevb_c = v.to_ascii_uppercase().starts_with('C'),
                "CASE" => self.o.case = v.chars().next().map_or('M', |c| c.to_ascii_uppercase()),
                "P" | "PREFIX" | "PREFIX1" => self.o.prefix[2] = value.filter(|v| !v.is_empty()),
                "PREFIX2" => self.o.prefix[1] = value.filter(|v| !v.is_empty()),
                "PREFIX3" => self.o.prefix[0] = value.filter(|v| !v.is_empty()),
                "RECT" => {
                    if value.is_none() {
                        self.o.rect = true;
                    }
                    // RECT=<bearing> rotates rectangular data; read past
                }
                "CT" => self.o.rect = false,
                "LRUD" => {
                    let (assoc, order) = v.split_once(':').unwrap_or((&v, ""));
                    self.o.lrud_to = assoc.to_ascii_uppercase().starts_with('T');
                    let idx: Vec<usize> = order
                        .to_ascii_uppercase()
                        .chars()
                        .filter_map(|c| "LRUD".find(c))
                        .collect();
                    if idx.len() == 4 {
                        self.o.lrud_order = [idx[0], idx[1], idx[2], idx[3]];
                    }
                }
                "TAPE" => {
                    let t = v.to_ascii_uppercase();
                    self.o.tape_it = t == "IT";
                    self.o.tape_ss = t == "SS";
                }
                "RESET" => self.o = Options::default(),
                "SAVE" => self.saved.push(self.o.clone()),
                "RESTORE" => {
                    if let Some(o) = self.saved.pop() {
                        self.o = o;
                    }
                }
                _ if name.starts_with('$') && name.len() > 1 => {
                    self.macros.insert(name[1..].to_string(), v);
                }
                _ => {}
            }
        }
    }

    fn record_lrud(&mut self, station: &str, l: &Lrud, from_splay: bool) {
        if let Some((_, prev_splay)) = self.station_lrud.get(station)
            && from_splay
            && !prev_splay
        {
            return; // a real shot's reading beats a splay line's
        }
        self.station_lrud.set(
            station,
            (Lrud::of(l.left, l.right, l.up, l.down), from_splay),
        );
    }

    /// The inside of `<...>` / `*...*`: four sizes in the LRUD order, "--" not measured.
    fn lrud(&mut self, inner: &str) -> Lrud {
        let mut sides = [None; 4];
        for (k, p) in fields(inner).into_iter().take(4).enumerate() {
            if !is_missing(p) {
                sides[self.o.lrud_order[k]] = self.dist(p, self.o.s_unit);
            }
        }
        Lrud::of(sides[0], sides[1], sides[2], sides[3])
    }

    fn fix(&mut self, args: &str) {
        // a "(?...)" variance floats the fix: any exact fix of the station beats it
        let floated = args
            .split(['/', ';'])
            .next()
            .unwrap_or("")
            .split_once('(')
            .is_some_and(|(_, v)| v.contains('?'));
        let f = fields(args.split(['(', '/', ';']).next().unwrap_or(""));
        if f.len() < 3 {
            return;
        }
        let name = self.station(f[0]);
        if f[1..]
            .iter()
            .any(|t| t.starts_with(['N', 'S', 'E', 'W', 'n', 's', 'e', 'w']))
        {
            self.survey.add_finding(
                "warning",
                "walls-latlong-fix",
                format!("{name} is fixed by latitude and longitude, which needs the project's geographic reference to place; it was left unfixed."),
            );
            return;
        }
        let mut xyz = [None, None, Some(0.0)];
        for (axis, tok) in self.o.order_rect.clone().into_iter().zip(&f[1..]) {
            let v = self.dist(tok, self.o.d_unit);
            match axis {
                'E' => xyz[0] = v,
                'N' => xyz[1] = v,
                _ => xyz[2] = v.or(Some(0.0)),
            }
        }
        if let (Some(x), Some(y)) = (xyz[0], xyz[1]) {
            // a repeated exact fix keeps the first, as Walls does
            let replace =
                !self.survey.fixed.has(&name) || (!floated && self.floated.contains(&name));
            if replace {
                self.survey
                    .fixed
                    .set(&name, Fixed::at(x, y, xyz[2].unwrap_or(0.0)));
                if floated {
                    self.floated.push(name);
                } else {
                    self.floated.retain(|n| *n != name);
                }
            }
        }
    }

    fn data_line(&mut self, raw: &str) {
        let mut line = raw.to_string();
        // the comment, and where the note comes from
        let notes = match line.find(';') {
            Some(i) => {
                let n = trim(&line[i + 1..]).to_string();
                line.truncate(i);
                n
            }
            None => String::new(),
        };
        // inline segment: #S, #Seg, #Segment, taking the rest of the line
        let mut seg = self.o.seg.clone();
        if let Some(i) = find_segment(&line) {
            let word = fields(&line[i..])
                .get(1)
                .map(|w| w.to_string())
                .unwrap_or_default();
            seg = segment_flags(&word).unwrap_or_default();
            line.truncate(i);
        }
        // LRUD: the first <...> or *...* block
        let mut lrud = None;
        if let Some((s, e, inner)) = lrud_block(&line) {
            lrud = Some(self.lrud(&inner));
            line = format!("{} {}", &line[..s], &line[e..]);
        }
        // variance overrides "(...)" are read past
        while let Some(s) = line.find('(') {
            let e = line[s..].find(')').map_or(line.len(), |e| s + e + 1);
            line.replace_range(s..e, " ");
        }
        let f = fields(&line);
        if f.is_empty() {
            return;
        }
        if f.len() == 1 {
            if let Some(l) = &lrud {
                let st = self.station(f[0]);
                self.record_lrud(&st, l, false);
            }
            return;
        }
        let from_dash = all_dashes(f[0]);
        let to_dash = all_dashes(f[1]);
        let mut shot = Shot {
            splay: from_dash || to_dash || seg.contains('S'),
            notes,
            exclude_from_plot: seg.contains('P'),
            exclude_from_length: seg.contains('L'),
            exclude_from_all: self.exclude_depth > 0,
            ..Shot::default()
        };
        // the measurements, in the survey's unit and degrees
        let measured = &f[2..];
        let (mut d_m, mut az, mut back_az, mut inc, mut back_inc) = (None, None, None, None, None);
        let mut omitted_az = false;
        let mut used = 0;
        if self.o.rect {
            let mut e = [0.0f64; 3];
            for (axis, tok) in self.o.order_rect.clone().iter().zip(measured) {
                let v = self.dist(tok, self.o.d_unit).unwrap_or(0.0);
                e["ENU".find(*axis).unwrap_or(2)] = v;
                used += 1;
            }
            let h = e[0].hypot(e[1]);
            d_m = Some(h.hypot(e[2]));
            az = if h > 0.0 {
                Some(normalize_azimuth(e[0].atan2(e[1]).to_degrees()))
            } else {
                None
            };
            omitted_az = h == 0.0;
            inc = Some(e[2].atan2(h).to_degrees());
            // rectangular data is already true north: no declination
            if let Some(a) = az.as_mut() {
                *a = normalize_azimuth(*a - self.o.decl);
            }
        } else {
            for (field, tok) in self.o.order_ct.clone().iter().zip(measured) {
                used += 1;
                match field {
                    'D' => {
                        if !is_missing(tok) {
                            d_m = self.dist(tok, self.o.d_unit);
                        }
                    }
                    'A' => {
                        let (fs, bs) = pair(tok, self.o.a, self.o.ab, true);
                        az = fs;
                        back_az = bs;
                    }
                    _ => {
                        let (fs, bs) = pair(tok, self.o.v, self.o.vb, false);
                        inc = fs;
                        back_inc = bs;
                    }
                }
            }
        }
        // instrument and target heights after the readings
        let mut rise = 0.0;
        if !self.o.rect {
            // "--" for a height is 0, as Walls reads it
            let height = |r: &mut Self, t: Option<&&str>| match t {
                Some(t) if is_missing(t) => Some(0.0),
                Some(t) => r.dist(t, r.o.d_unit),
                None => None,
            };
            let ih = height(self, measured.get(used));
            let th = height(self, measured.get(used + 1));
            if ih.is_some() && self.o.tape_it {
                rise += ih.unwrap_or(0.0) - th.unwrap_or(0.0);
            } else if self.o.tape_ss && inc.is_none() && back_inc.is_none() && ih.is_some() {
                // diving data: the heights are depths, and the tape the slope between them
                let dz = ih.unwrap_or(0.0) - th.unwrap_or(0.0);
                let d = d_m.unwrap_or(0.0);
                inc = Some(if d > 0.0 {
                    (dz / d).clamp(-1.0, 1.0).asin().to_degrees()
                } else {
                    0.0
                });
            }
        }
        if self.o.inch.0 != 0.0 {
            rise += self.in_survey_unit(self.o.inch);
        }

        // corrections (none on a zero-length shot, only INCD on a vertical one)
        let d = d_m.unwrap_or(0.0);
        let vertical = inc.is_some_and(|i| i.abs() >= 90.0);
        let d = if d != 0.0 && self.o.incd.0 != 0.0 {
            d + self.in_survey_unit(self.o.incd)
        } else {
            d
        };
        if d != 0.0 && !vertical {
            az = az.map(|a| a + self.o.inca);
            back_az = back_az.map(|b| b + self.o.incab);
            inc = inc.map(|i| i + self.o.incv);
            back_inc = back_inc.map(|b| b + self.o.incvb);
        }
        // the model keeps backsights unreversed: corrected ones are turned back
        if self.o.typeab_c {
            back_az = back_az.map(|b| normalize_azimuth(b + 180.0));
        }
        if self.o.typevb_c {
            back_inc = back_inc.map(|b| -b);
        }
        // a missing foresight is filled from the backsight, as the old reader did (the
        // geometry is the same: the two then agree)
        if az.is_none() {
            az = back_az.map(|b| normalize_azimuth(b + 180.0));
        }
        if inc.is_none() {
            inc = back_inc.map(|b| -b);
        }
        let inc_used = inc;
        if az.is_none() && back_az.is_none() {
            if inc_used.is_some_and(|i| i.abs() >= 89.999) || d == 0.0 || omitted_az {
                shot.azimuth_omitted = true;
            } else {
                self.survey.add_finding(
                    "warning",
                    "bearing-omitted-not-plumb",
                    format!("Leg {} to {} has no bearing and is not vertical, so it cannot be placed -- anything beyond it is unconnected.", f[0], f[1]),
                );
            }
        }
        // heights change the rise between the stations: folded into the shot's own readings
        if rise != 0.0 && d > 0.0 {
            let i = inc_used.unwrap_or(0.0).to_radians();
            let (h, v) = (d * i.cos(), d * i.sin() + rise);
            let new_inc = v.atan2(h).to_degrees();
            d_m = Some(h.hypot(v));
            inc = Some(new_inc);
            back_inc = back_inc.map(|_| -new_inc);
            self.folded_heights = true;
        } else {
            d_m = Some(d);
        }

        let mut az_v = az.map_or(f64::NAN, normalize_azimuth);
        let mut inc_v = inc.unwrap_or(if back_inc.is_some() { f64::NAN } else { 0.0 });
        if az.is_none() && back_az.is_none() {
            az_v = if shot.azimuth_omitted { 0.0 } else { f64::NAN };
        }
        if from_dash {
            // "- A1 ...": a wall point INTO the named station, turned to hang from it
            shot.from = self.station(f[1]);
            az_v = normalize_azimuth(az_v + 180.0);
            inc_v = -inc_v;
            back_az = back_az.map(|b| normalize_azimuth(b + 180.0));
            back_inc = back_inc.map(|b| -b);
        } else {
            shot.from = self.station(f[0]);
            shot.to = if to_dash {
                String::new()
            } else {
                self.station(f[1])
            };
        }
        shot.distance = d_m.unwrap_or(0.0);
        shot.azimuth = normalize_azimuth(az_v + self.o.decl);
        shot.back_azimuth = back_az.map(|b| normalize_azimuth(b + self.o.decl));
        shot.declination = Some(self.o.decl);
        shot.inclination = inc_v;
        shot.back_inclination = back_inc;
        if let Some(l) = &lrud {
            if self.o.lrud_to && !shot.splay {
                (shot.left, shot.right, shot.up, shot.down) = (l.left, l.right, l.up, l.down);
            } else {
                let st = shot.from.clone();
                self.record_lrud(&st, l, shot.splay);
            }
        }
        self.survey.shots.push(shot);
    }
}

/// Where an inline `#S`, `#Seg` or `#Segment` starts on a data line.
fn find_segment(line: &str) -> Option<usize> {
    let lower = line.to_ascii_lowercase();
    let mut from = 0;
    while let Some(off) = lower[from..].find('#') {
        let at = from + off;
        let rest = &lower[at + 1..];
        let word: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        if matches!(word.as_str(), "s" | "seg" | "segment") {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

/// A segment's Compass flag letters, when it is made of them: `C` is dropped (Walls' adjustment
/// ignores it) and `X` -- duplicate data, in Walls files -- becomes `L`.
fn segment_flags(word: &str) -> Option<String> {
    let w = word.trim_start_matches(['/', '\\']).to_ascii_uppercase();
    if w.is_empty() || !w.chars().all(|c| "LSPXC".contains(c)) {
        return None;
    }
    Some(w.replace('C', "").replace('X', "L"))
}

/// `<([^>]*)>|\*([^*]*)\*`: (start, end, inner) of the first.
fn lrud_block(line: &str) -> Option<(usize, usize, String)> {
    let lt = line.find('<').and_then(|s| {
        line[s + 1..]
            .find('>')
            .map(|e| (s, s + 1 + e + 1, line[s + 1..s + 1 + e].to_string()))
    });
    let star = line.find('*').and_then(|s| {
        line[s + 1..]
            .find('*')
            .map(|e| (s, s + 1 + e + 1, line[s + 1..s + 1 + e].to_string()))
    });
    match (lt, star) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

/// `#Date`: ISO (`2024-07-10`, `2024/07/10`) or US (`07/10/2024`, `7-10-24` as 1924), as YYYY-MM-DD.
fn walls_date(t: &str) -> String {
    let nums: Vec<u32> = t
        .split(['-', '/'])
        .take(3)
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    match nums[..] {
        [y, m, d] if y >= 100 => format!("{y:04}-{m:02}-{d:02}"),
        [m, d, y] => format!("{:04}-{m:02}-{d:02}", if y < 100 { y + 1900 } else { y }),
        _ => t.to_string(),
    }
}

/// Reads a Walls .srv file.
pub fn parse(content: &str) -> Survey {
    parse_with(content, None)
}

/// Reads a Walls .srv file in a project that has a geographic reference (`.REF`).
pub fn parse_with(content: &str, georef: Option<GeoRef>) -> Survey {
    let mut r = Reader {
        georef,
        floated: Vec::new(),
        survey: Survey {
            distance_unit: None,
            ..Survey::default()
        },
        o: Options::default(),
        saved: Vec::new(),
        macros: HashMap::new(),
        station_lrud: JsObj::new(),
        exclude_depth: 0,
        folded_heights: false,
    };
    for raw in split_lines(content) {
        let line = trim(&raw);
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(d) = line.strip_prefix('#') {
            if d.starts_with('[') {
                r.exclude_depth += 1;
                continue;
            }
            if d.starts_with(']') {
                r.exclude_depth = r.exclude_depth.saturating_sub(1);
                continue;
            }
            let d = crate::js::trim_start(d);
            let word: String = d
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            let rest = &d[word.len()..];
            let directive = word.to_ascii_lowercase();
            let args = if directive == "fix" {
                rest.to_string()
            } else {
                expand(rest, &r.macros)
            };
            match directive.as_str() {
                "u" | "units" => r.units(&args),
                "fix" => r.fix(&args),
                "date" => {
                    let t = fields(args.split(';').next().unwrap_or(""))
                        .first()
                        .map(|s| s.to_string());
                    if let Some(t) = t {
                        r.survey.date = walls_date(&t);
                        // with a geographic reference a date sets the declination, as a DECL= would
                        if let Some(g) = r.georef
                            && let Some(d) = g.declination_on(&r.survey.date)
                        {
                            r.o.decl = d - g.convergence();
                            if r.survey.declination_source.is_empty()
                                || r.survey.declination_source == "igrf"
                            {
                                r.survey.declination = r.o.decl;
                                r.survey.declination_source = "igrf".into();
                            }
                        }
                    }
                }
                "prefix" | "prefix1" | "prefix2" | "prefix3" => {
                    let level = match directive.as_str() {
                        "prefix2" => 1,
                        "prefix3" => 0,
                        _ => 2,
                    };
                    let v = fields(args.split(';').next().unwrap_or(""))
                        .first()
                        .map(|s| s.to_string());
                    r.o.prefix[level] = v;
                }
                "s" | "seg" | "segment" => {
                    let w = fields(args.split(';').next().unwrap_or(""))
                        .first()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    r.o.seg = segment_flags(&w).unwrap_or_default();
                }
                _ => {} // #Flag, #Note, #Symbol: read past
            }
            continue;
        }
        r.data_line(line);
    }

    let mut survey = r.survey;
    for s in &mut survey.shots {
        if !s.splay
            && !s.any_lrud()
            && let Some((l, _)) = r.station_lrud.get(&s.to)
        {
            (s.left, s.right, s.up, s.down) = (l.left, l.right, l.up, l.down);
        }
    }
    if let Some((l, _)) = survey
        .shots
        .first()
        .and_then(|s| r.station_lrud.get(&s.from))
    {
        survey.start_lrud = Some(Lrud::of(l.left, l.right, l.up, l.down));
    }
    if r.folded_heights {
        survey.add_finding(
            "info",
            "walls-heights",
            "Instrument and target heights (or INCH=) changed the rise of some shots; they are folded into those shots' length and inclination.".into(),
        );
    }
    if survey.distance_unit.is_none() {
        survey.distance_unit = Some(r.o.d_unit.into());
    }
    survey.ensure_trips();
    survey
}

/// Writes a survey as a Walls .srv file (LRUD=T, one file-wide declination: trip 0's).
pub fn write(survey: &mut Survey) -> String {
    survey.ensure_trips();
    let mut out: Vec<String> = Vec::new();
    let decl = if survey.declination.is_nan() {
        0.0
    } else {
        survey.declination
    };
    let unit_word = if survey.distance_unit.as_deref() == Some("ft") {
        "Feet"
    } else {
        "Meters"
    };
    let title = if !survey.cave_name.is_empty() {
        survey.cave_name.as_str()
    } else if !survey.name.is_empty() {
        survey.name.as_str()
    } else {
        "Cave survey"
    };
    out.push(format!("; {title}"));
    if !survey.date.is_empty() {
        out.push(format!("#Date {}", survey.date));
    }
    out.push(format!(
        "#Units {unit_word} Order=DAV LRUD=T{}",
        if decl != 0.0 {
            format!(" Decl={}", to_fixed(decl, 2))
        } else {
            String::new()
        }
    ));
    for (name, f) in survey.fixed.iter() {
        let fz = f.z();
        if fz.is_none() {
            out.push(format!("; {}", unknown_elevation_note(name)));
        }
        out.push(format!(
            "#Fix {name} {} {} {}",
            to_fixed(f.x.unwrap_or(f64::NAN), 2),
            to_fixed(f.y.unwrap_or(f64::NAN), 2),
            to_fixed(fz.unwrap_or(0.0), 2)
        ));
    }
    let fmt = |v: Option<f64>| v.map_or_else(|| "--".to_string(), |x| to_fixed(x, 2));
    let block = |l: Option<f64>, r: Option<f64>, u: Option<f64>, d: Option<f64>| {
        format!("<{},{},{},{}>", fmt(l), fmt(r), fmt(u), fmt(d))
    };
    if let (Some(sl), Some(first)) = (&survey.start_lrud, survey.shots.first())
        && sl.any()
    {
        out.push(format!(
            "{}\t{}",
            first.from,
            block(sl.left, sl.right, sl.up, sl.down)
        ));
    }
    let mut in_exclude = false;
    for s in &survey.shots {
        if s.exclude_from_all && !in_exclude {
            out.push("#[ excluded shots".into());
            in_exclude = true;
        } else if !s.exclude_from_all && in_exclude {
            out.push("#]".into());
            in_exclude = false;
        }
        let mut az = if s.azimuth.is_finite() {
            to_fixed(normalize_azimuth(s.azimuth - decl), 2)
        } else {
            "--".into()
        };
        if let Some(b) = s.back_azimuth {
            az += &format!("/{}", to_fixed(normalize_azimuth(b - decl), 2));
        }
        let mut inc = if s.inclination.is_finite() {
            to_fixed(s.inclination, 2)
        } else {
            "--".into()
        };
        if let Some(b) = s.back_inclination {
            inc += &format!("/{}", to_fixed(b, 2));
        }
        let mut line = format!(
            "{}\t{}\t{}\t{az}\t{inc}",
            s.from,
            if s.splay { "-" } else { &s.to },
            to_fixed(s.distance, 2)
        );
        if s.any_lrud() {
            line += &format!("\t{}", block(s.left, s.right, s.up, s.down));
        }
        let seg = format!(
            "{}{}",
            if s.exclude_from_plot { "P" } else { "" },
            if s.exclude_from_length { "L" } else { "" }
        );
        if !seg.is_empty() {
            line += &format!("\t#S {seg}");
        }
        if !s.notes.is_empty() {
            line += &format!("\t; {}", s.notes);
        }
        out.push(line);
    }
    if in_exclude {
        out.push("#]".into());
    }
    out.join("\n") + "\n"
}
