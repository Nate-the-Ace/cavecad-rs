//! What can be stored on a trip, station or shot, and the value types each field holds.

use cave_id::Id;
use serde::{Deserialize, Serialize};

/// The three kinds of survey entity.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Kind {
    /// A survey trip: one visit with its own date, team and declination.
    Trip,
    /// A named survey point.
    Station,
    /// A measurement between two stations (or a splay).
    Shot,
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
}

impl Value {
    /// Which [`ValueType`] this value is.
    pub fn value_type(&self) -> ValueType {
        match self {
            Value::Text(_) => ValueType::Text,
            Value::Number(_) => ValueType::Number,
            Value::Flag(_) => ValueType::Flag,
            Value::Ref(_) => ValueType::Ref,
        }
    }

    /// False for numbers that are not finite; those are never stored.
    pub fn is_finite(&self) -> bool {
        !matches!(self, Value::Number(Some(n)) if !n.is_finite())
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
    /// Unit of every distance and LRUD in the trip: `ft` or `m`.
    TripDistanceUnit,
    /// Station label shown to people (not its identity).
    StationLabel,
    /// Fixed easting/x, when this station is a fixed point.
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
}

impl Field {
    /// The kind of entity this field belongs to.
    pub fn kind(self) -> Kind {
        use Field::*;
        match self {
            TripName
            | TripDate
            | TripTeam
            | TripDeclination
            | TripDeclinationSource
            | TripDistanceUnit => Kind::Trip,
            StationLabel | StationFixedX | StationFixedY | StationFixedZ => Kind::Station,
            _ => Kind::Shot,
        }
    }

    /// The type of value this field holds.
    pub fn value_type(self) -> ValueType {
        use Field::*;
        match self {
            TripName
            | TripDate
            | TripTeam
            | TripDeclinationSource
            | TripDistanceUnit
            | StationLabel => ValueType::Text,
            TripDeclination | StationFixedX | StationFixedY | StationFixedZ | ShotDistance
            | ShotAzimuth | ShotInclination | ShotLeft | ShotRight | ShotUp | ShotDown => {
                ValueType::Number
            }
            ShotTrip | ShotFrom | ShotTo => ValueType::Ref,
            ShotLeftOpen | ShotRightOpen | ShotUpOpen | ShotDownOpen | ShotSplay
            | ShotExcludeFromPlot => ValueType::Flag,
        }
    }

    /// What an unset field reads as: empty text, not measured, false, no reference.
    pub fn default_value(self) -> Value {
        match self.value_type() {
            ValueType::Text => Value::Text(String::new()),
            ValueType::Number => Value::Number(None),
            ValueType::Flag => Value::Flag(false),
            ValueType::Ref => Value::Ref(None),
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
        assert!(!Value::Number(Some(f64::NAN)).is_finite());
    }
}
