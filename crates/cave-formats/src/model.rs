//! The survey every reader produces and every writer consumes: the old app's `CsModel` shape, field
//! for field, so a survey read here serializes to the same JSON the old readers produced.
//!
//! Conventions (suite-wide): azimuth is degrees clockwise from north, TRUE (declination already
//! added); declination is positive east; inclination is degrees, positive up. Distances, LRUD and
//! fixed coordinates are in the survey's own `distance_unit` ("ft" or "m"). An LRUD of `None` was not
//! measured; `*_open` says the passage was open that way (no wall), which is not the same as 0.

use crate::js::JsObj;
use serde::{Deserialize, Deserializer, Serialize};

/// `null` (or a missing field) reads as the type's default -- JavaScript's surveys were built by hand
/// in places, and `undefined` and `null` meant "not set" there.
fn nd<'de, D: Deserializer<'de>, T: Deserialize<'de> + Default>(d: D) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// A number that may be `null` in the JSON (JavaScript writes NaN as `null`).
fn num<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.unwrap_or(f64::NAN))
}

/// The passage dimensions at a station.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[allow(missing_docs)]
pub struct Lrud {
    pub left: Option<f64>,
    pub right: Option<f64>,
    pub up: Option<f64>,
    pub down: Option<f64>,
    /// Every reading when a side has more than one (a ledge and the wall beyond it).
    pub left_all: Option<Vec<f64>>,
    pub right_all: Option<Vec<f64>>,
    pub up_all: Option<Vec<f64>>,
    pub down_all: Option<Vec<f64>>,
    #[serde(deserialize_with = "nd")]
    pub left_open: bool,
    #[serde(deserialize_with = "nd")]
    pub right_open: bool,
    #[serde(deserialize_with = "nd")]
    pub up_open: bool,
    #[serde(deserialize_with = "nd")]
    pub down_open: bool,
}

impl Lrud {
    /// Just the four numbers.
    pub fn of(left: Option<f64>, right: Option<f64>, up: Option<f64>, down: Option<f64>) -> Self {
        Lrud {
            left,
            right,
            up,
            down,
            ..Lrud::default()
        }
    }

    /// Whether any of the four sides has a number.
    pub fn any(&self) -> bool {
        self.left.is_some() || self.right.is_some() || self.up.is_some() || self.down.is_some()
    }
}

/// One measured shot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[allow(missing_docs)]
pub struct Shot {
    #[serde(deserialize_with = "nd")]
    pub from: String,
    /// Empty for a splay with no named end.
    #[serde(deserialize_with = "nd")]
    pub to: String,
    #[serde(deserialize_with = "num")]
    pub distance: f64,
    #[serde(deserialize_with = "num")]
    pub azimuth: f64,
    #[serde(deserialize_with = "num")]
    pub inclination: f64,
    /// The backsight compass, true like `azimuth` but not reversed.
    pub back_azimuth: Option<f64>,
    pub back_inclination: Option<f64>,
    /// No bearing was sighted (a plumbed shot); `azimuth` is a stand-in 0.
    #[serde(deserialize_with = "nd")]
    pub azimuth_omitted: bool,
    pub left: Option<f64>,
    pub right: Option<f64>,
    pub up: Option<f64>,
    pub down: Option<f64>,
    pub left_all: Option<Vec<f64>>,
    pub right_all: Option<Vec<f64>>,
    pub up_all: Option<Vec<f64>>,
    pub down_all: Option<Vec<f64>>,
    #[serde(deserialize_with = "nd")]
    pub left_open: bool,
    #[serde(deserialize_with = "nd")]
    pub right_open: bool,
    #[serde(deserialize_with = "nd")]
    pub up_open: bool,
    #[serde(deserialize_with = "nd")]
    pub down_open: bool,
    #[serde(deserialize_with = "nd")]
    pub splay: bool,
    #[serde(deserialize_with = "nd")]
    pub exclude_from_plot: bool,
    #[serde(deserialize_with = "nd")]
    pub exclude_from_all: bool,
    #[serde(deserialize_with = "nd")]
    pub exclude_from_length: bool,
    #[serde(deserialize_with = "nd")]
    pub no_adjust: bool,
    #[serde(deserialize_with = "nd")]
    pub notes: String,
    /// Index into [`Survey::trips`].
    #[serde(deserialize_with = "nd")]
    pub trip: usize,
    /// The declination added to this shot's azimuths, when it differs from (or must not be inferred
    /// from) its trip's.
    pub declination: Option<f64>,
}

impl Default for Shot {
    fn default() -> Self {
        Shot {
            from: String::new(),
            to: String::new(),
            distance: 0.0,
            azimuth: 0.0,
            inclination: 0.0,
            back_azimuth: None,
            back_inclination: None,
            azimuth_omitted: false,
            left: None,
            right: None,
            up: None,
            down: None,
            left_all: None,
            right_all: None,
            up_all: None,
            down_all: None,
            left_open: false,
            right_open: false,
            up_open: false,
            down_open: false,
            splay: false,
            exclude_from_plot: false,
            exclude_from_all: false,
            exclude_from_length: false,
            no_adjust: false,
            notes: String::new(),
            trip: 0,
            declination: None,
        }
    }
}

impl Shot {
    /// Whether any of the four sides has a number.
    pub fn any_lrud(&self) -> bool {
        self.left.is_some() || self.right.is_some() || self.up.is_some() || self.down.is_some()
    }

    /// Whether either backsight reading is there.
    pub fn has_backsight(&self) -> bool {
        self.back_azimuth.is_some() || self.back_inclination.is_some()
    }
}

/// One survey trip: who, when, and the declination its compass readings were corrected by.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[allow(missing_docs)]
pub struct Trip {
    #[serde(deserialize_with = "nd")]
    pub name: String,
    #[serde(deserialize_with = "nd")]
    pub date: String,
    #[serde(deserialize_with = "nd")]
    pub team: String,
    #[serde(deserialize_with = "nd")]
    pub instruments: String,
    #[serde(deserialize_with = "nd")]
    pub declination: f64,
    #[serde(deserialize_with = "nd")]
    pub declination_source: String,
    pub distance_unit: Option<String>,
    #[serde(deserialize_with = "nd")]
    pub start_note: String,
    pub start_lrud: Option<Lrud>,
}

impl Default for Trip {
    fn default() -> Self {
        Trip {
            name: String::new(),
            date: String::new(),
            team: String::new(),
            instruments: String::new(),
            declination: 0.0,
            declination_source: String::new(),
            distance_unit: Some("ft".into()),
            start_note: String::new(),
            start_lrud: None,
        }
    }
}

impl Trip {
    /// A trip is identified by its date and team.
    pub fn fingerprint(&self) -> String {
        format!("{}|{}", self.date, self.team)
    }
}

/// A station whose position is known outright.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(missing_docs)]
pub struct Fixed {
    pub x: Option<f64>,
    pub y: Option<f64>,
    /// `None` when the elevation is unknown -- which is not elevation 0.
    pub z: Option<f64>,
}

impl Fixed {
    /// A fixed point with all three coordinates.
    pub fn at(x: f64, y: f64, z: f64) -> Self {
        Fixed {
            x: Some(x),
            y: Some(y),
            z: Some(z),
        }
    }

    /// The elevation, or `None` when it is unknown (absent or not a finite number).
    pub fn z(&self) -> Option<f64> {
        self.z.filter(|z| z.is_finite())
    }
}

/// Something a reader noticed about the file that the person should be told.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct Finding {
    pub severity: String,
    pub shot_index: i64,
    pub code: String,
    pub message: String,
}

/// A whole survey.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[allow(missing_docs)]
pub struct Survey {
    /// Trip 0's name, mirrored (see [`Survey::ensure_trips`]).
    #[serde(deserialize_with = "nd")]
    pub name: String,
    /// The cave, as opposed to the trip.
    #[serde(deserialize_with = "nd")]
    pub cave_name: String,
    #[serde(deserialize_with = "nd")]
    pub date: String,
    #[serde(deserialize_with = "nd")]
    pub team: String,
    #[serde(deserialize_with = "nd")]
    pub instruments: String,
    #[serde(deserialize_with = "nd")]
    pub declination: f64,
    #[serde(deserialize_with = "nd")]
    pub declination_source: String,
    /// "ft" or "m"; `None` only while a reader has not yet seen a distance.
    pub distance_unit: Option<String>,
    #[serde(deserialize_with = "nd")]
    pub shots: Vec<Shot>,
    #[serde(deserialize_with = "nd")]
    pub fixed: JsObj<Fixed>,
    #[serde(deserialize_with = "nd")]
    pub start_note: String,
    /// The first station's passage dimensions (it has no shot arriving at it to carry them).
    pub start_lrud: Option<Lrud>,
    #[serde(deserialize_with = "nd")]
    pub trips: Vec<Trip>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_findings: Option<Vec<Finding>>,
}

impl Default for Survey {
    fn default() -> Self {
        Survey {
            name: String::new(),
            cave_name: String::new(),
            date: String::new(),
            team: String::new(),
            instruments: String::new(),
            declination: 0.0,
            declination_source: String::new(),
            distance_unit: Some("ft".into()),
            shots: Vec::new(),
            fixed: JsObj::new(),
            start_note: String::new(),
            start_lrud: None,
            trips: Vec::new(),
            parse_findings: None,
        }
    }
}

impl Survey {
    /// The distance unit, "ft" when none was ever set.
    pub fn unit(&self) -> &str {
        self.distance_unit.as_deref().unwrap_or("ft")
    }

    /// Records a finding once (the same code and message twice is one finding).
    pub fn add_finding(&mut self, severity: &str, code: &str, message: String) {
        let list = self.parse_findings.get_or_insert_with(Vec::new);
        if list.iter().any(|f| f.code == code && f.message == message) {
            return;
        }
        list.push(Finding {
            severity: severity.into(),
            shot_index: -1,
            code: code.into(),
            message,
        });
    }

    /// The findings the reader recorded.
    pub fn findings(&self) -> &[Finding] {
        self.parse_findings.as_deref().unwrap_or(&[])
    }

    /// Makes sure there is at least one trip (built from the top-level fields when there is none),
    /// then mirrors trip 0 back onto the top-level fields. Trip 0's record is the truth: anything a
    /// reader writes onto the top level after the first trip exists is overwritten here, which is why
    /// the readers write trip 0 directly once it does.
    pub fn ensure_trips(&mut self) {
        if self.trips.is_empty() {
            self.trips.push(Trip {
                name: self.name.clone(),
                date: self.date.clone(),
                team: self.team.clone(),
                instruments: self.instruments.clone(),
                declination: self.declination,
                declination_source: self.declination_source.clone(),
                distance_unit: self.distance_unit.clone(),
                start_note: self.start_note.clone(),
                start_lrud: self.start_lrud.clone(),
            });
        }
        let t0 = &mut self.trips[0];
        self.name = t0.name.clone();
        self.date = t0.date.clone();
        self.team = t0.team.clone();
        self.instruments = t0.instruments.clone();
        self.declination = t0.declination;
        self.declination_source = t0.declination_source.clone();
        self.distance_unit = t0.distance_unit.clone();
        self.start_note = t0.start_note.clone();
        if t0.start_lrud.is_none() && self.start_lrud.is_some() {
            t0.start_lrud = self.start_lrud.clone();
        } else {
            self.start_lrud = t0.start_lrud.clone();
        }
    }

    /// Sets the first station's dimensions on the survey and on trip 0.
    pub fn set_start_lrud(&mut self, lrud: Option<Lrud>) {
        self.start_lrud = lrud;
        if let Some(t0) = self.trips.first_mut() {
            t0.start_lrud = self.start_lrud.clone();
        }
    }

    /// The trip a shot belongs to.
    pub fn trip_of(&self, shot: &Shot) -> Option<&Trip> {
        self.trips.get(shot.trip)
    }

    /// Trip `index` is the empty stand-in [`Survey::ensure_trips`] made: no date, no team, no shots.
    fn is_placeholder_trip(&self, index: usize) -> bool {
        match self.trips.get(index) {
            Some(t) if t.fingerprint() == "|" => !self.shots.iter().any(|s| s.trip == index),
            _ => false,
        }
    }

    /// The index of the trip with `record`'s date and team, adding it when there is none. A
    /// matching trip takes `record`'s declination when they differ (the shots keep their own).
    pub fn trip_id_for(&mut self, record: Trip) -> usize {
        self.ensure_trips();
        let fp = record.fingerprint();
        if let Some(i) = self.trips.iter().position(|t| t.fingerprint() == fp) {
            let existing = &mut self.trips[i];
            let was = if existing.declination.is_nan() {
                0.0
            } else {
                existing.declination
            };
            let now = if record.declination.is_nan() {
                0.0
            } else {
                record.declination
            };
            if (was - now).abs() >= 5e-5 {
                existing.declination = now;
                if !record.declination_source.is_empty() {
                    existing.declination_source = record.declination_source;
                }
            }
            return i;
        }
        if self.is_placeholder_trip(0) {
            self.trips[0] = record;
            self.ensure_trips();
            return 0;
        }
        self.trips.push(record);
        self.trips.len() - 1
    }
}

/// The declination that was added to a shot's azimuth: its own, else its trip's, else 0.
pub fn applied_declination(shot: &Shot, trip: Option<&Trip>) -> f64 {
    if let Some(d) = shot.declination.filter(|d| !d.is_nan()) {
        return d;
    }
    match trip {
        Some(t) if !t.declination.is_nan() => t.declination,
        _ => 0.0,
    }
}

/// Brings an azimuth into [0, 360).
pub fn normalize_azimuth(deg: f64) -> f64 {
    let a = deg % 360.0;
    if a < 0.0 { a + 360.0 } else { a }
}

/// Grads to degrees.
pub fn grads_to_degrees(g: f64) -> f64 {
    g * 0.9
}

/// A quadrant bearing ("N30E", "S12.5W") as an azimuth, or `None` if the text is not one.
pub fn parse_quadrant(text: &str) -> Option<f64> {
    let mut chars = text.chars();
    let first = chars.next()?;
    let last = text.chars().last()?;
    let north = matches!(first, 'N' | 'n');
    if !north && !matches!(first, 'S' | 's') {
        return None;
    }
    let east = matches!(last, 'E' | 'e');
    if !east && !matches!(last, 'W' | 'w') {
        return None;
    }
    if text.chars().count() < 3 {
        return None;
    }
    let inner = &text[first.len_utf8()..text.len() - last.len_utf8()];
    let digits = crate::js::trim(inner);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    // the white space allowed is only around the number, never inside it
    let deg = crate::js::parse_float(digits);
    if deg.is_nan() || deg > 90.0 {
        return None;
    }
    let az = match (north, east) {
        (true, true) => deg,
        (true, false) => 360.0 - deg,
        (false, true) => 180.0 - deg,
        (false, false) => 180.0 + deg,
    };
    Some(normalize_azimuth(az))
}

/// Feet per metre.
pub const FEET_PER_METER: f64 = 3.280839895;

/// Converts a distance between "ft" and "m"; anything else passes through unchanged.
pub fn convert(v: f64, from: &str, to: &str) -> f64 {
    match (from, to) {
        ("ft", "m") => v / FEET_PER_METER,
        ("m", "ft") => v * FEET_PER_METER,
        _ => v,
    }
}

/// The shots' bearings may be left out only on a shot at least this steep.
pub const PLUMB_DEG: f64 = 85.0;

/// One LRUD cell as written in notes: "3.5", "0" (wall at the station), "P" (open passage), "5/P"
/// (a ledge at 5, open beyond), "5/10" (several readings; the largest is the value), "" or "--"
/// (not measured).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LrudEntry {
    /// The primary reading.
    pub value: Option<f64>,
    /// Every reading, when there is more than one.
    pub all: Option<Vec<f64>>,
    /// A "P" was there.
    pub open: bool,
}

/// Reads one LRUD cell.
pub fn parse_lrud_entry(text: &str) -> LrudEntry {
    let raw = crate::js::trim(text);
    if raw.is_empty() || raw == "--" {
        return LrudEntry::default();
    }
    let mut values = Vec::new();
    let mut open = false;
    for part in raw.split('/') {
        let p = crate::js::trim(part);
        if p.is_empty() {
            continue;
        }
        if p == "P" || p == "p" {
            open = true;
            continue;
        }
        let n = crate::js::parse_float(p);
        if !n.is_nan() {
            values.push(n);
        }
    }
    if values.is_empty() {
        return LrudEntry {
            value: None,
            all: None,
            open,
        };
    }
    let mut primary = values[0];
    for &v in &values[1..] {
        if v > primary {
            primary = v;
        }
    }
    LrudEntry {
        value: Some(primary),
        all: if values.len() > 1 { Some(values) } else { None },
        open,
    }
}

/// The cell text for a stored LRUD side (the reverse of [`parse_lrud_entry`]).
pub fn lrud_entry_text(value: Option<f64>, all: Option<&[f64]>, open: bool) -> String {
    use crate::js::num_str;
    let num = match all {
        Some(a) if a.len() > 1 => a.iter().map(|v| num_str(*v)).collect::<Vec<_>>().join("/"),
        _ => value.map(num_str).unwrap_or_default(),
    };
    if !open {
        num
    } else if num.is_empty() {
        "P".into()
    } else {
        format!("{num}/P")
    }
}

/// The line the writers put where a fixed station's elevation is unknown.
pub(crate) fn unknown_elevation_note(name: &str) -> String {
    format!(
        "elevation UNKNOWN for {name} -- written as 0.00 to keep this file valid; it is NOT a surveyed datum"
    )
}

impl Survey {
    /// Rescales the whole survey into another distance unit ("ft" or "m"): distances, every LRUD
    /// reading, the start dimensions and the fixed points (angles do not change). A port of the old
    /// `CsUnits.convertSurvey`, which existed because a copy that forgot the fixed points or the
    /// LRUD drew a cave that looked right until somebody measured it.
    pub fn convert_units(&mut self, to: &str) {
        let from = self.unit().to_string();
        if from == to || !matches!(to, "ft" | "m") {
            return;
        }
        let k = convert(1.0, &from, to);
        let scale = |v: &mut Option<f64>| *v = v.map(|x| x * k);
        let scale_all = |v: &mut Option<Vec<f64>>| {
            if let Some(a) = v {
                a.iter_mut().for_each(|x| *x *= k);
            }
        };
        let scale_lrud = |l: &mut Lrud| {
            for v in [&mut l.left, &mut l.right, &mut l.up, &mut l.down] {
                scale(v);
            }
            for v in [
                &mut l.left_all,
                &mut l.right_all,
                &mut l.up_all,
                &mut l.down_all,
            ] {
                scale_all(v);
            }
        };
        for s in &mut self.shots {
            s.distance *= k;
            for v in [&mut s.left, &mut s.right, &mut s.up, &mut s.down] {
                scale(v);
            }
            for v in [
                &mut s.left_all,
                &mut s.right_all,
                &mut s.up_all,
                &mut s.down_all,
            ] {
                scale_all(v);
            }
        }
        let names: Vec<String> = self.fixed.iter().map(|(n, _)| n.to_string()).collect();
        for n in names {
            if let Some(f) = self.fixed.get_mut(&n) {
                for v in [&mut f.x, &mut f.y, &mut f.z] {
                    scale(v);
                }
            }
        }
        if let Some(l) = &mut self.start_lrud {
            scale_lrud(l);
        }
        for t in &mut self.trips {
            if let Some(l) = &mut t.start_lrud {
                scale_lrud(l);
            }
            t.distance_unit = Some(to.into());
        }
        self.distance_unit = Some(to.into());
    }

    /// Adds another survey's trips, shots and fixed points to this one, in this survey's unit.
    /// Stations with one name are one station (a project's files join that way); a fixed point
    /// already here keeps its place.
    pub fn absorb(&mut self, mut other: Survey) {
        self.ensure_trips();
        other.ensure_trips();
        other.convert_units(self.unit());
        let offset = self.trips.len();
        for s in &mut other.shots {
            s.trip += offset;
        }
        if self.cave_name.is_empty() {
            self.cave_name = std::mem::take(&mut other.cave_name);
        }
        self.trips.extend(other.trips);
        self.shots.extend(other.shots);
        for (n, f) in other.fixed.iter() {
            if !self.fixed.has(n) {
                self.fixed.set(n, f.clone());
            }
        }
        for f in other.parse_findings.into_iter().flatten() {
            self.add_finding(&f.severity, &f.code, f.message);
        }
    }
}

/// Where a survey is on the Earth, as a project file declares it: enough to work out the magnetic
/// declination on a date (IGRF) and the grid convergence of its UTM zone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoRef {
    /// Degrees, north positive.
    pub lat: f64,
    /// Degrees, east positive.
    pub lon: f64,
    /// UTM zone, negative in the southern hemisphere.
    pub zone: i32,
}

impl GeoRef {
    /// The magnetic declination on a "YYYY-MM-DD" date, degrees east positive.
    pub fn declination_on(&self, date: &str) -> Option<f64> {
        let year = cave_geomag::decimal_year_of(date)?;
        cave_geomag::declination(self.lat, self.lon, year)
    }

    /// Grid north's angle east of true north here. Fixed points given in UTM are on the grid, so a
    /// survey tied to them needs grid bearings: true bearing minus this.
    pub fn convergence(&self) -> f64 {
        cave_geomag::utm_convergence(self.lat, self.lon, self.zone)
    }
}
