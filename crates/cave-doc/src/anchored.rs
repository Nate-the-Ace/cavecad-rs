//! Strokes anchored to stations, so a sketch stays attached to the cave when the survey changes.
//!
//! Each point is stored relative to a frame: a pair of neighbouring stations. `u` is how far
//! along the leg from station `a` to station `b` the point is (0 at `a`, 1 at `b`, outside that
//! range beyond the ends); `v` is how far off the leg, in metres, on its left. Re-placing a stroke
//! from the stations' current positions makes it follow the survey: stretch along a leg that gets
//! longer, rotate with a leg that turns, keep its sideways offsets in metres. This is derived
//! placement: the authored truth is the anchors, never the screen position.

use cave_id::Id;
use serde::{Deserialize, Serialize};

/// Two neighbouring stations and where they are in the drawing's own 2D space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// First station.
    pub a: Id,
    /// Second station.
    pub b: Id,
    /// Position of `a`.
    pub pa: [f64; 2],
    /// Position of `b`.
    pub pb: [f64; 2],
}

/// One stroke point relative to its frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnchorPoint {
    /// Index into the path's `frames`.
    pub frame: u32,
    /// Fraction along `a` to `b` (not clamped).
    pub u: f64,
    /// Metres off the leg, positive to its left.
    pub v: f64,
}

/// A stroke stored as anchors.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnchoredPath {
    /// The station pairs the points refer to.
    pub frames: Vec<(Id, Id)>,
    /// The stroke, in drawing order.
    pub points: Vec<AnchorPoint>,
    /// Relative pen weight per point (from pressure), if recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widths: Option<Vec<f64>>,
}

/// Why a stroke could not be anchored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorError {
    /// Fewer than two points.
    TooShort,
    /// A coordinate or width was not a usable number, or widths did not match the points.
    BadStroke,
    /// There is no usable pair of stations to anchor to.
    NoFrames,
}

fn basis(pa: [f64; 2], pb: [f64; 2]) -> Option<([f64; 2], f64)> {
    let d = [pb[0] - pa[0], pb[1] - pa[1]];
    let len = d[0].hypot(d[1]);
    (len > 1e-9 && len.is_finite()).then_some((d, len))
}

fn distance_to_segment(p: [f64; 2], pa: [f64; 2], d: [f64; 2], len: f64) -> f64 {
    let t = (((p[0] - pa[0]) * d[0] + (p[1] - pa[1]) * d[1]) / (len * len)).clamp(0.0, 1.0);
    (p[0] - (pa[0] + t * d[0])).hypot(p[1] - (pa[1] + t * d[1]))
}

/// Anchors a stroke drawn in the drawing's 2D space to the nearest of `frames`.
pub fn anchor_stroke(
    points: &[[f64; 2]],
    frames: &[Frame],
    widths: Option<Vec<f64>>,
) -> Result<AnchoredPath, AnchorError> {
    if points.len() < 2 {
        return Err(AnchorError::TooShort);
    }
    if points
        .iter()
        .any(|p| !p[0].is_finite() || !p[1].is_finite())
    {
        return Err(AnchorError::BadStroke);
    }
    if let Some(w) = &widths
        && (w.len() != points.len() || w.iter().any(|x| !x.is_finite() || *x <= 0.0))
    {
        return Err(AnchorError::BadStroke);
    }
    let usable: Vec<(usize, [f64; 2], f64)> = frames
        .iter()
        .enumerate()
        .filter_map(|(i, f)| basis(f.pa, f.pb).map(|(d, l)| (i, d, l)))
        .collect();
    if usable.is_empty() {
        return Err(AnchorError::NoFrames);
    }
    let mut used: Vec<usize> = Vec::new();
    let mut out = Vec::with_capacity(points.len());
    for p in points {
        let &(i, d, len) =
            usable
                .iter()
                .min_by(|x, y| {
                    distance_to_segment(*p, frames[x.0].pa, x.1, x.2)
                        .total_cmp(&distance_to_segment(*p, frames[y.0].pa, y.1, y.2))
                })
                .expect("usable is not empty");
        let pa = frames[i].pa;
        let (rx, ry) = (p[0] - pa[0], p[1] - pa[1]);
        let slot = used.iter().position(|&x| x == i).unwrap_or_else(|| {
            used.push(i);
            used.len() - 1
        });
        out.push(AnchorPoint {
            frame: slot as u32,
            u: (rx * d[0] + ry * d[1]) / (len * len),
            v: (-rx * d[1] + ry * d[0]) / len,
        });
    }
    Ok(AnchoredPath {
        frames: used.iter().map(|&i| (frames[i].a, frames[i].b)).collect(),
        points: out,
        widths,
    })
}

impl AnchoredPath {
    /// True if the stroke is well formed.
    pub fn is_valid(&self) -> bool {
        self.points.len() >= 2
            && !self.frames.is_empty()
            && self.points.iter().all(|p| {
                (p.frame as usize) < self.frames.len() && p.u.is_finite() && p.v.is_finite()
            })
            && self.widths.as_ref().is_none_or(|w| {
                w.len() == self.points.len() && w.iter().all(|x| x.is_finite() && *x > 0.0)
            })
    }

    /// The stations the stroke hangs on.
    pub fn stations(&self) -> impl Iterator<Item = Id> + '_ {
        self.frames.iter().flat_map(|(a, b)| [*a, *b])
    }

    /// Places the stroke from the stations' current positions. `None` when any station it hangs
    /// on is missing or two of them coincide (a stroke is placed whole or not at all).
    pub fn place(&self, position: &dyn Fn(Id) -> Option<[f64; 2]>) -> Option<Vec<[f64; 2]>> {
        self.place_with_frames(position)
            .map(|v| v.into_iter().map(|(_, p)| p).collect())
    }

    /// [`place`](Self::place), also saying which frame (index into `frames`) each point hangs on.
    pub fn place_with_frames(
        &self,
        position: &dyn Fn(Id) -> Option<[f64; 2]>,
    ) -> Option<Vec<(usize, [f64; 2])>> {
        let mut bases = Vec::with_capacity(self.frames.len());
        for (a, b) in &self.frames {
            let (pa, pb) = (position(*a)?, position(*b)?);
            let (d, len) = basis(pa, pb)?;
            bases.push((pa, d, len));
        }
        Some(
            self.points
                .iter()
                .map(|p| {
                    let (pa, d, len) = bases[p.frame as usize];
                    (
                        p.frame as usize,
                        [
                            pa[0] + p.u * d[0] - p.v * d[1] / len,
                            pa[1] + p.u * d[1] + p.v * d[0] / len,
                        ],
                    )
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_id::{IdSource, SeqIds};
    use std::collections::HashMap;

    fn setup() -> (Vec<Frame>, HashMap<Id, [f64; 2]>) {
        let mut ids = SeqIds::new(1);
        let (a, b, c) = (ids.next_id(), ids.next_id(), ids.next_id());
        let pos: HashMap<Id, [f64; 2]> =
            [(a, [0.0, 0.0]), (b, [10.0, 0.0]), (c, [10.0, -8.0])].into();
        let frames = vec![
            Frame {
                a,
                b,
                pa: pos[&a],
                pb: pos[&b],
            },
            Frame {
                a: b,
                b: c,
                pa: pos[&b],
                pb: pos[&c],
            },
        ];
        (frames, pos)
    }

    fn close(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(p, q)| (p[0] - q[0]).abs() < 1e-9 && (p[1] - q[1]).abs() < 1e-9)
    }

    #[test]
    fn anchoring_then_placing_without_changes_gives_the_stroke_back() {
        let (frames, pos) = setup();
        let stroke = [
            [1.0, 2.0],
            [5.0, 3.0],
            [11.0, -4.0],
            [12.0, -9.0],
            [-2.0, 1.0],
        ];
        let a = anchor_stroke(&stroke, &frames, None).unwrap();
        assert!(a.is_valid());
        assert!(close(
            &a.place(&|id| pos.get(&id).copied()).unwrap(),
            &stroke
        ));
    }

    #[test]
    fn the_stroke_follows_when_stations_move_stretch_and_turn() {
        let (frames, pos) = setup();
        let stroke = [[2.0, 1.0], [8.0, 1.0]];
        let a = anchor_stroke(&stroke, &frames, None).unwrap();
        // Move everything 100 right, 50 up: the stroke moves exactly the same.
        let moved = |id: Id| pos.get(&id).map(|p| [p[0] + 100.0, p[1] + 50.0]);
        assert!(close(
            &a.place(&moved).unwrap(),
            &[[102.0, 51.0], [108.0, 51.0]]
        ));
        // Stretch the leg to double length: along-leg distances double, the 1 m offset stays 1 m.
        let stretched = |id: Id| pos.get(&id).map(|p| [p[0] * 2.0, p[1]]);
        assert!(close(
            &a.place(&stretched).unwrap(),
            &[[4.0, 1.0], [16.0, 1.0]]
        ));
        // Turn the whole thing a quarter turn: the stroke turns with it.
        let turned = |id: Id| pos.get(&id).map(|p| [-p[1], p[0]]);
        assert!(close(
            &a.place(&turned).unwrap(),
            &[[-1.0, 2.0], [-1.0, 8.0]]
        ));
    }

    #[test]
    fn a_vertical_leg_works_like_any_other() {
        let mut ids = SeqIds::new(2);
        let (a, b) = (ids.next_id(), ids.next_id());
        let frames = [Frame {
            a,
            b,
            pa: [0.0, 0.0],
            pb: [0.0, -20.0],
        }];
        let stroke = [[1.0, -5.0], [1.0, -15.0]];
        let path = anchor_stroke(&stroke, &frames, None).unwrap();
        let pos: HashMap<Id, [f64; 2]> = [(a, [0.0, 0.0]), (b, [0.0, -20.0])].into();
        assert!(close(
            &path.place(&|id| pos.get(&id).copied()).unwrap(),
            &stroke
        ));
        let lower: HashMap<Id, [f64; 2]> = [(a, [0.0, 0.0]), (b, [0.0, -30.0])].into();
        let p = path.place(&|id| lower.get(&id).copied()).unwrap();
        assert!(
            (p[0][1] + 7.5).abs() < 1e-9 && (p[1][1] + 22.5).abs() < 1e-9,
            "stretches with the pit"
        );
    }

    #[test]
    fn a_missing_or_coincident_station_means_not_placed() {
        let (frames, pos) = setup();
        let a = anchor_stroke(&[[1.0, 1.0], [2.0, 1.0]], &frames, None).unwrap();
        let (first, _) = a.frames[0];
        assert!(
            a.place(&|id| (id != first).then(|| pos[&id])).is_none(),
            "a deleted station"
        );
        assert!(
            a.place(&|_| Some([0.0, 0.0])).is_none(),
            "coincident stations"
        );
    }

    #[test]
    fn bad_input_is_refused() {
        let (frames, _) = setup();
        assert_eq!(
            anchor_stroke(&[[0.0, 0.0]], &frames, None).unwrap_err(),
            AnchorError::TooShort
        );
        assert_eq!(
            anchor_stroke(&[[0.0, 0.0], [f64::NAN, 1.0]], &frames, None).unwrap_err(),
            AnchorError::BadStroke
        );
        assert_eq!(
            anchor_stroke(&[[0.0, 0.0], [1.0, 1.0]], &[], None).unwrap_err(),
            AnchorError::NoFrames
        );
        assert_eq!(
            anchor_stroke(&[[0.0, 0.0], [1.0, 1.0]], &frames, Some(vec![1.0])).unwrap_err(),
            AnchorError::BadStroke
        );
    }

    #[test]
    fn pen_weights_and_json_survive() {
        let (frames, _) = setup();
        let a = anchor_stroke(&[[1.0, 1.0], [2.0, 1.0]], &frames, Some(vec![0.5, 1.5])).unwrap();
        assert_eq!(a.widths.as_deref(), Some(&[0.5, 1.5][..]));
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(serde_json::from_str::<AnchoredPath>(&json).unwrap(), a);
    }
}
