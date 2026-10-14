use crate::Pt;
use crate::views::Mark;
use serde::{Deserialize, Serialize};

/// A stroke as stored: its fit points relative to the nearest station of the view it was drawn in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anchored {
    /// The station it hangs on.
    pub anchor: String,
    /// Offsets from the station, in the view's metres.
    pub pts: Vec<Pt>,
    /// Pen weight per point (relative, 1 is normal), when there was a pen.
    pub widths: Option<Vec<f64>>,
    /// What the line is.
    pub kind: String,
}

/// Anchors a stroke drawn at world points `pts` (view metres) to the nearest of `stations`. `None` when there are none.
pub fn anchor_stroke(
    stations: &[Mark],
    pts: &[Pt],
    kind: &str,
    widths: Option<Vec<f64>>,
) -> Option<Anchored> {
    if stations.is_empty() || pts.is_empty() {
        return None;
    }
    let mid = pts[pts.len() / 2];
    let mut best = &stations[0];
    let mut bd = f64::INFINITY;
    for s in stations {
        let d = (s.p[0] - mid[0]).hypot(s.p[1] - mid[1]);
        if d < bd {
            bd = d;
            best = s;
        }
    }
    Some(Anchored {
        anchor: best.label.clone(),
        pts: pts
            .iter()
            .map(|p| [p[0] - best.p[0], p[1] - best.p[1]])
            .collect(),
        widths,
        kind: kind.to_string(),
    })
}

/// Where a stored stroke is now, or `None` when its station is gone.
pub fn place_stroke(stations: &[Mark], a: &Anchored) -> Option<Vec<Pt>> {
    let s = stations.iter().find(|x| x.label == a.anchor)?;
    Some(
        a.pts
            .iter()
            .map(|p| [p[0] + s.p[0], p[1] + s.p[1]])
            .collect(),
    )
}
