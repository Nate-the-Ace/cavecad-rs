//! What can be stored on a trip, station or shot, and the value types each field holds.

use crate::anchored::AnchoredPath;
use cave_geom::{Path, Point};
use cave_id::Id;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The kinds of stored entity.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Kind {
    /// A survey trip: one visit with its own date, team and declination.
    Trip,
    /// A named survey point.
    Station,
    /// A measurement between two stations (or a splay).
    Shot,
    /// A traced line (wall, ledge, pit edge, ...).
    Feature,
    /// A placed symbol (stalactite, breakdown, ...).
    Symbol,
    /// A piece of text on the map.
    Text,
}

/// The type of value a field holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueType {
    /// Free text.
    Text,
    /// A measurement; `None` means not measured.
    Number,
    /// Yes or no.
    Flag,
    /// A reference to another entity; `None` means no reference.
    Ref,
    /// A position; `None` means not placed.
    Point,
    /// A traced line; `None` means not drawn.
    Path,
    /// Free-form text tags, name to value.
    Map,
    /// A stroke anchored to stations; `None` means not drawn.
    Anchored,
}

/// A stored value.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum Value {
    /// Free text.
    Text(String),
    /// A measurement, or `None` for not measured (distinct from `Some(0.0)`).
    Number(Option<f64>),
    /// Yes or no.
    Flag(bool),
    /// A reference to another entity by id.
    Ref(Option<Id>),
    /// A position, or `None` for not placed.
    Point(Option<Point>),
    /// A traced line, or `None` for not drawn. Stored whole; edits replace it.
    Path(Option<Path>),
    /// Free-form tags. Stored whole; edits replace it.
    Map(BTreeMap<String, String>),
    /// A stroke anchored to stations, or `None` for not drawn. Stored whole.
    Anchored(Option<AnchoredPath>),
}

impl Value {
    /// Which [`ValueType`] this value is.
    pub fn value_type(&self) -> ValueType {
        match self {
            Value::Text(_) => ValueType::Text,
            Value::Number(_) => ValueType::Number,
            Value::Flag(_) => ValueType::Flag,
            Value::Ref(_) => ValueType::Ref,
            Value::Point(_) => ValueType::Point,
            Value::Path(_) => ValueType::Path,
            Value::Map(_) => ValueType::Map,
            Value::Anchored(_) => ValueType::Anchored,
        }
    }

    /// False for values that must never be stored: non-finite numbers or points, and paths
    /// that fail [`Path::validate`].
    pub fn is_valid(&self) -> bool {
        match self {
            Value::Number(Some(n)) => n.is_finite(),
            Value::Point(Some(p)) => p.is_finite(),
            Value::Path(Some(p)) => p.validate().is_ok(),
            Value::Anchored(Some(a)) => a.is_valid(),
            _ => true,
        }
    }
}

/// Every field of every kind. A field belongs to exactly one [`Kind`] and holds one value type.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Field {
    /// Trip name.
    TripName,
    /// Trip date as `YYYY-MM-DD`, or empty.
    TripDate,
    /// Trip team, free text.
    TripTeam,
    /// Magnetic declination in degrees, positive east, already applied to azimuths.
    TripDeclination,
    /// Where the declination came from: `file`, `user`, `igrf`, or empty.
    TripDeclinationSource,
    /// Unit of every distance and LRUD in the trip: `ft` or `m` (empty means metres).
    TripDistanceUnit,
    /// The cave's name (the file's first line), kept on the first trip.
    TripCaveName,
    /// Instruments used on the trip, free text.
    TripInstruments,
    /// A note about the trip's first station.
    TripStartNote,
    /// Position of the trip among trips (smaller is earlier; ties break by id).
    TripOrder,
    /// Walls at the trip's first station (nothing arrives there, so no shot carries them).
    TripStartLeft,
    /// See `TripStartLeft`.
    TripStartRight,
    /// See `TripStartLeft`.
    TripStartUp,
    /// See `TripStartLeft`.
    TripStartDown,
    /// The start station's left side was written "P" (open passage).
    TripStartLeftOpen,
    /// See `TripStartLeftOpen`.
    TripStartRightOpen,
    /// See `TripStartLeftOpen`.
    TripStartUpOpen,
    /// See `TripStartLeftOpen`.
    TripStartDownOpen,
    /// How the tape was held: `slope` (default, empty means slope) or `horizontal`.
    TripTapeMode,
    /// Station label shown to people (not its identity).
    StationLabel,
    /// The team's mark on a station: `open`, `assigned`, `pushed`, `closed`, ... (empty: none).
    StationStatus,
    /// The team's own note on a station (separate from the note written in the survey).
    StationTeamNote,
    /// Who has the lead.
    StationAssigned,
    /// The station is an agreed benchmark (drawn as a fixed survey point), not just a survey point.
    StationBenchmark,
    /// Fixed easting/x in metres, when this station is a fixed point.
    StationFixedX,
    /// Fixed northing/y.
    StationFixedY,
    /// Fixed elevation/z.
    StationFixedZ,
    /// The trip a shot belongs to.
    ShotTrip,
    /// Station the shot starts at.
    ShotFrom,
    /// Station the shot ends at; none for a splay's missing end.
    ShotTo,
    /// Tape distance.
    ShotDistance,
    /// True bearing in degrees clockwise from north.
    ShotAzimuth,
    /// Inclination in degrees, positive up.
    ShotInclination,
    /// Backsight bearing, if one was read.
    ShotBackAzimuth,
    /// Backsight inclination, if one was read (reads sign-flipped).
    ShotBackInclination,
    /// The notes record no bearing for this shot (a plumbed pitch).
    ShotAzimuthOmitted,
    /// Hold this leg rigid when adjusting loops.
    ShotNoAdjust,
    /// Leave this shot out of everything (a void shot, a station never found again).
    ShotExcludeFromAll,
    /// The shot's note.
    ShotNote,
    /// The declination this shot's azimuth was computed with (degrees); absent means "its trip's".
    ShotDeclination,
    /// Leave this shot out of length totals.
    ShotExcludeFromLength,
    /// Position of the shot in survey order (smaller is earlier; ties break by id).
    ShotOrder,
    /// Distance to the left wall at the TO station.
    ShotLeft,
    /// Distance to the right wall.
    ShotRight,
    /// Distance to the ceiling.
    ShotUp,
    /// Distance to the floor.
    ShotDown,
    /// Passage is open on the left (no wall), not merely unmeasured.
    ShotLeftOpen,
    /// Passage is open on the right.
    ShotRightOpen,
    /// Passage is open above.
    ShotUpOpen,
    /// Passage is open below.
    ShotDownOpen,
    /// A splay: geometry only, no named TO station.
    ShotSplay,
    /// Position the stations but draw nothing.
    ShotExcludeFromPlot,
    /// The traced line itself.
    FeaturePath,
    /// What the feature is (`wall`, `ledge`, `pit`, ...).
    FeatureType,
    /// How it is drawn (`plain` or a shaped-line style).
    FeatureShape,
    /// The trip whose ground the feature sits on.
    FeatureTrip,
    /// For section traces, the station the section is cut at.
    FeatureStation,
    /// For profile linework, the run (key) whose band the stroke was drawn on.
    FeatureRun,
    /// Which view the stroke was drawn in, and so what its points mean: `profile` (anchored to
    /// the band of its run), `section` (in the plane across the passage at its station), or `map`
    /// (in plan, anchored to the survey). `FeatureType` says what it IS (wall, ledge, pit, ...).
    FeatureView,
    /// Shaped lines: true puts the ornament on the left of the direction of travel instead of the right.
    FeatureFlip,
    /// Words that go with the stroke: a label next to a symbol, or the text of a text stroke.
    FeatureText,
    /// A stroke anchored to stations (follows the survey when it changes).
    FeatureAnchored,
    /// Free-form tags.
    FeatureTags,
    /// Which symbol from the palette.
    SymbolName,
    /// Where it is placed.
    SymbolAt,
    /// Rotation in degrees counter-clockwise.
    SymbolRotation,
    /// Scale factor (1 is natural size).
    SymbolScale,
    /// The trip whose ground the symbol sits on.
    SymbolTrip,
    /// Free-form tags (attribute values).
    SymbolTags,
    /// The trip whose ground the text sits on.
    TextTrip,
    /// The words.
    TextBody,
    /// Where the text starts.
    TextAt,
    /// Text height in drawing units.
    TextHeight,
    /// Rotation in degrees counter-clockwise.
    TextRotation,
}

impl Field {
    /// The kind of entity this field belongs to.
    pub fn kind(self) -> Kind {
        self.meta().0
    }

    /// The type of value this field holds.
    pub fn value_type(self) -> ValueType {
        self.meta().1
    }

    /// The one place that says which kind and value type each field has.
    fn meta(self) -> (Kind, ValueType) {
        use Field::*;
        use Kind as K;
        use ValueType as V;
        match self {
            TripName => (K::Trip, V::Text),
            TripDate => (K::Trip, V::Text),
            TripTeam => (K::Trip, V::Text),
            TripDeclination => (K::Trip, V::Number),
            TripDeclinationSource => (K::Trip, V::Text),
            TripDistanceUnit => (K::Trip, V::Text),
            TripTapeMode => (K::Trip, V::Text),
            TripOrder => (K::Trip, V::Number),
            TripCaveName | TripInstruments | TripStartNote => (K::Trip, V::Text),
            TripStartLeft | TripStartRight | TripStartUp | TripStartDown => (K::Trip, V::Number),
            TripStartLeftOpen | TripStartRightOpen | TripStartUpOpen | TripStartDownOpen => {
                (K::Trip, V::Flag)
            }
            StationLabel => (K::Station, V::Text),
            StationStatus | StationTeamNote | StationAssigned => (K::Station, V::Text),
            StationBenchmark => (K::Station, V::Flag),
            StationFixedX => (K::Station, V::Number),
            StationFixedY => (K::Station, V::Number),
            StationFixedZ => (K::Station, V::Number),
            ShotTrip => (K::Shot, V::Ref),
            ShotFrom => (K::Shot, V::Ref),
            ShotTo => (K::Shot, V::Ref),
            ShotDistance => (K::Shot, V::Number),
            ShotAzimuth => (K::Shot, V::Number),
            ShotInclination => (K::Shot, V::Number),
            ShotBackAzimuth => (K::Shot, V::Number),
            ShotBackInclination => (K::Shot, V::Number),
            ShotAzimuthOmitted => (K::Shot, V::Flag),
            ShotNoAdjust => (K::Shot, V::Flag),
            ShotOrder => (K::Shot, V::Number),
            ShotNote => (K::Shot, V::Text),
            ShotDeclination => (K::Shot, V::Number),
            ShotExcludeFromLength => (K::Shot, V::Flag),
            ShotExcludeFromAll => (K::Shot, V::Flag),
            ShotLeft => (K::Shot, V::Number),
            ShotRight => (K::Shot, V::Number),
            ShotUp => (K::Shot, V::Number),
            ShotDown => (K::Shot, V::Number),
            ShotLeftOpen => (K::Shot, V::Flag),
            ShotRightOpen => (K::Shot, V::Flag),
            ShotUpOpen => (K::Shot, V::Flag),
            ShotDownOpen => (K::Shot, V::Flag),
            ShotSplay => (K::Shot, V::Flag),
            ShotExcludeFromPlot => (K::Shot, V::Flag),
            FeaturePath => (K::Feature, V::Path),
            FeatureType => (K::Feature, V::Text),
            FeatureShape => (K::Feature, V::Text),
            FeatureTrip => (K::Feature, V::Ref),
            FeatureStation => (K::Feature, V::Ref),
            FeatureRun => (K::Feature, V::Text),
            FeatureView => (K::Feature, V::Text),
            FeatureFlip => (K::Feature, V::Flag),
            FeatureText => (K::Feature, V::Text),
            FeatureAnchored => (K::Feature, V::Anchored),
            FeatureTags => (K::Feature, V::Map),
            SymbolName => (K::Symbol, V::Text),
            SymbolAt => (K::Symbol, V::Point),
            SymbolRotation => (K::Symbol, V::Number),
            SymbolScale => (K::Symbol, V::Number),
            SymbolTrip => (K::Symbol, V::Ref),
            SymbolTags => (K::Symbol, V::Map),
            TextTrip => (K::Text, V::Ref),
            TextBody => (K::Text, V::Text),
            TextAt => (K::Text, V::Point),
            TextHeight => (K::Text, V::Number),
            TextRotation => (K::Text, V::Number),
        }
    }

    /// What an unset field reads as: empty text, not measured, false, no reference.
    pub fn default_value(self) -> Value {
        match self.value_type() {
            ValueType::Text => Value::Text(String::new()),
            ValueType::Number => Value::Number(None),
            ValueType::Flag => Value::Flag(false),
            ValueType::Ref => Value::Ref(None),
            ValueType::Point => Value::Point(None),
            ValueType::Path => Value::Path(None),
            ValueType::Map => Value::Map(BTreeMap::new()),
            ValueType::Anchored => Value::Anchored(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_declared_types() {
        use Field::*;
        for f in [
            TripName,
            TripDeclination,
            StationFixedZ,
            ShotFrom,
            ShotSplay,
            ShotLeft,
            ShotLeftOpen,
        ] {
            assert_eq!(f.default_value().value_type(), f.value_type());
        }
    }

    #[test]
    fn kinds_split_as_expected() {
        assert_eq!(Field::TripDate.kind(), Kind::Trip);
        assert_eq!(Field::StationLabel.kind(), Kind::Station);
        assert_eq!(Field::ShotDownOpen.kind(), Kind::Shot);
    }

    #[test]
    fn unmeasured_is_not_zero() {
        assert_ne!(Value::Number(None), Value::Number(Some(0.0)));
        assert!(!Value::Number(Some(f64::NAN)).is_valid());
    }
}
