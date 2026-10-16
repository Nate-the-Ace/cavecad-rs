//! The readers against real cave survey data, checked by Survex itself.
//!
//! For every root file in `REAL_DATA` (paths separated by `;`), Survex's `cavern` computes the
//! station positions and `dump3d` writes them out; the same file is read here with
//! [`cave_formats::read_any`], placed by our survey math, and compared station by station.
//! Real data -- decades of hand-typed notes -- finds what made-up test files do not.
//!
//! The data stays on this machine: it holds real entrance locations and belongs to the clubs that
//! surveyed it. Nothing from it is copied into this repository. Run with
//! `SURVEX_BIN=<dir with cavern and dump3d> SURVEXLIB=<survex lib dir> REAL_DATA=<file;file>
//! cargo test -p cave-formats --test real_data -- --ignored --nocapture`.

use cave_doc::{ActorId, Field};
use cave_survey::{Params, adjust, extract, resolve};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Reads a file the way Windows would: exact name first, then ignoring case at each level.
fn read_fs(p: &str) -> Option<String> {
    let path = PathBuf::from(p);
    if let Ok(t) = std::fs::read(&path) {
        return Some(String::from_utf8_lossy(&t).into_owned());
    }
    let mut cur = PathBuf::from("/");
    for part in path.components().skip(1) {
        let want = part.as_os_str().to_string_lossy().to_lowercase();
        let next = std::fs::read_dir(&cur)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().to_lowercase() == want)?;
        cur = next.path();
    }
    std::fs::read(cur)
        .ok()
        .map(|t| String::from_utf8_lossy(&t).into_owned())
}

/// Is this a Therion file (checked by Therion rather than Survex)?
fn is_therion(root: &Path) -> bool {
    let n = root
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    n.ends_with(".th") || n.contains("thconfig")
}

/// A Therion configuration exporting the station positions of `root` to `out` (a .th file, or the
/// sources of a thconfig).
fn therion_config(root: &Path, out: &Path) -> String {
    let dir = root.parent().unwrap_or(Path::new("."));
    let mut sources = Vec::new();
    if root
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("th"))
    {
        sources.push(root.to_path_buf());
    } else {
        for l in std::fs::read_to_string(root).unwrap_or_default().lines() {
            let t = l.split('#').next().unwrap_or("").trim();
            if let Some(rest) = t.strip_prefix("source") {
                for f in rest.split_whitespace() {
                    sources.push(dir.join(f.trim_matches('"')));
                }
            }
        }
    }
    let mut cfg = String::from("encoding utf-8\n");
    for s in sources {
        cfg += &format!("source \"{}\"\n", s.display());
    }
    cfg += &format!("export model -fmt survex -o \"{}\"\n", out.display());
    cfg
}

/// What Survex (or, for Therion files, Therion) makes of a file: station name to position (metres).
fn survex(root: &Path) -> Result<HashMap<String, [f64; 3]>, String> {
    let bin = PathBuf::from(std::env::var("SURVEX_BIN").map_err(|_| "SURVEX_BIN not set")?);
    let out = std::env::temp_dir().join(format!("cavecad-real-{}", std::process::id()));
    let (program, run) = if is_therion(root) {
        let therion = PathBuf::from(std::env::var("THERION").map_err(|_| "THERION not set")?);
        let cfg = out.with_extension("thconfig");
        std::fs::write(&cfg, therion_config(root, &out.with_extension("3d")))
            .map_err(|e| e.to_string())?;
        (
            "therion",
            Command::new(therion)
                .arg(&cfg)
                .current_dir(std::env::temp_dir())
                .output()
                .map_err(|e| e.to_string())?,
        )
    } else {
        (
            "cavern",
            Command::new(bin.join("cavern"))
                .arg(format!("--output={}", out.display()))
                .arg(root)
                .current_dir(root.parent().unwrap_or(Path::new(".")))
                .output()
                .map_err(|e| e.to_string())?,
        )
    };
    if !run.status.success() {
        let text = String::from_utf8_lossy(&run.stdout).to_string()
            + &String::from_utf8_lossy(&run.stderr);
        let err = text
            .lines()
            .filter(|l| l.contains("error"))
            .take(3)
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(format!("{program} failed: {err}"));
    }
    let dump = Command::new(bin.join("dump3d"))
        .arg(out.with_extension("3d"))
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&dump.stdout);
    let mut nodes = HashMap::new();
    for l in text.lines().filter(|l| l.starts_with("NODE ")) {
        let (Some(a), Some(b)) = (l.find('['), l.rfind(']')) else {
            continue;
        };
        let v: Vec<f64> = l[5..a]
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        if v.len() == 3 && !l[a + 1..b].is_empty() {
            nodes.insert(l[a + 1..b].to_string(), [v[0], v[1], v[2]]);
        }
    }
    Ok(nodes)
}

/// Each leg's measured vector (the reading itself, before any adjustment), by "from -> to".
type Raw = HashMap<(String, String), [f64; 3]>;

/// Our positions, by station name, in metres, and the raw leg vectors.
fn ours(survey: &cave_formats::Survey) -> Result<(HashMap<String, [f64; 3]>, Raw), String> {
    let doc = cave_formats::to_document(survey, ActorId(1))?;
    let ex = extract(&doc);
    let r = resolve(&ex.legs, &ex.fixed);
    let a = adjust(&ex.legs, &r, &Params::default());
    let mut placed = a.stations.clone();
    for f in &ex.fixed {
        placed.entry(f.station).or_insert(f.pos);
    }
    let name = |id| {
        doc.entity(id)
            .map(|e| e.text(Field::StationLabel).to_lowercase())
            .unwrap_or_default()
    };
    let raw = ex
        .legs
        .iter()
        .map(|l| {
            (
                (name(l.from), name(l.to)),
                [l.offset.x, l.offset.y, l.offset.z],
            )
        })
        .collect();
    Ok((
        placed
            .iter()
            .filter_map(|(id, p)| {
                Some((doc.entity(*id)?.text(Field::StationLabel), [p.x, p.y, p.z]))
            })
            .collect(),
        raw,
    ))
}

#[test]
#[ignore = "needs real data and Survex: set REAL_DATA, SURVEX_BIN, SURVEXLIB"]
fn real_data_against_survex() {
    let Ok(list) = std::env::var("REAL_DATA") else {
        eprintln!("REAL_DATA not set");
        return;
    };
    for root in list.split(';').filter(|s| !s.is_empty()).map(PathBuf::from) {
        let name = root.display().to_string();
        let started = std::time::Instant::now();
        let want = match survex(&root) {
            Ok(w) => w,
            Err(e) => {
                println!("{name}: {e}");
                continue;
            }
        };
        let text = read_fs(&name).unwrap_or_default();
        let (label, survey) = match cave_formats::read_any(&name, &text, &read_fs) {
            Ok(r) => r,
            Err(e) => {
                println!("{name}: {e}");
                continue;
            }
        };
        let (got, raw) = match ours(&survey) {
            Ok(g) => g,
            Err(e) => {
                println!("{name}: no cave: {e}");
                continue;
            }
        };
        // a station joined by *equate is in Survex's list under every name, in ours under one
        let aliases: HashMap<String, String> = survey
            .aliases
            .iter()
            .map(|(a, k)| (a.to_lowercase(), k.to_lowercase()))
            .collect();
        let key = |n: &str| {
            let n = n.to_lowercase();
            aliases.get(&n).cloned().unwrap_or(n)
        };
        let mut want: HashMap<String, [f64; 3]> =
            want.into_iter().map(|(k, v)| (key(&k), v)).collect();
        let got: HashMap<String, [f64; 3]> = got.into_iter().map(|(k, v)| (key(&k), v)).collect();
        // Therion names a station with every survey around it; ours leave out a file's single
        // outermost survey (it is the cave). Drop it from the reference when that matches better.
        if is_therion(&root) {
            let stripped: HashMap<String, [f64; 3]> = want
                .iter()
                .map(|(k, v)| (key(k.split_once('.').map_or(k.as_str(), |(_, r)| r)), *v))
                .collect();
            let hits =
                |m: &HashMap<String, [f64; 3]>| m.keys().filter(|k| got.contains_key(*k)).count();
            if hits(&stripped) > hits(&want) {
                want = stripped;
            }
        }
        let mut common: Vec<&String> = want.keys().filter(|k| got.contains_key(*k)).collect();
        common.sort();
        let Some(first) = common.first() else {
            println!(
                "{name}: no stations in common ({} Survex, {} ours)",
                want.len(),
                got.len()
            );
            let mut w: Vec<&String> = want.keys().collect();
            w.sort();
            let mut g: Vec<&String> = got.keys().collect();
            g.sort();
            println!(
                "  Survex e.g. {:?}\n  ours e.g.   {:?}",
                &w[..w.len().min(5)],
                &g[..g.len().min(5)]
            );
            continue;
        };
        let shift = if survey.fixed.is_empty() {
            let (a, b) = (want[*first], got[*first]);
            [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
        } else {
            [0.0; 3]
        };
        let mut errs: Vec<(f64, &String)> = common
            .iter()
            .map(|k| {
                let (a, b) = (want[*k], got[*k]);
                (
                    ((a[0] - b[0] - shift[0]).powi(2)
                        + (a[1] - b[1] - shift[1]).powi(2)
                        + (a[2] - b[2] - shift[2]).powi(2))
                    .sqrt(),
                    *k,
                )
            })
            .collect();
        errs.sort_by(|a, b| b.0.total_cmp(&a.0));
        let within = |m: f64| errs.iter().filter(|e| e.0 <= m).count();
        println!(
            "{name}\n  read as {label}; {} stations Survex, {} ours, {} in common; within 5 cm {}, 50 cm {}, 5 m {}; worst {:.2} m at {} ({:.1}s){}",
            want.len(),
            got.len(),
            common.len(),
            within(0.05),
            within(0.5),
            within(5.0),
            errs[0].0,
            errs[0].1,
            started.elapsed().as_secs_f64(),
            if survey.findings().is_empty() {
                String::new()
            } else {
                format!("; {} findings", survey.findings().len())
            }
        );
        for (d, k) in errs.iter().take(5).filter(|e| e.0 > 0.05) {
            println!("    {d:.2} m  {k}");
        }
        if let Ok(prefix) = std::env::var("DETAIL") {
            for k in common.iter().filter(|k| k.starts_with(&prefix)) {
                let (a, b) = (want[*k], got[*k]);
                println!(
                    "    {k:<40} survex {:>12.3} {:>12.3} {:>9.3}  ours {:>12.3} {:>12.3} {:>9.3}",
                    a[0],
                    a[1],
                    a[2],
                    b[0] + shift[0],
                    b[1] + shift[1],
                    b[2] + shift[2]
                );
            }
        }
        // Each leg's vector, Survex against ours. A leg on no loop must agree exactly whatever the
        // adjustment did, so a big difference here is a reading read wrong; a small one is the
        // loop closure being shared out differently.
        let mut legs: Vec<(f64, String, f64, f64)> = Vec::new();
        for s in survey
            .shots
            .iter()
            .filter(|s| !s.splay && !s.to.is_empty() && !s.exclude_from_all)
        {
            let (f, t) = (key(&s.from), key(&s.to));
            if let (Some(wf), Some(wt), Some(gf), Some(gt)) =
                (want.get(&f), want.get(&t), got.get(&f), got.get(&t))
            {
                let d = (0..3)
                    .map(|i| ((wt[i] - wf[i]) - (gt[i] - gf[i])).powi(2))
                    .sum::<f64>()
                    .sqrt();
                // how far each placed the leg from what was read
                let r = raw
                    .get(&(s.from.to_lowercase(), s.to.to_lowercase()))
                    .copied()
                    .unwrap_or([f64::NAN; 3]);
                let sx = (0..3)
                    .map(|i| ((wt[i] - wf[i]) - r[i]).powi(2))
                    .sum::<f64>()
                    .sqrt();
                let us = (0..3)
                    .map(|i| ((gt[i] - gf[i]) - r[i]).powi(2))
                    .sum::<f64>()
                    .sqrt();
                legs.push((d, format!("{} -> {}", s.from, s.to), sx, us));
            }
        }
        legs.sort_by(|a, b| b.0.total_cmp(&a.0));
        // the typical turn between their legs and ours: a declination difference shows here
        let mut turns: Vec<f64> = Vec::new();
        for s in survey.shots.iter().filter(|s| !s.splay && !s.to.is_empty()) {
            let (f, t) = (key(&s.from), key(&s.to));
            if let (Some(wf), Some(wt), Some(gf), Some(gt)) =
                (want.get(&f), want.get(&t), got.get(&f), got.get(&t))
            {
                let (wx, wy, gx, gy) = (wt[0] - wf[0], wt[1] - wf[1], gt[0] - gf[0], gt[1] - gf[1]);
                if wx.hypot(wy) > 5.0 && gx.hypot(gy) > 5.0 {
                    let d = (wx.atan2(wy) - gx.atan2(gy)).to_degrees();
                    turns.push((d + 540.0) % 360.0 - 180.0);
                }
            }
        }
        if !turns.is_empty() {
            turns.sort_by(f64::total_cmp);
            println!(
                "  typical leg turn (theirs - ours), median of {}: {:.3} degrees",
                turns.len(),
                turns[turns.len() / 2]
            );
        }
        let legs_within = |m: f64| legs.iter().filter(|l| l.0 <= m).count();
        println!(
            "  legs compared {}: vector within 1 cm {}, 10 cm {}, 1 m {}",
            legs.len(),
            legs_within(0.01),
            legs_within(0.1),
            legs_within(1.0)
        );
        let misread = legs.iter().filter(|l| l.0 > 0.1 && l.3 < 0.01).count();
        println!(
            "  legs off by >10 cm where ours equals the reading (Survex moved it, or we read it differently): {misread}"
        );
        if std::env::var("MISREAD").is_ok() {
            for (d, l, sx, _) in legs.iter().filter(|l| l.0 > 0.1 && l.3 < 0.01) {
                println!(
                    "    possible misread: {l} off by {d:.2} m (Survex {sx:.2} m from our reading)"
                );
            }
        }
        for (d, l, sx, us) in legs.iter().take(8).filter(|l| l.0 > 0.1) {
            println!(
                "    leg off by {d:.2} m  {l}  (Survex {sx:.2} m from the reading, ours {us:.2} m)"
            );
        }
        let mut missing: Vec<&String> = want.keys().filter(|k| !got.contains_key(*k)).collect();
        missing.sort();
        if !missing.is_empty() {
            println!(
                "  not in ours ({}), e.g. {:?}",
                missing.len(),
                &missing[..missing.len().min(6)]
            );
        }
        for f in survey.findings().iter().take(5) {
            println!(
                "  finding: {}",
                f.message.chars().take(160).collect::<String>()
            );
        }
    }
}
