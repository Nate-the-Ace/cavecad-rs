//! Trimming a scan to an outline traced round the part that matters (the sketch), so the rest of
//! the page, the margins, the other sketches and the binder holes, stays out of the views. Only the
//! outline is stored, in scan pixels; the picture is never changed.

use crate::{ScanError, key, key_checked};
use cave_doc::{Action, Document, Field, Kind, Value};
use cave_id::Id;

/// A closed outline: points `[px, py]` in pixels from the scan's top left. The last point joins the first.
pub type Outline = Vec<[f64; 2]>;

/// Most points an outline may have.
pub const MAX_POINTS: usize = 600;

fn encode(o: &Outline) -> String {
    o.iter()
        .map(|p| format!("{},{}", p[0], p[1]))
        .collect::<Vec<_>>()
        .join(";")
}

fn decode(text: &str) -> Option<Outline> {
    text.split(';')
        .map(|p| {
            let (x, y) = p.split_once(',')?;
            Some([x.parse().ok()?, y.parse().ok()?])
        })
        .collect()
}

fn area(o: &Outline) -> f64 {
    let n = o.len();
    (0..n)
        .map(|i| {
            let (a, b) = (o[i], o[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

/// The outline a scan is trimmed to on a trip in a view, or `None` when it is not trimmed.
pub fn trim(doc: &Document, trip: Id, view: &str, path: &str) -> Option<Outline> {
    let e = doc.entity(trip).filter(|e| e.kind() == Kind::Trip)?;
    decode(e.map(Field::TripScanTrim).get(&key(view, path)?)?)
}

/// Sets the outline a scan is trimmed to, or with `None` takes the trim off. The outline needs three
/// or more finite points that are not negative, and some area.
pub fn set_trim(
    doc: &Document,
    trip: Id,
    view: &str,
    path: &str,
    want: Option<Outline>,
) -> Result<Vec<Action>, ScanError> {
    let k = key_checked(view, path)?;
    let e = doc
        .entity(trip)
        .filter(|e| e.kind() == Kind::Trip)
        .ok_or(ScanError::NoSuchTrip)?;
    let mut map = e.map(Field::TripScanTrim);
    match want {
        Some(o) => {
            if o.len() < 3
                || o.len() > MAX_POINTS
                || o.iter()
                    .any(|p| !p[0].is_finite() || !p[1].is_finite() || p[0] < 0.0 || p[1] < 0.0)
                || area(&o).abs() < 1.0
            {
                return Err(ScanError::BadOutline);
            }
            map.insert(k, encode(&o));
        }
        None => {
            map.remove(&k);
        }
    }
    if map == e.map(Field::TripScanTrim) {
        return Ok(Vec::new());
    }
    Ok(vec![Action::Set {
        id: trip,
        field: Field::TripScanTrim,
        value: Value::Map(map),
    }])
}

/// Thins a traced line to the points that matter (Ramer-Douglas-Peucker): no dropped point is
/// further than `tolerance` pixels from the result. A hand-drawn loop of hundreds of points becomes
/// a few dozen, and never more than [`MAX_POINTS`].
pub fn simplify(points: &Outline, tolerance: f64) -> Outline {
    let mut tol = tolerance.max(0.0);
    let mut out = rdp(points, tol);
    while out.len() > MAX_POINTS {
        tol = tol.max(0.5) * 1.5;
        out = rdp(points, tol);
    }
    out
}

fn rdp(points: &[[f64; 2]], tol: f64) -> Outline {
    let n = points.len();
    if n < 3 {
        return points.to_vec();
    }
    // Worked with a stack, not recursion: a long traced line can need thousands of splits.
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0, n - 1)];
    while let Some((i, j)) = stack.pop() {
        let (a, b) = (points[i], points[j]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx.hypot(dy);
        let dist = |p: [f64; 2]| {
            if len < 1e-12 {
                (p[0] - a[0]).hypot(p[1] - a[1])
            } else {
                ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len
            }
        };
        let (mut at, mut far) = (i, 0.0);
        for (k, p) in points.iter().enumerate().take(j).skip(i + 1) {
            let d = dist(*p);
            if d > far {
                (at, far) = (k, d);
            }
        }
        if far > tol {
            keep[at] = true;
            stack.push((i, at));
            stack.push((at, j));
        }
    }
    points
        .iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(p, _)| *p)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_doc::{ActorId, History};

    fn cave() -> (Document, Id) {
        let path = format!(
            "{}/../cave-notebook/tests/data/PitfallCave.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let doc = cave_fixture::from_shot_list(&std::fs::read_to_string(path).unwrap(), ActorId(1))
            .unwrap();
        let trip = doc.ids_of(Kind::Trip)[0];
        (doc, trip)
    }

    #[test]
    fn an_outline_is_stored_undone_and_cleared() {
        let (mut doc, trip) = cave();
        let mut h = History::new();
        let square = vec![[10.0, 10.0], [200.0, 10.0], [200.0, 150.0], [10.0, 150.0]];
        let e = doc
            .edit(set_trim(&doc, trip, "map", "a/p.png", Some(square.clone())).unwrap())
            .unwrap();
        h.record(&e);
        assert_eq!(trim(&doc, trip, "map", "a\\p.png"), Some(square.clone()));
        assert!(
            set_trim(&doc, trip, "map", "a/p.png", Some(square.clone()))
                .unwrap()
                .is_empty()
        );
        let e = doc
            .edit(set_trim(&doc, trip, "map", "a/p.png", None).unwrap())
            .unwrap();
        h.record(&e);
        assert_eq!(trim(&doc, trip, "map", "a/p.png"), None);
        h.undo(&mut doc).unwrap();
        assert_eq!(trim(&doc, trip, "map", "a/p.png"), Some(square));
    }

    #[test]
    fn a_bad_outline_is_refused() {
        let (doc, trip) = cave();
        for bad in [
            vec![[0.0, 0.0], [5.0, 5.0]],
            vec![[0.0, 0.0], [10.0, 10.0], [20.0, 20.0]],
            vec![[0.0, 0.0], [10.0, -1.0], [20.0, 20.0]],
            vec![[0.0, 0.0], [f64::NAN, 1.0], [20.0, 20.0]],
        ] {
            assert_eq!(
                set_trim(&doc, trip, "map", "p.png", Some(bad)),
                Err(ScanError::BadOutline)
            );
        }
        assert_eq!(
            set_trim(&doc, trip, "map", "../p.png", None),
            Err(ScanError::BadPath)
        );
    }

    #[test]
    fn a_traced_loop_is_thinned_without_straying() {
        // 400 points round a circle of radius 100: a handful of points keep it within 1 pixel.
        let loop_: Outline = (0..400)
            .map(|i| {
                let t = f64::from(i) / 400.0 * std::f64::consts::TAU;
                [150.0 + 100.0 * t.cos(), 150.0 + 100.0 * t.sin()]
            })
            .collect();
        let thin = simplify(&loop_, 1.0);
        assert!(thin.len() < 60 && thin.len() > 8, "{}", thin.len());
        for p in &loop_ {
            let near = thin.windows(2).any(|w| {
                let (a, b) = (w[0], w[1]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy))
                    .clamp(0.0, 1.0);
                (p[0] - (a[0] + t * dx)).hypot(p[1] - (a[1] + t * dy)) <= 1.0 + 1e-9
            });
            assert!(near, "a dropped point strayed");
        }
        let noisy: Outline = (0..5000)
            .map(|i| [f64::from(i), if i % 2 == 0 { 0.0 } else { 3.0 }])
            .collect();
        assert!(simplify(&noisy, 0.0).len() <= MAX_POINTS);
    }

    #[test]
    fn unlinking_a_scan_takes_its_trim_with_it() {
        let (mut doc, trip) = cave();
        doc.edit(crate::link(&doc, trip, "p.png").unwrap()).unwrap();
        let sq = vec![[0.0, 0.0], [50.0, 0.0], [50.0, 50.0]];
        doc.edit(set_trim(&doc, trip, "map", "p.png", Some(sq)).unwrap())
            .unwrap();
        doc.edit(crate::unlink(&doc, trip, "p.png").unwrap())
            .unwrap();
        assert_eq!(trim(&doc, trip, "map", "p.png"), None);
    }

    #[test]
    fn the_outline_survives_a_saved_file() {
        let (mut doc, trip) = cave();
        let sq = vec![[0.5, 0.25], [50.0, 0.0], [50.0, 50.75]];
        doc.edit(set_trim(&doc, trip, "map", "p.png", Some(sq)).unwrap())
            .unwrap();
        let text = cave_dxf::export(&doc, &cave_dxf::ExportOptions::default());
        let back = cave_dxf::import(&text, ActorId(2))
            .unwrap()
            .document
            .unwrap();
        assert_eq!(
            trim(&back, trip, "map", "p.png"),
            trim(&doc, trip, "map", "p.png")
        );
    }
}
