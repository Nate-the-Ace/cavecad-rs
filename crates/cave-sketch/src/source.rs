use cave_notebook::{Page, RowKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One shot as the notebook holds it. Tape in metres, angles in degrees; backsights as read (compass about
/// 180 off, clino flipped).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurveyShot {
    /// The shot's id, as text.
    pub id: String,
    /// Where it starts.
    pub from: String,
    /// Where it ends; `None` for a splay.
    pub to: Option<String>,
    /// Tape, metres.
    pub distance: Option<f64>,
    /// Foresight compass.
    pub az: Option<f64>,
    /// Foresight clino.
    pub inc: Option<f64>,
    /// Backsight compass.
    #[serde(default, rename = "azBack")]
    pub az_back: Option<f64>,
    /// Backsight clino.
    #[serde(default, rename = "incBack")]
    pub inc_back: Option<f64>,
}

/// The passage dimensions at a station, metres; `None` is not measured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Dims {
    /// Left.
    #[serde(default)]
    pub left: Option<f64>,
    /// Right.
    #[serde(default)]
    pub right: Option<f64>,
    /// Up.
    #[serde(default)]
    pub up: Option<f64>,
    /// Down.
    #[serde(default)]
    pub down: Option<f64>,
}

/// Dimensions by station label.
pub type DimsByStation = BTreeMap<String, Dims>;

/// What the views are made from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// The shots, in survey order.
    pub shots: Vec<SurveyShot>,
    /// The passage dimensions.
    pub dims: DimsByStation,
}

/// Reads a notebook page as a [`Source`]. The page shows what the compass read (magnetic); the trip's
/// declination turns it into the true bearing the views are drawn on.
pub fn source(page: &Page) -> Source {
    let decl = page.declination.unwrap_or(0.0);
    let turn = |v: Option<f64>| v.map(|a| (a + decl).rem_euclid(360.0));
    let mut out = Source::default();
    let mut at = String::new();
    for (i, row) in page.rows.iter().enumerate() {
        match row.kind {
            RowKind::Station => {
                at = row.label.clone();
                if let Some(l) = &row.lrud {
                    let d = Dims {
                        left: l.left,
                        right: l.right,
                        up: l.up,
                        down: l.down,
                    };
                    let entry = out.dims.entry(row.label.clone()).or_default();
                    // The arrival row and the opening row both speak for a station: keep whatever either measured.
                    *entry = Dims {
                        left: d.left.or(entry.left),
                        right: d.right.or(entry.right),
                        up: d.up.or(entry.up),
                        down: d.down.or(entry.down),
                    };
                }
            }
            RowKind::Shot | RowKind::Splay => {
                let to = (row.kind == RowKind::Shot).then(|| {
                    page.rows[i + 1..]
                        .iter()
                        .find(|r| r.kind == RowKind::Station)
                        .map(|r| r.label.clone())
                        .unwrap_or_default()
                });
                out.shots.push(SurveyShot {
                    id: row.shot.map(|s| s.to_string()).unwrap_or_default(),
                    from: at.clone(),
                    to,
                    distance: row.distance,
                    az: turn(row.az_fs),
                    inc: row.inc_fs,
                    az_back: turn(row.az_bs),
                    inc_back: row.inc_bs,
                });
            }
            RowKind::Gap => {}
        }
    }
    out
}
