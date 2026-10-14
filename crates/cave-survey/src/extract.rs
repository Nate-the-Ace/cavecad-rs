//! Reads a document's shots into legs for [`crate::resolve`].
//!
//! Splays and shots that cannot be measured are not legs; they are reported in
//! [`Extracted::skipped`] so a Check tool can say why a station is missing instead of guessing.
//! Distances are converted to metres using each shot's trip (empty unit means metres).

use crate::network::{Fixed, Leg};
use crate::traverse::{Reading, TapeMode, is_plumb, offset};
use crate::vec3::Vec3;
use cave_doc::{Document, Field, Kind};
use cave_id::Id;
use std::collections::HashMap;

const FEET_TO_METRES: f64 = 0.3048;

/// Why a shot was not used as a leg.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// A splay: geometry for walls, not a station-to-station leg.
    Splay,
    /// FROM or TO is empty, or names a station that no longer exists.
    MissingStation,
    /// Flagged to be left out of everything (a void shot).
    Excluded,
    /// FROM and TO are the same station: skipped, never scored as a closure of its own length.
    SelfLoop,
    /// A distance, bearing or inclination is missing, or the geometry is impossible.
    Unusable,
}

/// A shot left out of the network, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skipped {
    /// The shot.
    pub shot: Id,
    /// Why.
    pub reason: SkipReason,
}

/// A splay: a measured wall hit aimed from a station, not a station-to-station leg.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Splay {
    /// The shot.
    pub shot: Id,
    /// The station it is aimed from.
    pub from: Id,
    /// What was read (distances in metres).
    pub reading: Reading,
    /// Displacement from the station, in metres.
    pub offset: Vec3,
    /// Left out of the plot (and so out of the walls).
    pub exclude_from_plot: bool,
}

/// Passage dimensions measured at a station (metres). `None` means not measured, which is not
/// the same as 0 (the wall is at the station). An open flag means "P": looked at, no wall.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct StationLrud {
    /// Distance to the left wall.
    pub left: Option<f64>,
    /// Distance to the right wall.
    pub right: Option<f64>,
    /// Distance to the ceiling.
    pub up: Option<f64>,
    /// Distance to the floor.
    pub down: Option<f64>,
    /// Open passage to the left.
    pub left_open: bool,
    /// Open passage to the right.
    pub right_open: bool,
    /// Open above.
    pub up_open: bool,
    /// Open below.
    pub down_open: bool,
    /// The bearing the caver faced when pulling the tapes; `None` for a pitch (no bearing).
    pub azimuth: Option<f64>,
}

impl StationLrud {
    /// True if anything was recorded (a number or an open flag).
    pub fn has_evidence(&self) -> bool {
        self.left.is_some()
            || self.right.is_some()
            || self.up.is_some()
            || self.down.is_some()
            || self.left_open
            || self.right_open
            || self.up_open
            || self.down_open
    }
}

/// Everything the network needs from a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Extracted {
    /// Usable legs in survey order.
    pub legs: Vec<Leg>,
    /// Stations with surveyed control.
    pub fixed: Vec<Fixed>,
    /// Splays with a usable reading, in survey order.
    pub splays: Vec<Splay>,
    /// What was read on each leg (by shot), for bearings and pitch tests.
    pub readings: HashMap<Id, Reading>,
    /// Walls measured at each station, from the first shot arriving there that measured any.
    pub lruds: HashMap<Id, StationLrud>,
    /// Walls typed on a trip's opening row, for stations no shot arrives at.
    pub start_lruds: HashMap<Id, StationLrud>,
    /// The old app's choice: the LAST shot arriving at each station that measured walls. Kept only so
    /// the parity tests against the old app still run; the plan walls use `lruds`.
    pub last_arrival_lruds: HashMap<Id, StationLrud>,
    /// The walls on the FIRST shot arriving at each station, even if that shot recorded none (the
    /// 3D passage uses this reading).
    pub first_arrival_lruds: HashMap<Id, StationLrud>,
    /// Which trip (by order, 0 first) each station belongs to: the trip of the first shot arriving
    /// there, else the first leaving it.
    pub station_trip: HashMap<Id, usize>,
    /// Shots not used, with reasons.
    pub skipped: Vec<Skipped>,
}

/// Collects legs, fixed stations and skipped shots from a document.
pub fn extract(doc: &Document) -> Extracted {
    let mut shots: Vec<(f64, Id)> = doc
        .ids_of(Kind::Shot)
        .into_iter()
        .map(|id| {
            (
                doc.entity(id)
                    .and_then(|e| e.number(Field::ShotOrder))
                    .unwrap_or(f64::MAX),
                id,
            )
        })
        .collect();
    shots.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));

    let (mut legs, mut skipped, mut splays) = (Vec::new(), Vec::new(), Vec::new());
    let mut readings: HashMap<Id, Reading> = HashMap::new();
    let mut lruds: HashMap<Id, StationLrud> = HashMap::new();
    let mut first_arrival_lruds: HashMap<Id, StationLrud> = HashMap::new();
    let mut last_arrival_lruds: HashMap<Id, StationLrud> = HashMap::new();
    // (trip, from, to) of every non-splay shot in survey order, excluded ones too.
    let mut trip_marks: Vec<(Option<Id>, Option<Id>, Option<Id>)> = Vec::new();
    let mut arrived: std::collections::HashSet<Id> = std::collections::HashSet::new();
    let mut first_of_trip: HashMap<Option<Id>, Id> = HashMap::new();
    let (mut first_station, mut seen_first) = (None, false);
    for (_, id) in shots {
        let Some(e) = doc.entity(id) else { continue };
        let skip = |reason| Skipped { shot: id, reason };
        let station = |f| {
            e.reference(f)
                .filter(|s| doc.entity(*s).is_some_and(|x| x.kind() == Kind::Station))
        };
        if !e.flag(Field::ShotSplay) {
            trip_marks.push((
                e.reference(Field::ShotTrip),
                station(Field::ShotFrom),
                station(Field::ShotTo),
            ));
        }
        if e.flag(Field::ShotExcludeFromAll) {
            skipped.push(skip(SkipReason::Excluded));
            continue;
        }
        let trip = e.reference(Field::ShotTrip).and_then(|t| doc.entity(t));
        let factor = match trip.map(|t| t.text(Field::TripDistanceUnit)).as_deref() {
            Some("ft") => FEET_TO_METRES,
            _ => 1.0,
        };
        let mode = match trip.map(|t| t.text(Field::TripTapeMode)).as_deref() {
            Some("horizontal") => TapeMode::Horizontal,
            _ => TapeMode::Slope,
        };
        let reading = Reading {
            distance: e
                .number(Field::ShotDistance)
                .map_or(f64::NAN, |d| d * factor),
            azimuth: e.number(Field::ShotAzimuth),
            inclination: e.number(Field::ShotInclination),
            back_azimuth: e.number(Field::ShotBackAzimuth),
            back_inclination: e.number(Field::ShotBackInclination),
            azimuth_omitted: e.flag(Field::ShotAzimuthOmitted),
        };
        if e.flag(Field::ShotSplay) {
            skipped.push(skip(SkipReason::Splay));
            if let (Some(from), Some(o)) = (station(Field::ShotFrom), offset(&reading, mode)) {
                splays.push(Splay {
                    shot: id,
                    from,
                    reading,
                    offset: o.v,
                    exclude_from_plot: e.flag(Field::ShotExcludeFromPlot),
                });
            }
            continue;
        }
        // Walls: LRUD belongs to the TO station of the first non-splay shot arriving there that measured walls.
        let (from_station, to_station) = (station(Field::ShotFrom), station(Field::ShotTo));
        if !seen_first {
            seen_first = true;
            first_station = from_station;
        }
        if let Some(f) = from_station {
            first_of_trip
                .entry(e.reference(Field::ShotTrip))
                .or_insert(f);
        }
        if let Some(t) = to_station {
            arrived.insert(t);
            first_arrival_lruds.entry(t).or_insert_with(|| StationLrud {
                left: e.number(Field::ShotLeft).map(|v| v * factor),
                right: e.number(Field::ShotRight).map(|v| v * factor),
                up: e.number(Field::ShotUp).map(|v| v * factor),
                down: e.number(Field::ShotDown).map(|v| v * factor),
                left_open: e.flag(Field::ShotLeftOpen),
                right_open: e.flag(Field::ShotRightOpen),
                up_open: e.flag(Field::ShotUpOpen),
                down_open: e.flag(Field::ShotDownOpen),
                azimuth: None,
            });
            let lrud = StationLrud {
                left: e.number(Field::ShotLeft).map(|v| v * factor),
                right: e.number(Field::ShotRight).map(|v| v * factor),
                up: e.number(Field::ShotUp).map(|v| v * factor),
                down: e.number(Field::ShotDown).map(|v| v * factor),
                left_open: e.flag(Field::ShotLeftOpen),
                right_open: e.flag(Field::ShotRightOpen),
                up_open: e.flag(Field::ShotUpOpen),
                down_open: e.flag(Field::ShotDownOpen),
                azimuth: if reading.azimuth_omitted || is_plumb(&reading) {
                    None
                } else {
                    reading.azimuth
                },
            };
            // The first arrival that measured walls keeps the station: it is the shot the station
            // panel and the section edit, and a later arrival from the other side (a loop closing
            // on a junction) faces the other way, which would swap the walls left for right.
            if lrud.has_evidence() {
                last_arrival_lruds.insert(t, lrud);
                lruds.entry(t).or_insert(lrud);
            }
        }
        let (Some(from), Some(to)) = (from_station, to_station) else {
            skipped.push(skip(SkipReason::MissingStation));
            continue;
        };
        if from == to {
            skipped.push(skip(SkipReason::SelfLoop));
            continue;
        }
        match offset(&reading, mode) {
            Some(o) => {
                readings.insert(id, reading);
                legs.push(Leg {
                    shot: id,
                    from,
                    to,
                    offset: o.v,
                    distance: reading.distance,
                    no_adjust: e.flag(Field::ShotNoAdjust),
                    azimuth_omitted: reading.azimuth_omitted,
                });
            }
            None => skipped.push(skip(SkipReason::Unusable)),
        }
    }

    let fixed = doc
        .ids_of(Kind::Station)
        .into_iter()
        .filter_map(|id| {
            let e = doc.entity(id)?;
            let pos = Vec3::new(
                e.number(Field::StationFixedX)?,
                e.number(Field::StationFixedY)?,
                e.number(Field::StationFixedZ)?,
            );
            Some(Fixed { station: id, pos })
        })
        .collect();
    // Walls typed on a trip's opening row live on the station that trip begins at, but only if
    // no shot arrives there (otherwise that shot carries them, measured facing the passage).
    let mut trips: Vec<(f64, Id)> = doc
        .ids_of(Kind::Trip)
        .into_iter()
        .map(|id| {
            (
                doc.entity(id)
                    .and_then(|e| e.number(Field::TripOrder))
                    .unwrap_or(f64::MAX),
                id,
            )
        })
        .collect();
    trips.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let rank: HashMap<Id, usize> = trips
        .iter()
        .enumerate()
        .map(|(i, (_, id))| (*id, i))
        .collect();
    let trip_index = |t: Option<Id>| t.and_then(|t| rank.get(&t).copied()).unwrap_or(0);
    let mut station_trip: HashMap<Id, usize> = HashMap::new();
    for (trip, _, to) in &trip_marks {
        if let Some(t) = to {
            station_trip.entry(*t).or_insert_with(|| trip_index(*trip));
        }
    }
    for (trip, from, _) in &trip_marks {
        if let Some(f) = from {
            station_trip.entry(*f).or_insert_with(|| trip_index(*trip));
        }
    }
    let start_of = |trip: Id| -> Option<StationLrud> {
        let e = doc.entity(trip)?;
        let f = if e.text(Field::TripDistanceUnit) == "ft" {
            FEET_TO_METRES
        } else {
            1.0
        };
        let l = StationLrud {
            left: e.number(Field::TripStartLeft).map(|v| v * f),
            right: e.number(Field::TripStartRight).map(|v| v * f),
            up: e.number(Field::TripStartUp).map(|v| v * f),
            down: e.number(Field::TripStartDown).map(|v| v * f),
            left_open: e.flag(Field::TripStartLeftOpen),
            right_open: e.flag(Field::TripStartRightOpen),
            up_open: e.flag(Field::TripStartUpOpen),
            down_open: e.flag(Field::TripStartDownOpen),
            azimuth: None,
        };
        l.has_evidence().then_some(l)
    };
    let mut start_lruds: HashMap<Id, StationLrud> = HashMap::new();
    let mut take = |name: Option<Id>, lrud: Option<StationLrud>| {
        if let (Some(n), Some(l)) = (name, lrud)
            && !arrived.contains(&n)
            && !start_lruds.contains_key(&n)
        {
            start_lruds.insert(n, l);
        }
    };
    take(first_station, trips.first().and_then(|(_, t)| start_of(*t)));
    for (_, t) in &trips {
        take(first_of_trip.get(&Some(*t)).copied(), start_of(*t));
    }

    Extracted {
        legs,
        fixed,
        splays,
        readings,
        lruds,
        start_lruds,
        first_arrival_lruds,
        last_arrival_lruds,
        station_trip,
        skipped,
    }
}
