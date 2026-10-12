//! The Rust passage mesh must match the old CaveCAD (JavaScript) mesh on its own test caves:
//! the same triangles, normals and colours, vertex for vertex. (Old values are in feet; ours are
//! metres, so positions are scaled and everything else compares directly.)

use cave_doc::ActorId;
use cave_id::Id;
use cave_mesh::{ColorMode, LoftMode, Mesh, MeshInput, build};
use cave_survey::{Vec3, extract, resolve};
use serde_json::Value;
use std::collections::BTreeMap;

const FEET: f64 = 0.3048;

fn mesh_of(text: &str, color: ColorMode) -> Mesh {
    let doc = cave_fixture::from_shot_list(text, ActorId(1)).unwrap();
    let ex = extract(&doc);
    let r = resolve(&ex.legs, &ex.fixed);
    let stations: BTreeMap<Id, Vec3> = r.stations.iter().map(|(id, p)| (*id, p.pos)).collect();
    build(&MeshInput {
        stations: &stations,
        legs: &r.legs,
        splays: &ex.splays,
        first_arrival_lruds: &ex.first_arrival_lruds,
        start_lruds: &ex.start_lruds,
        station_trip: &ex.station_trip,
        color,
        loft: LoftMode::Nearest,
        fill_gaps: false,
    })
}

fn floats(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

fn same(file: &str, what: &str, got: &[f64], want: &[f64], scale: f64, tol: f64) {
    assert_eq!(got.len(), want.len(), "{file} {what}: length");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(
            (g - w * scale).abs() < tol,
            "{file} {what}[{i}]: {g} vs {}",
            w * scale
        );
    }
}

fn check(file: &str) {
    let path = format!(
        "{}/../cave-survey/tests/golden/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let want: Value = serde_json::from_str(&text).unwrap();
    let want = &want["mesh"];

    let trip = mesh_of(&text, ColorMode::Trip);
    assert_eq!(
        trip.indices.len() as u64,
        want["indexCount"].as_u64().unwrap(),
        "{file}: triangle count"
    );
    same(
        file,
        "positions",
        &trip.positions,
        &floats(&want["positions"]),
        FEET,
        2e-4,
    );
    same(
        file,
        "normals",
        &trip.normals,
        &floats(&want["normals"]),
        1.0,
        2e-4,
    );
    same(
        file,
        "trip colours",
        &trip.colors,
        &floats(&want["colors"]),
        1.0,
        2e-3,
    );

    let depth = mesh_of(&text, ColorMode::Depth);
    same(
        file,
        "depth colours",
        &depth.colors,
        &floats(&want["depthColors"]),
        1.0,
        2e-3,
    );

    if let Some((lo, hi)) = trip.bounds {
        let b = &want["bounds"];
        for (got, key, axis) in [
            (lo.x, "min", "x"),
            (lo.y, "min", "y"),
            (lo.z, "min", "z"),
            (hi.x, "max", "x"),
            (hi.y, "max", "y"),
            (hi.z, "max", "z"),
        ] {
            let w = b[key][axis].as_f64().unwrap() * FEET;
            assert!(
                (got - w).abs() < 1e-3,
                "{file} bounds {key}.{axis}: {got} vs {w}"
            );
        }
    }
}

#[test]
fn pitfall_mesh_matches_the_old_mesh() {
    check("PitfallCave.json");
}

#[test]
fn fingerprint_mesh_matches_the_old_mesh() {
    check("FingerprintCave.json");
}

#[test]
fn plumbline_pit_mesh_matches_the_old_mesh() {
    check("PlumblinePit.json");
}

#[test]
fn stairstep_mesh_matches_the_old_mesh() {
    check("StairstepCave.json");
}
