use serde::{Deserialize, Serialize};

/// A mark the surveyor sets on a shot at the instrument.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    /// No mark.
    #[default]
    None,
    /// A feature.
    Feature,
    /// A ridge.
    Ridge,
    /// This shot is a backsight.
    Backsight,
    /// A generic mark.
    Generic,
}

/// A wall of a station: which way a passage dimension is measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// Left of the direction of travel.
    Left,
    /// Right of the direction of travel.
    Right,
    /// The ceiling.
    Up,
    /// The floor.
    Down,
}

/// One shot ready for the survey: metres and degrees, as the instrument measured them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    /// Tape, metres.
    pub distance: f64,
    /// Degrees, as the instrument reports it (magnetic: the trip's declination is applied by the notebook).
    pub azimuth: f64,
    /// Degrees, up positive.
    pub inclination: f64,
    /// Instrument roll, degrees, when it reports one.
    #[serde(default)]
    pub roll: Option<f64>,
    /// Which link sent it, for the log.
    #[serde(default)]
    pub source: String,
    /// The instrument says this shot is part of a leg (three or more close shots).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leg: Option<bool>,
    /// The mark set on the shot with the instrument's own button.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<Mark>,
    /// The instrument's accuracy complaint, if it made one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Laser signal quality, raw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<u32>,
    /// This shot measures that wall at the station: a passage dimension, never a splay. Set where the
    /// instrument cannot say (the simulator; the surveyor's LRUD mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lrud: Option<Side>,
}

impl Reading {
    /// A plain reading: tape, compass and clino, from `source`.
    pub fn new(distance: f64, azimuth: f64, inclination: f64, source: &str) -> Self {
        Reading {
            distance,
            azimuth,
            inclination,
            source: source.into(),
            ..Reading::default()
        }
    }
}
