//! Placing a scrap in the views other than the map: a run's profile band, and a station's cross
//! section.
//!
//! A profile scrap (Therion's extended or projected elevation, TopoDroid's profile) has its own
//! unrolling of the cave, which never agrees with the bands here: another run order, other extend
//! directions, legs foreshortened in a projected one. So it is not fitted as a whole. Each stroke
//! goes to the run whose legs it was drawn beside, and each of its points follows the nearest leg
//! of that run: along the leg as the leg runs on the band, off it by the same distance it was drawn
//! off it (turned round where the sketch runs the leg the other way). A wall drawn along a
//! passage stays along it, a pit's walls stay either side of the shaft.

use crate::place::{Place, Stations, declared};
use cave_formats::th2::Scrap;
use std::cell::{Cell, RefCell};

/// A survey leg as a profile scrap draws it and as a band has it.
#[derive(Clone, Debug)]
struct BandLeg {
    /// Index into the runs.
    run: usize,
    /// Ends in the scrap, metres (x along, y up).
    sketch: [[f64; 2]; 2],
    /// Ends on the band, metres (distance along the run, elevation).
    band: [[f64; 2]; 2],
    /// +1 where the band runs the leg the way the sketch does, -1 where it runs it backwards.
    sign: f64,
}

/// A run's band: its key and its consecutive station pairs (names, band positions).
pub struct Band {
    /// The run's key.
    pub key: String,
    /// `(station a, station b, [s, z] of a, [s, z] of b)` for each leg along the band.
    pub legs: Vec<(String, String, [f64; 2], [f64; 2])>,
}

/// A profile scrap placed onto the profile bands.
pub struct ProfilePlace {
    legs: Vec<BandLeg>,
    /// Metres per sketch unit.
    k: f64,
    /// The run the stroke being drawn belongs to.
    run: Cell<usize>,
    /// The leg (index into `legs`) the stroke's last point followed: a stroke keeps to it until
    /// another is clearly nearer, so a wall passing between two legs drawn side by side in the
    /// sketch (and far apart on the band) does not jump back and forth between them.
    last: Cell<Option<usize>>,
    /// Points (as placed) where the stroke changed to a leg that places it somewhere else: the
    /// sketch and the band lay the cave out differently here, and the stroke is drawn as two
    /// lines rather than with a line across that was never drawn.
    breaks: RefCell<Vec<[f64; 2]>>,
}

/// How far apart (metres) two legs may place the same point before the stroke is broken there.
const BREAK_M: f64 = 1.0;

/// How much nearer (metres, or a share of the distance) another leg must be to take a stroke off
/// the one it is following.
const KEEP_M: f64 = 0.75;
const KEEP_SHARE: f64 = 0.5;

/// Distance from `p` to the segment `a`-`b`, and where along it (clamped to it) `p` is nearest.
fn nearest_on(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> (f64, f64) {
    let d = [b[0] - a[0], b[1] - a[1]];
    let len2 = d[0] * d[0] + d[1] * d[1];
    let t = if len2 < 1e-12 {
        0.0
    } else {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len2).clamp(0.0, 1.0)
    };
    let q = [a[0] + t * d[0], a[1] + t * d[1]];
    ((p[0] - q[0]).hypot(p[1] - q[1]), t)
}

impl ProfilePlace {
    /// A scrap's placement on `bands`, or `None` when no leg of any band has both its stations
    /// marked in the scrap. `stations` resolves the scrap's tie names to the bands' names.
    pub fn new(scrap: &Scrap, bands: &[Band], scope: &str) -> Option<ProfilePlace> {
        let k = declared(scrap.scale.as_ref()).unwrap_or(1.0);
        let names: Vec<String> = bands
            .iter()
            .flat_map(|b| b.legs.iter().flat_map(|l| [l.0.clone(), l.1.clone()]))
            .collect();
        let stations = Stations::new(names.iter().map(|n| (n.clone(), [0.0, 0.0])), scope);
        // tie name in the scrap -> where the scrap draws it, by the bands' own (lower case) name
        let mut at: std::collections::HashMap<String, [f64; 2]> = Default::default();
        for t in crate::place::ties(scrap) {
            if let Some(n) = stations.resolve(&t.name) {
                at.entry(n).or_insert([k * t.x, k * t.y]);
            }
        }
        let mut legs = Vec::new();
        for (run, band) in bands.iter().enumerate() {
            let mut these: Vec<BandLeg> = band
                .legs
                .iter()
                .filter_map(|(a, b, pa, pb)| {
                    let sa = *at.get(&a.to_lowercase())?;
                    let sb = *at.get(&b.to_lowercase())?;
                    Some(BandLeg {
                        run,
                        sketch: [sa, sb],
                        band: [*pa, *pb],
                        sign: 0.0,
                    })
                })
                .collect();
            // which way the sketch runs this band overall, for legs that do not say (a shaft)
            let overall: f64 = these
                .iter()
                .map(|l| (l.sketch[1][0] - l.sketch[0][0]) * (l.band[1][0] - l.band[0][0]))
                .sum();
            let overall = if overall < 0.0 { -1.0 } else { 1.0 };
            for l in &mut these {
                let (dx, ds) = (l.sketch[1][0] - l.sketch[0][0], l.band[1][0] - l.band[0][0]);
                l.sign = if dx.abs() > 0.05 && ds.abs() > 0.05 {
                    dx.signum() * ds.signum()
                } else {
                    overall
                };
            }
            legs.extend(these);
        }
        (!legs.is_empty()).then(|| ProfilePlace {
            legs,
            k,
            run: Cell::new(0),
            last: Cell::new(None),
            breaks: RefCell::new(Vec::new()),
        })
    }

    /// How many legs the scrap was tied to.
    pub fn legs(&self) -> usize {
        self.legs.len()
    }

    /// Where leg `i` places sketch point `p` (metres), `t` along it.
    fn along(&self, i: usize, t: f64, p: [f64; 2]) -> [f64; 2] {
        let l = &self.legs[i];
        let [a, b] = l.sketch;
        let on = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
        let [pa, pb] = l.band;
        [
            pa[0] + t * (pb[0] - pa[0]) + l.sign * (p[0] - on[0]),
            pa[1] + t * (pb[1] - pa[1]) + (p[1] - on[1]),
        ]
    }

    /// The stroke's points split where it changed legs across a gap (see `breaks`): each piece
    /// is drawn as a line of its own. Clears what was noted.
    pub fn split(&self, pts: &[[f64; 2]]) -> Vec<Vec<[f64; 2]>> {
        let breaks = std::mem::take(&mut *self.breaks.borrow_mut());
        let mut out: Vec<Vec<[f64; 2]>> = vec![Vec::new()];
        for p in pts {
            if breaks.contains(p) && !out.last().is_some_and(Vec::is_empty) {
                out.push(Vec::new());
            }
            out.last_mut().unwrap().push(*p);
        }
        out
    }

    /// How far (metres) a sketch point is from the nearest leg it is tied to.
    pub fn reach(&self, p: [f64; 2]) -> f64 {
        let p = [self.k * p[0], self.k * p[1]];
        self.legs
            .iter()
            .map(|l| nearest_on(p, l.sketch[0], l.sketch[1]).0)
            .fold(f64::INFINITY, f64::min)
    }

    /// The run (index into the bands) of the stroke being drawn.
    pub fn run(&self) -> usize {
        self.run.get()
    }

    fn nearest(&self, p: [f64; 2], run: Option<usize>) -> Option<(usize, f64, f64)> {
        self.legs
            .iter()
            .enumerate()
            .filter(|(_, l)| run.is_none_or(|r| l.run == r))
            .map(|(i, l)| {
                let (d, t) = nearest_on(p, l.sketch[0], l.sketch[1]);
                (i, d, t)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}

impl Place for ProfilePlace {
    fn stroke(&self, raw: &[[f64; 2]]) {
        // the run most of the stroke lies beside
        let mut votes: std::collections::HashMap<usize, usize> = Default::default();
        self.last.set(None);
        self.breaks.borrow_mut().clear();
        for p in raw {
            if let Some((i, _, _)) = self.nearest([self.k * p[0], self.k * p[1]], None) {
                *votes.entry(self.legs[i].run).or_default() += 1;
            }
        }
        if let Some((run, _)) = votes
            .into_iter()
            .max_by_key(|(r, n)| (*n, std::cmp::Reverse(*r)))
        {
            self.run.set(run);
        }
    }

    fn at(&self, p: [f64; 2]) -> [f64; 2] {
        let p = [self.k * p[0], self.k * p[1]];
        let Some((mut i, d, mut t)) = self.nearest(p, Some(self.run.get())) else {
            return p;
        };
        let previous = self
            .last
            .get()
            .filter(|j| self.legs[*j].run == self.run.get());
        if let Some(j) = previous {
            let (dj, tj) = nearest_on(p, self.legs[j].sketch[0], self.legs[j].sketch[1]);
            if dj <= d + KEEP_M.max(KEEP_SHARE * d) {
                (i, t) = (j, tj);
            }
        }
        self.last.set(Some(i));
        let q = self.along(i, t, p);
        if let Some(j) = previous.filter(|j| *j != i) {
            let (_, tj) = nearest_on(p, self.legs[j].sketch[0], self.legs[j].sketch[1]);
            let was = self.along(j, tj, p);
            if (was[0] - q[0]).hypot(was[1] - q[1]) > BREAK_M {
                self.breaks.borrow_mut().push(q);
            }
        }
        q
    }
}

/// A scrap placed by a plain function of its points (a cross section: sketch units to the
/// section's `[across, up]` metres).
pub struct Mapped<F: Fn([f64; 2]) -> [f64; 2]>(pub F);

impl<F: Fn([f64; 2]) -> [f64; 2]> Place for Mapped<F> {
    fn at(&self, p: [f64; 2]) -> [f64; 2] {
        (self.0)(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cave_formats::th2::{Scale, StationTie};

    fn scrap(ties: &[(&str, f64, f64)]) -> Scrap {
        Scrap {
            name: "s".into(),
            projection: Some("extended".into()),
            projection_angle: None,
            scale: Some(Scale {
                units_per_drawing: 1.0,
                unit: "m".into(),
            }),
            options: Default::default(),
            stations: ties
                .iter()
                .map(|(n, x, y)| StationTie {
                    name: n.to_string(),
                    x: *x,
                    y: *y,
                })
                .collect(),
            lines: vec![],
            areas: vec![],
            points: vec![],
            sections: vec![],
        }
    }

    fn band(key: &str, legs: &[(&str, &str, [f64; 2], [f64; 2])]) -> Band {
        Band {
            key: key.into(),
            legs: legs
                .iter()
                .map(|(a, b, pa, pb)| (a.to_string(), b.to_string(), *pa, *pb))
                .collect(),
        }
    }

    #[test]
    fn a_wall_follows_its_leg_onto_the_band() {
        // the sketch draws 1-2 going right 10 m; the band has it at s 100..110, 50 m up
        let s = scrap(&[("1", 0.0, 0.0), ("2", 10.0, 0.0)]);
        let p = ProfilePlace::new(
            &s,
            &[band("r", &[("1", "2", [100.0, 50.0], [110.0, 50.0])])],
            "",
        )
        .unwrap();
        p.stroke(&[[5.0, 2.0]]);
        let q = p.at([5.0, 2.0]);
        assert!(
            (q[0] - 105.0).abs() < 1e-9 && (q[1] - 52.0).abs() < 1e-9,
            "{q:?}"
        );
    }

    #[test]
    fn a_sketch_running_the_other_way_is_turned_round() {
        let s = scrap(&[("1", 0.0, 0.0), ("2", -10.0, -3.0)]);
        let p = ProfilePlace::new(
            &s,
            &[band("r", &[("1", "2", [0.0, 0.0], [10.0, -3.0])])],
            "",
        )
        .unwrap();
        p.stroke(&[[-5.0, 1.0]]);
        let q = p.at([-5.0, 1.0]);
        assert!(
            (q[0] - 5.0).abs() < 1e-9 && (q[1] - 1.0).abs() < 1e-9,
            "{q:?}"
        );
    }

    #[test]
    fn a_shafts_walls_stay_either_side() {
        // a 20 m shaft: the sketch draws it straight down at x 0, the band at s 30; the run goes
        // right on the band and in the sketch
        let s = scrap(&[("0", -10.0, 0.0), ("1", 0.0, 0.0), ("2", 0.0, -20.0)]);
        let b = band(
            "r",
            &[
                ("0", "1", [20.0, 0.0], [30.0, 0.0]),
                ("1", "2", [30.0, 0.0], [30.0, -20.0]),
            ],
        );
        let p = ProfilePlace::new(&s, &[b], "").unwrap();
        p.stroke(&[[2.0, -10.0]]);
        let q = p.at([2.0, -10.0]);
        assert!(
            (q[0] - 32.0).abs() < 1e-9 && (q[1] + 10.0).abs() < 1e-9,
            "{q:?}"
        );
        let q = p.at([-2.0, -10.0]);
        assert!((q[0] - 28.0).abs() < 1e-9, "{q:?}");
    }

    #[test]
    fn a_stroke_goes_to_the_run_it_was_drawn_beside() {
        let s = scrap(&[("1", 0.0, 0.0), ("2", 10.0, 0.0), ("3", 10.0, -10.0)]);
        let bands = [
            band("a", &[("1", "2", [0.0, 0.0], [10.0, 0.0])]),
            band("b", &[("2", "3", [0.0, 0.0], [0.0, -10.0])]),
        ];
        let p = ProfilePlace::new(&s, &bands, "").unwrap();
        p.stroke(&[[11.0, -4.0], [11.0, -6.0], [5.0, 1.0]]);
        assert_eq!(p.run(), 1);
    }

    #[test]
    fn a_stroke_crossing_a_layout_change_is_split_not_joined() {
        // the sketch draws 1-2 going left and 2-3 going right; the band runs both right
        let s = scrap(&[("1", 0.0, 0.0), ("2", -10.0, 0.0), ("3", 0.0, 0.1)]);
        let b = band(
            "r",
            &[
                ("1", "2", [0.0, 0.0], [10.0, 0.0]),
                ("2", "3", [10.0, 0.0], [20.0, 0.1]),
            ],
        );
        let p = ProfilePlace::new(&s, &[b], "").unwrap();
        let raw = [[-2.0, 2.0], [-2.0, 3.0], [-2.0, 4.0]];
        p.stroke(&raw);
        let placed: Vec<[f64; 2]> = raw.iter().map(|q| p.at(*q)).collect();
        // the middle point is as near one leg as the other; wherever the stroke changes legs it
        // splits, so no piece spans the 16 m between them
        for piece in p.split(&placed) {
            for w in piece.windows(2) {
                assert!((w[0][0] - w[1][0]).abs() < 2.0, "{piece:?}");
            }
        }
    }

    #[test]
    fn nothing_tied_places_nothing() {
        let s = scrap(&[("9", 0.0, 0.0)]);
        assert!(
            ProfilePlace::new(&s, &[band("r", &[("1", "2", [0.0, 0.0], [1.0, 0.0])])], "")
                .is_none()
        );
    }
}
