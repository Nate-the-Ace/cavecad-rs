//! Damaged files must never crash an import. Each seed file is cut, shuffled and poisoned many ways
//! and fed to every reader, the document bridge, every writer, and the sketch import:
//! `FUZZ_SEEDS=<dir> FUZZ_N=200 cargo test --release -p cave-th2 --test fuzz -- --ignored --nocapture`
//! (seeds are read from the folder, recursively; nothing is written).

use cave_doc::ActorId;
use cave_formats::{FORMATS, Survey};
use cave_id::SeqIds;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

/// A small fixed random source, so a failure can be replayed by seed.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

const POISON: &[&str] = &[
    "NaN",
    "inf",
    "-inf",
    "1e400",
    "-0",
    "0",
    "",
    "-",
    "--",
    "999999999999",
    "-999",
    "-999.00",
    "1e-320",
    "0x10",
    "1,5",
    "1.2.3",
    "°",
    "é",
    "🦇",
    "\u{0}",
    "\t",
    "\\",
    "\"",
    "\"unclosed",
    "[",
    "]",
    "[1 2",
    "#",
    ";",
    "*",
    "*begin",
    "*end",
    "endsurvey",
    "endcentreline",
    "endscrap",
    "endline",
    "endarea",
    "input",
    "*include",
    "@",
    "1@",
    "@x",
    ".",
    "..",
    "#units",
    "!",
    "%",
    "smooth",
    "-close",
    "-id",
    "-subtype",
    "-scale",
    "-orientation",
    "station",
    "-name",
];

fn mutate(text: &str, r: &mut Rng) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    for _ in 0..1 + r.below(6) {
        if lines.is_empty() {
            lines.push(String::new());
        }
        let i = r.below(lines.len());
        match r.below(9) {
            0 => {
                lines.remove(i);
            }
            1 => {
                let l = lines[i].clone();
                lines.insert(r.below(lines.len()), l);
            }
            2 => {
                let j = r.below(lines.len());
                lines.swap(i, j);
            }
            3..=5 => {
                // a word replaced by poison
                let mut w: Vec<String> = lines[i].split(' ').map(str::to_string).collect();
                let k = r.below(w.len());
                w[k] = POISON[r.below(POISON.len())].to_string();
                lines[i] = w.join(" ");
            }
            6 => {
                // cut the line at a character boundary
                let cs: Vec<char> = lines[i].chars().collect();
                lines[i] = cs[..r.below(cs.len() + 1)].iter().collect();
            }
            7 => lines.truncate(i),
            _ => lines.insert(i, POISON[r.below(POISON.len())].to_string()),
        }
    }
    let mut out = lines.join(if r.below(4) == 0 { "\r\n" } else { "\n" });
    if r.below(5) == 0 {
        out.push('\u{0C}');
    }
    out
}

fn seeds(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            seeds(&p, out);
            continue;
        }
        let name = p.file_name().unwrap().to_string_lossy().to_lowercase();
        let wanted = [
            ".svx", ".th", ".th2", ".dat", ".srv", ".mak", ".wpj", ".csv", "thconfig",
        ];
        if wanted.iter().any(|w| name.ends_with(w))
            && let Ok(t) = std::fs::read_to_string(&p)
            && t.len() < 400_000
        {
            out.push((p.to_string_lossy().into_owned(), t));
        }
    }
}

fn exercise(name: &str, text: &str) {
    let base = name.rsplit('/').next().unwrap();
    // the whole pipeline the app runs, through read_any (no other files)
    if let Ok((_, s)) = cave_formats::read_any(base, text, &|_| None) {
        round_trip(&s);
    }
    for f in FORMATS {
        let s = f.parse(text);
        round_trip(&s);
    }
    let sketch = cave_formats::th2::parse(text);
    let survey = cave_formats::therion::parse(
        "survey s\ncentreline\ndata normal from to length compass clino\n1 2 10 0 0\n2 3 10 90 0\n3 4 5 180 0\nendcentreline\nendsurvey\n",
    );
    let doc = cave_formats::to_document(&survey, ActorId(1)).unwrap();
    let mut doc2 = doc.clone();
    let (a, r) = cave_th2::import(&doc, &mut SeqIds::new(9), &sketch, "", &[], true);
    doc2.edit(a).expect("sketch actions apply");
    let _ = r.messages(base);
}

fn round_trip(s: &Survey) {
    if let Ok(doc) = cave_formats::to_document(s, ActorId(1)) {
        let back = cave_formats::from_document(&doc);
        for f in FORMATS {
            let _ = f.write(&mut back.clone());
        }
        let _ = cave_scene_build(&doc);
    }
    for f in FORMATS {
        let _ = f.write(&mut s.clone());
    }
}

fn cave_scene_build(doc: &cave_doc::Document) -> usize {
    cave_scene::build(doc, true).stations.len()
}

static WHERE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

#[test]
#[ignore]
fn damaged_files_never_crash_an_import() {
    let dir = std::env::var("FUZZ_SEEDS").expect("FUZZ_SEEDS");
    let n: usize = std::env::var("FUZZ_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let mut files = Vec::new();
    seeds(Path::new(&dir), &mut files);
    files.sort();
    std::panic::set_hook(Box::new(|info| {
        *WHERE.lock().unwrap() = info.location().map(|l| l.to_string()).unwrap_or_default();
    }));
    let mut failures = Vec::new();
    for (fi, (name, text)) in files.iter().enumerate() {
        for k in 0..=n {
            let seed = (fi as u64 + 1) * 1_000_003 + k as u64;
            let mut r = Rng(seed | 1);
            let m = if k == 0 {
                text.clone()
            } else {
                mutate(text, &mut r)
            };
            let (tx, rx) = std::sync::mpsc::channel();
            let (nm, mm) = (name.clone(), m.clone());
            std::thread::spawn(move || {
                let got = catch_unwind(AssertUnwindSafe(|| exercise(&nm, &mm)));
                let _ = tx.send(got.err().map(|e| {
                    let msg = e
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_default();
                    format!("{msg} at {}", WHERE.lock().unwrap())
                }));
            });
            let problem = match rx.recv_timeout(std::time::Duration::from_secs(20)) {
                Ok(None) => None,
                Ok(Some(m)) => Some(m),
                Err(_) => Some("took over 20 s (hang?)".to_string()),
            };
            if let Some(msg) = problem {
                let path = format!("{}/fuzz-{fi}-{k}.txt", std::env::temp_dir().display());
                std::fs::write(&path, &m).unwrap();
                println!("{name} #{k}: {msg} ({path})");
                failures.push(format!("{name} #{k}: {msg} ({path})"));
                break;
            }
        }
    }
    let _ = std::panic::take_hook();
    for f in &failures {
        println!("{f}");
    }
    println!("{} files, {} failed", files.len(), failures.len());
    // a hung case's thread is still running: end here rather than wait for it
    std::process::exit(i32::from(!failures.is_empty()));
}
