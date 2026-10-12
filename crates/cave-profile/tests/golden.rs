//! The Rust profile bands must match the old CaveCAD (JavaScript) bands on its own test caves:
//! the same runs, the same station order, the same distances along the run and elevations.
//! (Old values are in feet; ours are metres.)

use cave_doc::{ActorId, Field};
use cave_id::Id;
use cave_profile::{Graph, LegInfo, group_runs, hierarchy, unroll_band};
use cave_survey::{extract, resolve};
use serde_json::Value;
use std::collections::HashMap;

const FEET: f64 = 0.3048;

fn check(file: &str) {
    let path = format!(
        "{}/../cave-survey/tests/golden/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let want: Value = serde_json::from_str(&text).unwrap();
    let want = &want["profile"];

    let doc = cave_fixture::from_shot_list(&text, ActorId(1)).unwrap();
    let ex = extract(&doc);
    let r = resolve(&ex.legs, &ex.fixed);

    let mut order: Vec<(usize, Id)> = r.stations.iter().map(|(id, p)| (p.seq, *id)).collect();
    order.sort();
    let order: Vec<Id> = order.into_iter().map(|(_, id)| id).collect();
    let labels: HashMap<Id, String> = order
        .iter()
        .map(|id| {
            (
                *id,
                doc.entity(*id)
                    .map(|e| e.text(Field::StationLabel))
                    .unwrap_or_default(),
            )
        })
        .collect();
    let label = |id: &Id| labels[id].clone();

    let runs = group_runs(&labels, &order);
    let keys: Vec<&str> = runs.runs.iter().map(|r| r.key.as_str()).collect();
    let want_keys: Vec<&str> = want["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    assert_eq!(keys, want_keys, "{file}: run order");
    assert_eq!(
        runs.ungrouped.len(),
        want["ungrouped"].as_array().unwrap().len(),
        "{file}: ungrouped"
    );

    let plan: HashMap<Id, f64> = ex
        .legs
        .iter()
        .map(|l| (l.shot, l.offset.horizontal()))
        .collect();
    let infos: Vec<LegInfo> = r
        .legs
        .iter()
        .map(|l| LegInfo {
            shot: l.shot,
            from: l.from,
            to: l.to,
            kind: l.kind,
            plan: plan[&l.shot],
        })
        .collect();
    let graph = Graph::new(&infos);
    let seq: HashMap<Id, usize> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let hier = hierarchy(&runs, &infos, &seq);
    for run in &runs.runs {
        let want_tie = want["hierarchy"]["ties"][&run.key].as_str();
        let want_parent = want["hierarchy"]["parents"][&run.key].as_str();
        assert_eq!(
            hier.ties[&run.key].map(|t| label(&t)).as_deref(),
            want_tie,
            "{file} run {}: tie station",
            run.key
        );
        assert_eq!(
            hier.parents[&run.key].as_deref(),
            want_parent,
            "{file} run {}: parent run",
            run.key
        );
    }
    let z_of = |id: Id| r.stations.get(&id).map(|p| p.pos.z);

    for run in &runs.runs {
        let w = &want["bands"][&run.key];
        let names: Vec<String> = run.stations.iter().map(label).collect();
        let want_names: Vec<&str> = w["runStations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        assert_eq!(names, want_names, "{file} run {}: member order", run.key);

        let tie = hier.ties[&run.key];
        let band = unroll_band(run, tie, &graph, &labels, &z_of, &ex.lruds);
        let stations = w["stations"].as_array().unwrap();
        assert_eq!(
            band.stations.len(),
            stations.len(),
            "{file} run {}: band length",
            run.key
        );
        for (g, e) in band.stations.iter().zip(stations) {
            assert_eq!(
                g.label,
                e[0].as_str().unwrap(),
                "{file} run {}: station order",
                run.key
            );
            let (s, z) = (e[1].as_f64().unwrap() * FEET, e[2].as_f64().unwrap() * FEET);
            assert!(
                (g.s - s).abs() < 1e-6 && (g.z - z).abs() < 1e-6,
                "{file} run {} {}: ({:.4},{:.4}) vs ({s:.4},{z:.4})",
                run.key,
                g.label,
                g.s,
                g.z
            );
        }
        assert_eq!(
            band.tie.map(|t| label(&t)),
            w["tie"].as_str().map(String::from),
            "{file} run {}: tie",
            run.key
        );
        assert_eq!(
            band.stopped.is_some(),
            !w["stopped"].is_null(),
            "{file} run {}: stopped",
            run.key
        );
        let omitted: Vec<String> = band.omitted.iter().map(label).collect();
        let want_omitted: Vec<&str> = w["omitted"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        assert_eq!(omitted, want_omitted, "{file} run {}: omitted", run.key);
    }
}

#[test]
fn pitfall_profile_matches_the_old_profile() {
    check("PitfallCave.json");
}

#[test]
fn fingerprint_profile_matches_the_old_profile() {
    check("FingerprintCave.json");
}

#[test]
fn plumbline_pit_profile_matches_the_old_profile() {
    check("PlumblinePit.json");
}

#[test]
fn stairstep_profile_matches_the_old_profile() {
    check("StairstepCave.json");
}
