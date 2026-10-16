//! The readers against Survex's own test suite: every Compass, Walls (and, with
//! `SURVEX_SUITE_SVX=1`, Survex) test file that Survex records station positions for is read here,
//! turned into a cave, placed by our survey math, and compared station by station.
//!
//! Survex is GPL and its test files are not copied into this repository; point `SURVEX_TESTS` at a
//! checkout's `tests` folder (`git clone https://github.com/ojwb/survex`) and run
//! `cargo test -p cave-formats --test survex_suite -- --ignored --nocapture`.
//! It reports rather than fails: some of those files test features no reader here claims.

use cave_doc::{ActorId, Field};
use cave_formats::{detect, detect_project, read_project, to_document};
use cave_survey::{Params, adjust, extract, resolve};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Station positions Survex wrote: a `.pos` (`( x, y, z ) name`) or a `.dump` (`NODE x y z [name]`).
fn expected(stem: &Path) -> Option<HashMap<String, [f64; 3]>> {
    let mut out = HashMap::new();
    if let Ok(t) = std::fs::read_to_string(stem.with_extension("pos")) {
        for l in t.lines().skip(1) {
            let (Some(a), Some(b)) = (l.find('('), l.find(')')) else {
                continue;
            };
            let v: Vec<f64> = l[a + 1..b]
                .split(',')
                .filter_map(|x| x.trim().parse().ok())
                .collect();
            if v.len() == 3 {
                out.insert(l[b + 1..].trim().to_string(), [v[0], v[1], v[2]]);
            }
        }
        return Some(out);
    }
    let t = std::fs::read_to_string(stem.with_extension("dump")).ok()?;
    for l in t.lines().filter(|l| l.starts_with("NODE ")) {
        let (Some(a), Some(b)) = (l.find('['), l.rfind(']')) else {
            continue;
        };
        let v: Vec<f64> = l[5..a]
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        if v.len() == 3 {
            out.insert(l[a + 1..b].to_string(), [v[0], v[1], v[2]]);
        }
    }
    Some(out)
}

/// Reads a file the way Windows would: exact name first, then ignoring case.
fn reader(dir: PathBuf) -> impl Fn(&str) -> Option<String> {
    move |p: &str| {
        let path = if Path::new(p).is_absolute() {
            PathBuf::from(p)
        } else {
            dir.join(p)
        };
        if let Ok(t) = std::fs::read(&path) {
            return Some(String::from_utf8_lossy(&t).into_owned());
        }
        let parent = path.parent()?;
        let want = path.file_name()?.to_string_lossy().to_lowercase();
        for e in std::fs::read_dir(parent).ok()?.flatten() {
            if e.file_name().to_string_lossy().to_lowercase() == want {
                return std::fs::read(e.path())
                    .ok()
                    .map(|t| String::from_utf8_lossy(&t).into_owned());
            }
        }
        None
    }
}

/// Our positions, by station name, in metres (adjusted, as Survex's are).
fn ours(survey: &cave_formats::Survey) -> Result<HashMap<String, [f64; 3]>, String> {
    let doc = to_document(survey, ActorId(1))?;
    let ex = extract(&doc);
    let r = resolve(&ex.legs, &ex.fixed);
    let a = adjust(&ex.legs, &r, &Params::default());
    // a station only ever fixed (no shot reaches it) is placed by its fix
    let mut placed = a.stations.clone();
    for f in &ex.fixed {
        placed.entry(f.station).or_insert(f.pos);
    }
    Ok(placed
        .iter()
        .filter_map(|(id, p)| {
            let name = doc.entity(*id)?.text(Field::StationLabel);
            Some((name, [p.x, p.y, p.z]))
        })
        .collect())
}

#[test]
#[ignore = "needs a Survex checkout: set SURVEX_TESTS"]
fn survex_test_suite() {
    let Ok(dir) = std::env::var("SURVEX_TESTS") else {
        eprintln!("SURVEX_TESTS not set");
        return;
    };
    let dir = PathBuf::from(dir);
    let svx = std::env::var("SURVEX_SUITE_SVX").is_ok();
    let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    names.sort();
    let (mut good, mut total) = (0, 0);
    for path in names {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let wanted = matches!(ext.as_str(), "mak" | "wpj" | "dat" | "srv") || (svx && ext == "svx");
        if !wanted {
            continue;
        }
        let Some(want) = expected(&path.with_extension("")) else {
            continue;
        };
        if want.is_empty() {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
        let survey = match detect_project(&name, &text) {
            Some(f) => read_project(f, &name, &text, &reader(dir.clone())).survey,
            None => match detect(&name, &text) {
                Some(f) => f.parse(&text),
                None => {
                    println!("{name:<28} not recognised");
                    continue;
                }
            },
        };
        total += 1;
        let got = match ours(&survey) {
            Ok(g) => g,
            Err(e) => {
                println!("{name:<28} no cave: {e}");
                continue;
            }
        };
        // Survex joins prefixes with its separator; compare names with it removed
        // and writes an empty Walls level as " " and an empty name as "empty name"
        let key = |n: &str| {
            n.replace("empty name", "")
                .replace(' ', "")
                .replace([':', '.'], "/")
                .to_lowercase()
        };
        let want: HashMap<String, [f64; 3]> = want.into_iter().map(|(k, v)| (key(&k), v)).collect();
        let got: HashMap<String, [f64; 3]> = got.into_iter().map(|(k, v)| (key(&k), v)).collect();
        let common: Vec<&String> = {
            let mut c: Vec<&String> = want.keys().filter(|k| got.contains_key(*k)).collect();
            c.sort();
            c
        };
        if common.is_empty() {
            println!(
                "{name:<28} no stations in common ({} expected, {} read)",
                want.len(),
                got.len()
            );
            continue;
        }
        // Absolute when the survey has fixed points; else both are placed from one station.
        let shift = if survey.fixed.is_empty() {
            let a = want[common[0]];
            let b = got[common[0]];
            [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
        } else {
            [0.0; 3]
        };
        let mut worst = (0.0f64, String::new());
        for k in &common {
            let (a, b) = (want[*k], got[*k]);
            let d = ((a[0] - b[0] - shift[0]).powi(2)
                + (a[1] - b[1] - shift[1]).powi(2)
                + (a[2] - b[2] - shift[2]).powi(2))
            .sqrt();
            if d > worst.0 {
                worst = (d, k.to_string());
            }
        }
        let ok = worst.0 < 0.05;
        good += usize::from(ok);
        println!(
            "{name:<28} {} {}/{} stations, worst {:.3} m{}{}",
            if ok { "ok  " } else { "DIFF" },
            common.len(),
            want.len(),
            worst.0,
            if worst.1.is_empty() {
                String::new()
            } else {
                format!(" at {}", worst.1)
            },
            if survey.findings().is_empty() {
                String::new()
            } else {
                format!("  [{} findings]", survey.findings().len())
            }
        );
    }
    println!("{good} of {total} files place every common station within 5 cm of Survex");
}
