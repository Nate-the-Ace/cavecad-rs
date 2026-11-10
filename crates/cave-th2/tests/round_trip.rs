//! Real caves out through every writer and back in: every station must land where it did. Local
//! data only: `REAL_RT=<file;file> cargo test --release -p cave-th2 --test round_trip -- --ignored --nocapture`

use cave_doc::ActorId;
use std::collections::HashMap;

fn positions(s: &cave_formats::Survey) -> HashMap<String, [f64; 3]> {
    let d = cave_formats::to_document(s, ActorId(1)).unwrap();
    cave_scene::build(&d, true)
        .stations
        .into_iter()
        .map(|st| (st.label, st.at))
        .collect()
}

#[test]
#[ignore]
fn real_caves_survive_every_writer() {
    let list = std::env::var("REAL_RT").unwrap_or_default();
    let mut bad = Vec::new();
    for top in list.split(';').filter(|s| !s.is_empty()) {
        let read = |p: &str| {
            std::fs::read(p)
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
        };
        let (_, survey) = cave_formats::read_any(top, &read(top).unwrap(), &read).unwrap();
        let doc = cave_formats::to_document(&survey, ActorId(1)).unwrap();
        let start = positions(&cave_formats::from_document(&doc));
        for f in cave_formats::FORMATS {
            let mut out = cave_formats::from_document(&doc);
            let text = f.write(&mut out);
            let renamed = out.findings().iter().any(|x| x.code == "export-renamed");
            let back = positions(&f.parse(&text));
            let mut worst = (0.0f64, String::new());
            let mut lost = 0;
            for (name, p) in &start {
                match back.get(name) {
                    Some(q) => {
                        let d =
                            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2))
                                .sqrt();
                        if d > worst.0 {
                            worst = (d, name.clone());
                        }
                    }
                    None => lost += 1,
                }
            }
            println!(
                "{:<60} {:<8} stations {:>5}/{:<5} lost {:>4} worst {:.3} m at {}{}",
                top.rsplit('/').next().unwrap(),
                f.id(),
                back.len(),
                start.len(),
                lost,
                worst.0,
                worst.1,
                if renamed { " (renamed)" } else { "" }
            );
            if worst.0 > 0.02 || (lost > 0 && !renamed) {
                bad.push(format!("{top} {}", f.id()));
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
