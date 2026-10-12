//! The ceiling and floor of a band, built from the U and D measurements and from splays.
//!
//! Ported from the old `CsProfile.bandWallRuns` (read its comments: they record each decision).
//! A band is an extended elevation, so a splay is placed at its station's distance along the run
//! plus its own distance along the passage, at the elevation it reaches. Splays that point up or
//! down belong to the ceiling or the floor; a near-level splay (within [`FLAT_SPLAY_DEG`]) says
//! nothing about either, so it becomes a separate tick instead of bending a line it does not
//! describe. A splay with no inclination contributes nothing at all.

use crate::band::Band;
use cave_id::Id;
use cave_survey::{
    LegKind, LegResult, PLUMB_DEG, Reading, Splay, StationLrud, effective_azimuth,
    effective_inclination, is_plumb,
};
use std::collections::{HashMap, HashSet};

/// Half-width of the near-horizontal dead zone, in degrees.
pub const FLAT_SPLAY_DEG: f64 = 10.0;

/// Which line a splay belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplaySide {
    /// Aimed up: part of the ceiling.
    Ceiling,
    /// Aimed down: part of the floor.
    Floor,
    /// Near level: a tick of its own.
    Flat,
}

/// Which line a splay belongs to: the boundary itself (exactly the dead zone) reads as flat, and a
/// splay with no inclination on record reads as flat too (the caller never plots that one).
pub fn classify_splay(reading: &Reading, dead_deg: f64) -> SplaySide {
    match effective_inclination(reading) {
        None => SplaySide::Flat,
        Some(i) if i.abs() <= dead_deg => SplaySide::Flat,
        Some(i) if i > 0.0 => SplaySide::Ceiling,
        Some(_) => SplaySide::Floor,
    }
}

/// Everything the walls of a band are read from.
pub struct WallInput<'a> {
    /// Each station's splays in survey order (those left out of the plot already removed).
    pub splays_at: &'a HashMap<Id, Vec<Splay>>,
    /// What was read on each leg, by shot.
    pub readings: &'a HashMap<Id, Reading>,
    /// Walls measured at each station by the last shot arriving there.
    pub lruds: &'a HashMap<Id, StationLrud>,
    /// Walls typed on a trip's opening row.
    pub start_lruds: &'a HashMap<Id, StationLrud>,
    /// Every resolved leg of the survey (for pitches that lead on above or below a station).
    pub legs: &'a [LegResult],
    /// True where three or more ways lead out of a station (a junction breaks the lines).
    pub is_junction: &'a dyn Fn(Id) -> bool,
    /// The dead zone, degrees (usually [`FLAT_SPLAY_DEG`]).
    pub flat_deg: f64,
}

/// A near-level splay, drawn as a tick on the band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlatTick {
    /// Where it lands: `(s, z)`.
    pub at: [f64; 2],
    /// The station it was aimed from.
    pub station: Id,
}

/// The ceiling and floor lines of one band, and the flat ticks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BandWalls {
    /// Ceiling lines, each a run of `(s, z)` points.
    pub ceiling: Vec<Vec<[f64; 2]>>,
    /// Floor lines.
    pub floor: Vec<Vec<[f64; 2]>>,
    /// Splays that are near level.
    pub flat: Vec<FlatTick>,
}

struct Entry {
    p: [f64; 2],
    t: f64,
    order: i64,
}

fn by_along(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    a.t.total_cmp(&b.t).then(a.order.cmp(&b.order))
}

/// Builds the ceiling and floor of `band`. Lines break at a junction, at a closure, and at a
/// station with no vertical evidence at all, and are never joined across the break.
pub fn band_wall_runs(band: &Band, input: &WallInput) -> BandWalls {
    let plumb_deg = |shot: &Id| input.readings.get(shot).is_some_and(is_plumb);
    let mut plumb_arrival: HashSet<Id> = HashSet::new();
    for l in &band.legs {
        if plumb_deg(&l.shot) {
            plumb_arrival.insert(l.to);
        }
    }

    // Pitches that lead on up or down from a station, over the whole survey.
    let mut leads_up: HashMap<Id, Vec<Id>> = HashMap::new();
    let mut leads_down: HashMap<Id, Vec<Id>> = HashMap::new();
    for l in input.legs {
        let Some(inc) = input
            .readings
            .get(&l.shot)
            .and_then(effective_inclination)
            .filter(|i| i.is_finite() && i.abs() >= PLUMB_DEG)
        else {
            continue;
        };
        let (here, there) = if inc > 0.0 {
            (&mut leads_up, &mut leads_down)
        } else {
            (&mut leads_down, &mut leads_up)
        };
        here.entry(l.from).or_default().push(l.to);
        there.entry(l.to).or_default().push(l.from);
    }
    let has_lead = |map: &HashMap<Id, Vec<Id>>, at: Id, prev: Option<Id>, next: Option<Id>| {
        map.get(&at)
            .is_some_and(|l| l.iter().any(|o| Some(*o) != prev && Some(*o) != next))
    };

    // The passage's bearing at a station: the leg arriving there within the band (a pitch's
    // bearing is noise and is left out), the band's first outgoing leg for its opening station.
    let mut az_at: HashMap<Id, f64> = HashMap::new();
    let mut closure_at: HashSet<Id> = HashSet::new();
    for l in &band.legs {
        if l.kind == LegKind::Closure {
            closure_at.insert(l.to);
        }
        if plumb_deg(&l.shot) {
            continue;
        }
        let Some(az) = input.readings.get(&l.shot).and_then(effective_azimuth) else {
            continue;
        };
        az_at.insert(l.to, az);
        if !az_at.contains_key(&l.from) && !plumb_arrival.contains(&l.from) {
            az_at.insert(l.from, az);
        }
    }

    let mut out = BandWalls::default();
    let (mut ceiling, mut floor): (Vec<[f64; 2]>, Vec<[f64; 2]>) = (Vec::new(), Vec::new());
    fn flush(line: &mut Vec<[f64; 2]>, into: &mut Vec<Vec<[f64; 2]>>) {
        if line.len() >= 2 {
            into.push(std::mem::take(line));
        } else {
            line.clear();
        }
    }
    for (i, st) in band.stations.iter().enumerate() {
        if closure_at.contains(&st.id) {
            flush(&mut ceiling, &mut out.ceiling);
            flush(&mut floor, &mut out.floor);
            continue;
        }
        let lrud = input
            .lruds
            .get(&st.id)
            .or_else(|| input.start_lruds.get(&st.id));
        let along = az_at.get(&st.id).map(|az| {
            let r = az.to_radians();
            (r.sin(), r.cos())
        });
        let (mut c_entries, mut f_entries): (Vec<Entry>, Vec<Entry>) = (Vec::new(), Vec::new());
        if let Some(l) = lrud {
            if let Some(up) = l.up {
                c_entries.push(Entry {
                    p: [st.s, st.z + up],
                    t: 0.0,
                    order: -1,
                });
            }
            if let Some(down) = l.down {
                f_entries.push(Entry {
                    p: [st.s, st.z - down],
                    t: 0.0,
                    order: -1,
                });
            }
        }
        for (k, sp) in input
            .splays_at
            .get(&st.id)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let t = along.map_or(0.0, |(ax, ay)| sp.offset.x * ax + sp.offset.y * ay);
            let p = [st.s + t, st.z + sp.offset.z];
            let entry = Entry {
                p,
                t,
                order: k as i64,
            };
            match classify_splay(&sp.reading, input.flat_deg) {
                SplaySide::Ceiling => c_entries.push(entry),
                SplaySide::Floor => f_entries.push(entry),
                SplaySide::Flat => out.flat.push(FlatTick {
                    at: p,
                    station: st.id,
                }),
            }
        }
        c_entries.sort_by(by_along);
        f_entries.sort_by(by_along);
        let open_up = lrud.is_some_and(|l| l.up_open);
        let open_down = lrud.is_some_and(|l| l.down_open);
        let no_evidence = c_entries.is_empty() && f_entries.is_empty() && !open_up && !open_down;
        ceiling.extend(c_entries.iter().map(|e| e.p));
        floor.extend(f_entries.iter().map(|e| e.p));
        if (input.is_junction)(st.id) || no_evidence {
            flush(&mut ceiling, &mut out.ceiling);
            flush(&mut floor, &mut out.floor);
            continue;
        }
        let prev = (i > 0).then(|| band.stations[i - 1].id);
        let next = band.stations.get(i + 1).map(|s| s.id);
        if open_up && has_lead(&leads_up, st.id, prev, next) {
            flush(&mut ceiling, &mut out.ceiling);
        }
        if open_down && has_lead(&leads_down, st.id, prev, next) {
            flush(&mut floor, &mut out.floor);
        }
    }
    flush(&mut ceiling, &mut out.ceiling);
    flush(&mut floor, &mut out.floor);
    out
}
