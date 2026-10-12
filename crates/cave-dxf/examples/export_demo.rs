//! Writes a demo cave (the synthetic test shot lists) as DXF, for checking in other programs.
//!   cargo run -p cave-dxf --example export_demo -- crates/cave-survey/tests/golden/PitfallCave.json /tmp/pitfall.dxf

use cave_doc::ActorId;
use cave_dxf::{ExportOptions, export};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (input, output) = (&args[1], &args[2]);
    let doc = cave_fixture::from_shot_list(
        &std::fs::read_to_string(input).expect("read input"),
        ActorId(1),
    )
    .expect("parse cave");
    std::fs::write(output, export(&doc, &ExportOptions::default())).expect("write dxf");
    println!("wrote {output}");
}
