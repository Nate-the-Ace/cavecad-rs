//! Sweeping a phone's camera through the room with the gimbal: target-mode lists for the phone held portrait,
//! camera out (target yaw pans the camera, pitch tilts it). Pure: no Bluetooth.

use crate::targets::{LIST_MAX, Target};

/// Where the camera can be pointed, degrees relative to home (target-mode yaw and pitch).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    /// Yaw (pan) range, low to high.
    pub yaw: (f64, f64),
    /// Pitch (tilt) range, low to high.
    pub pitch: (f64, f64),
}

/// The OM7 holding the phone portrait, camera out, inside its joint windows with a margin (FINDINGS; bench-tuned).
/// Bench 2026-11-21: a sweep to -195 stalled at step 7 of 13 (pan stop is joint -202, and at that pan the tilt
/// joint sits on its own stop), so the far end keeps 22 degrees from the pan stop.
pub const OM7_CAMERA: Window = Window {
    yaw: (-180.0, 80.0),
    pitch: (-10.0, 30.0),
};

/// Longest single move, seconds: target mode drops out ~9 s after the last command, so long sweeps are split.
pub const MAX_MOVE_S: f64 = 8.0;

const HOME: (f64, f64) = (0.0, 0.0);

/// Targets moving from `from` to `to` at `dps`, split so no move is longer than [`MAX_MOVE_S`].
fn segment(from: (f64, f64), to: (f64, f64), dps: f64) -> Vec<Target> {
    let d = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
    let n = ((d / dps) / MAX_MOVE_S).ceil().max(1.0) as usize;
    (1..=n)
        .map(|k| {
            let f = k as f64 / n as f64;
            Target {
                yaw: from.0 + (to.0 - from.0) * f,
                roll: 0.0,
                pitch: from.1 + (to.1 - from.1) * f,
                seconds: (d / n as f64 / dps).max(0.6),
            }
        })
        .collect()
}

/// Cuts a path (starting and ending at home) into lists of at most [`LIST_MAX`] targets, each ending at home: a
/// list that would overflow is closed with a move home, and the next one starts from home again.
fn lists_of(points: &[(f64, f64)], dps: f64) -> Vec<Vec<Target>> {
    let mut lists = Vec::new();
    let mut cur: Vec<Target> = Vec::new();
    let mut at = HOME;
    let mut i = 0;
    while i < points.len() {
        let p = points[i];
        let seg = segment(at, p, dps);
        let back = segment(p, HOME, dps);
        if cur.len() + seg.len() + back.len() > LIST_MAX && !cur.is_empty() {
            cur.extend(segment(at, HOME, dps));
            lists.push(std::mem::take(&mut cur));
            at = HOME;
            continue;
        }
        cur.extend(seg);
        at = p;
        i += 1;
    }
    if !cur.is_empty() || at != HOME {
        cur.extend(segment(at, HOME, dps));
    }
    if !cur.is_empty() {
        lists.push(cur);
    }
    lists.retain(|l| !l.is_empty());
    lists
}

/// One continuous slow sweep of the whole window: pitch at its bottom, pan to the high end, pan across to the low
/// end, tilt to the top, pan back to the high end, home. Empty for a non-positive speed.
pub fn sweep(w: Window, dps: f64) -> Vec<Vec<Target>> {
    if dps.is_nan() || dps <= 0.0 {
        return vec![];
    }
    let path = [
        (w.yaw.1, w.pitch.0),
        (w.yaw.0, w.pitch.0),
        (w.yaw.0, w.pitch.1),
        (w.yaw.1, w.pitch.1),
    ];
    lists_of(&path, dps)
}

/// Aims at missing directions (yaw, pitch relative to home): keeps those inside the window, merges any within 12°
/// of one already kept, visits them nearest-first from home, every list ending home.
pub fn fill(dirs: &[(f64, f64)], w: Window, dps: f64) -> Vec<Vec<Target>> {
    if dps.is_nan() || dps <= 0.0 {
        return vec![];
    }
    let inside =
        |d: &(f64, f64)| d.0 >= w.yaw.0 && d.0 <= w.yaw.1 && d.1 >= w.pitch.0 && d.1 <= w.pitch.1;
    let dist = |a: (f64, f64), b: (f64, f64)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let mut kept: Vec<(f64, f64)> = Vec::new();
    for d in dirs.iter().filter(|d| inside(d)) {
        if !kept.iter().any(|k| dist(*k, *d) < 12.0) {
            kept.push(*d);
        }
    }
    let mut order = Vec::new();
    let mut at = HOME;
    while !kept.is_empty() {
        let (i, _) = kept
            .iter()
            .enumerate()
            .min_by(|a, b| dist(at, *a.1).total_cmp(&dist(at, *b.1)))
            .unwrap();
        at = kept.remove(i);
        order.push(at);
    }
    if order.is_empty() {
        return vec![];
    }
    lists_of(&order, dps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(lists: &[Vec<Target>]) -> Vec<Target> {
        lists.iter().flatten().copied().collect()
    }

    #[test]
    fn sweep_stays_inside_and_ends_each_list_home() {
        let lists = sweep(OM7_CAMERA, 10.0);
        assert!(!lists.is_empty());
        for l in &lists {
            assert!(
                (1..=crate::targets::LIST_MAX).contains(&l.len()),
                "{}",
                l.len()
            );
            let last = l.last().unwrap();
            assert_eq!((last.yaw, last.roll, last.pitch), (0.0, 0.0, 0.0));
            for t in l {
                assert!(
                    t.yaw >= OM7_CAMERA.yaw.0 - 1e-9 && t.yaw <= OM7_CAMERA.yaw.1 + 1e-9,
                    "{t:?}"
                );
                assert!(
                    t.pitch >= OM7_CAMERA.pitch.0 - 1e-9 && t.pitch <= OM7_CAMERA.pitch.1 + 1e-9,
                    "{t:?}"
                );
                assert!(
                    t.seconds <= MAX_MOVE_S + 1e-9 && t.seconds >= 0.6 - 1e-9,
                    "{t:?}"
                );
            }
        }
        let ts = all(&lists);
        for (yaw, pitch) in [
            (OM7_CAMERA.yaw.0, OM7_CAMERA.pitch.0),
            (OM7_CAMERA.yaw.1, OM7_CAMERA.pitch.0),
            (OM7_CAMERA.yaw.0, OM7_CAMERA.pitch.1),
            (OM7_CAMERA.yaw.1, OM7_CAMERA.pitch.1),
        ] {
            assert!(
                ts.iter()
                    .any(|t| (t.yaw - yaw).abs() < 1e-9 && (t.pitch - pitch).abs() < 1e-9),
                "{yaw},{pitch}"
            );
        }
    }

    #[test]
    fn move_time_is_distance_over_speed() {
        let lists = sweep(
            Window {
                yaw: (-20.0, 20.0),
                pitch: (0.0, 0.0),
            },
            10.0,
        );
        let ts = all(&lists);
        // first move: home (0,0) to (20,0) = 20 deg at 10 deg/s
        assert!((ts[0].seconds - 2.0).abs() < 1e-9, "{:?}", ts[0]);
        assert!(sweep(OM7_CAMERA, 0.0).is_empty());
    }

    #[test]
    fn fill_keeps_reachable_merges_close_and_goes_home() {
        let dirs = [
            (10.0, 5.0),
            (12.0, 6.0),
            (300.0, 0.0),
            (-150.0, 20.0),
            (0.0, 80.0),
        ];
        let lists = fill(&dirs, OM7_CAMERA, 10.0);
        let ts = all(&lists);
        let wanted = [(10.0, 5.0), (-150.0, 20.0)];
        let aims: Vec<_> = ts
            .iter()
            .filter(|t| {
                wanted
                    .iter()
                    .any(|w| (t.yaw - w.0).abs() < 1e-9 && (t.pitch - w.1).abs() < 1e-9)
            })
            .collect();
        assert_eq!(aims.len(), 2, "{aims:?}"); // (12,6) merged into (10,5); 300 and pitch 80 unreachable
        assert!((aims[0].yaw - 10.0).abs() < 1e-9); // nearest first
        for l in &lists {
            let last = l.last().unwrap();
            assert_eq!((last.yaw, last.pitch), (0.0, 0.0));
        }
        assert!(fill(&[], OM7_CAMERA, 10.0).is_empty());
    }
}
