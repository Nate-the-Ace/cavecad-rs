//! Edge cases found by fuzzing and reading the import path, kept as regressions.

use cave_doc::ActorId;

fn doc_of(svx: &str) -> cave_doc::Document {
    cave_formats::to_document(&cave_formats::survex::parse(svx), ActorId(1)).unwrap()
}

#[test]
fn a_mistyped_far_fix_does_not_freeze_the_map() {
    // a fix 10^12 m out made the map sample its leg every 30 cm: trillions of steps
    let doc = doc_of("*fix A 999999999999 0 0\n*fix B 0 0 0\nA C 10 0 0\nB C 10 0 0\nB D 5 90 0\n");
    let t = std::time::Instant::now();
    let scene = cave_scene::build(&doc, true);
    assert!(!scene.stations.is_empty());
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
}

mod ties {
    use cave_formats::th2::parse;
    use cave_th2::place::Placement;
    use std::collections::HashMap;

    fn placed(th2: &str, map: &[(&str, [f64; 2])]) -> Option<Placement> {
        let s = parse(th2);
        let m: HashMap<&str, [f64; 2]> = map.iter().cloned().collect();
        Placement::solve(&s.scraps[0], &|n| m.get(n).copied()).ok()
    }

    fn finite(p: &Placement) -> bool {
        [[0.0, 0.0], [50.0, 50.0], [-100.0, 300.0]].iter().all(|q| {
            let a = p.at(*q);
            // the sketch spans ~10 m: anything far beyond is a blown-up fit
            a[0].is_finite() && a[1].is_finite() && a[0].abs() < 200.0 && a[1].abs() < 200.0
        })
    }

    #[test]
    fn degenerate_ties_never_give_nonsense() {
        let cases: &[(&str, &[(&str, [f64; 2])])] = &[
            // the same marker twice
            (
                "point 0 0 station -name a\npoint 0 0 station -name a\npoint 100 0 station -name b\npoint 0 100 station -name c\n",
                &[("a", [0.0, 0.0]), ("b", [10.0, 0.0]), ("c", [0.0, 10.0])],
            ),
            // two markers on one sketch spot, different stations
            (
                "point 0 0 station -name a\npoint 0 0 station -name b\npoint 100 0 station -name c\npoint 0 100 station -name d\n",
                &[
                    ("a", [0.0, 0.0]),
                    ("b", [1.0, 0.0]),
                    ("c", [10.0, 0.0]),
                    ("d", [0.0, 10.0]),
                ],
            ),
            // all in a line
            (
                "point 0 0 station -name a\npoint 50 0 station -name b\npoint 100 0 station -name c\n",
                &[("a", [0.0, 0.0]), ("b", [5.0, 0.0]), ("c", [10.0, 0.0])],
            ),
            // two different spots on one station (equated stations)
            (
                "point 0 0 station -name a\npoint 100 0 station -name b\npoint 0 100 station -name c\n",
                &[("a", [0.0, 0.0]), ("b", [0.0, 0.0]), ("c", [0.0, 10.0])],
            ),
            // every marker on one station
            (
                "point 0 0 station -name a\npoint 100 0 station -name b\n",
                &[("a", [3.0, 3.0]), ("b", [3.0, 3.0])],
            ),
        ];
        for (i, (body, map)) in cases.iter().enumerate() {
            let th2 = format!("scrap s -scale [0 0 100 0 0 0 10 0 m]\n{body}endscrap\n");
            if let Some(p) = placed(&th2, map) {
                assert!(
                    finite(&p),
                    "case {i}: {:?} {:?}",
                    p.at([50.0, 50.0]),
                    p.at([-100.0, 300.0])
                );
            }
        }
    }
}

mod import {
    use cave_doc::ActorId;
    use cave_id::SeqIds;

    /// A straight passage 1 → 2 → 3 east, 10 m legs; the sketch page's report.
    fn import(th2: &str) -> cave_th2::Report {
        let s = cave_formats::therion::parse(
            "survey c\ncentreline\ndata normal from to length compass clino\n1 2 10 90 0\n2 3 10 90 0\nendcentreline\nendsurvey\n",
        );
        let doc = cave_formats::to_document(&s, ActorId(1)).unwrap();
        let sketch = cave_formats::th2::parse(th2);
        let (actions, report) = cave_th2::import(&doc, &mut SeqIds::new(3), &sketch, "", &[], true);
        let mut doc = doc;
        doc.edit(actions).unwrap();
        report
    }

    const TIES: &str = "point 0 0 station -name 1\npoint 100 0 station -name 2\n";

    #[test]
    fn a_station_marker_with_a_subtype_still_ties() {
        let r = import(
            "scrap s\npoint 0 0 station:fixed -name 1\npoint 100 0 station -name 2\nendscrap\n",
        );
        assert_eq!(r.placed.get("two-station"), Some(&1), "{r:?}");
    }

    #[test]
    fn an_island_is_not_a_second_lake() {
        let r = import(&format!(
            "scrap s -scale [0 0 100 0 0 0 10 0 m]\n{TIES}line border -id o -close on\n0 -50\n100 -50\n100 50\n0 50\nendline\nline border -id i -close on\n40 -10\n60 -10\n60 10\n40 10\nendline\narea water\no i\nendarea\nendscrap\n"
        ));
        assert_eq!(r.fills, 1, "{r:?}");
        assert_eq!(r.skipped.get("area-hole"), Some(&1));
    }

    #[test]
    fn an_area_naming_a_missing_line_is_not_guessed() {
        let r = import(&format!(
            "scrap s\n{TIES}line border -id o\n0 -50\n100 -50\n100 50\nendline\narea water\no gone\nendarea\nendscrap\n"
        ));
        assert_eq!(r.fills, 0);
        assert!(
            r.messages("p.th2")
                .iter()
                .any(|m| m.contains("does not have")),
            "{r:?}"
        );
    }

    #[test]
    fn an_unscaled_single_tie_is_said() {
        let r =
            import("scrap s\npoint 0 0 station -name 1\nline wall\n0 0\n10 0\nendline\nendscrap\n");
        assert_eq!(r.unscaled, 1);
        assert!(r.messages("p.th2").iter().any(|m| m.contains("guess")));
    }

    #[test]
    fn a_closed_line_drawn_back_to_its_start_stays_one_ring() {
        let r = import(&format!(
            "scrap s\n{TIES}line pit -close on\n10 10\n20 10\n20 20\n10 10\nendline\nendscrap\n"
        ));
        assert_eq!(r.lines, 1);
    }
}
