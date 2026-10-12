//! Reads a DXF and checks it restores the cave built from a shot list.
//!   cargo run -p cave-dxf --example verify_demo -- crates/cave-survey/tests/golden/PitfallCave.json some.dxf

use cave_doc::ActorId;
use cave_dxf::import;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let original = cave_fixture::from_shot_list(
        &std::fs::read_to_string(&args[1]).expect("read json"),
        ActorId(1),
    )
    .expect("parse cave");
    let text = std::fs::read_to_string(&args[2]).expect("read dxf");
    match import(&text, ActorId(2)) {
        Ok(i) => println!(
            "loaded: {} | exact match with the original: {} | warnings: {:?} | drawing entities: {}",
            i.document.is_some(),
            i.document.as_ref() == Some(&original),
            i.warnings,
            i.entities
        ),
        Err(e) => println!("error: {e:?}"),
    }
}
