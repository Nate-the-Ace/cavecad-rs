//! Reads a document's shots into legs for [`crate::resolve`].
//!
//! Splays and shots that cannot be measured are not legs; they are reported in
//! [`Extracted::skipped`] so a Check tool can say why a station is missing instead of guessing.
//! Distances are converted to metres using each shot's trip (empty unit means metres).

use crate::network::{Fixed, Leg};
use crate::traverse::{Reading, TapeMode, offset};
use crate::vec3::Vec3;
use cave_doc::{Document, Field, Kind};
use cave_id::Id;

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

/// Everything the network needs from a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Extracted {
    /// Usable legs in survey order.
    pub legs: Vec<Leg>,
    /// Stations with surveyed control.
    pub fixed: Vec<Fixed>,
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

    let (mut legs, mut skipped) = (Vec::new(), Vec::new());
    for (_, id) in shots {
        let Some(e) = doc.entity(id) else { continue };
        let skip = |reason| Skipped { shot: id, reason };
        if e.flag(Field::ShotExcludeFromAll) {
            skipped.push(skip(SkipReason::Excluded));
            continue;
        }
        if e.flag(Field::ShotSplay) {
            skipped.push(skip(SkipReason::Splay));
            continue;
        }
        let station = |f| {
            e.reference(f)
                .filter(|s| doc.entity(*s).is_some_and(|x| x.kind() == Kind::Station))
        };
        let (Some(from), Some(to)) = (station(Field::ShotFrom), station(Field::ShotTo)) else {
            skipped.push(skip(SkipReason::MissingStation));
            continue;
        };
        if from == to {
            skipped.push(skip(SkipReason::SelfLoop));
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
        match offset(&reading, mode) {
            Some(o) => legs.push(Leg {
                shot: id,
                from,
                to,
                offset: o.v,
                distance: reading.distance,
                no_adjust: e.flag(Field::ShotNoAdjust),
                azimuth_omitted: reading.azimuth_omitted,
            }),
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
    Extracted {
        legs,
        fixed,
        skipped,
    }
}
