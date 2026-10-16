//! Real Therion projects with their sketch pages, read from this machine only (they belong to the
//! clubs that drew them and are never committed). `REAL_TH2` lists the projects' top files,
//! `;`-separated:
//! `REAL_TH2=a/cave.th cargo test -p cave-th2 --test real -- --ignored --nocapture`

use cave_doc::ActorId;
use cave_id::SeqIds;

#[test]
#[ignore]
fn real_sketches_land_on_their_surveys() {
    let list = std::env::var("REAL_TH2").unwrap_or_default();
    for top in list.split(';').filter(|s| !s.is_empty()) {
        let read = |p: &str| std::fs::read_to_string(p).ok();
        let text = std::fs::read_to_string(top).unwrap();
        let (_, survey) = cave_formats::read_any(top, &text, &read).unwrap();
        let mut doc = cave_formats::to_document(&survey, ActorId(1)).unwrap();
        println!(
            "== {top}: {} shots, {} sketch pages",
            survey.shots.len(),
            survey.sketches.len()
        );
        let mut ids = SeqIds::new(7);
        let aliases: Vec<(String, String)> = survey
            .aliases
            .iter()
            .map(|(a, k)| (a.to_string(), k.clone()))
            .collect();
        for page in &survey.sketches {
            let Some(t) = read(&page.path) else {
                println!("  {} missing", page.path);
                continue;
            };
            let sketch = cave_formats::th2::parse(&t);
            let (actions, report) =
                cave_th2::import(&doc, &mut ids, &sketch, &page.survey, &aliases, true);
            doc.edit(actions).unwrap();
            for m in report.messages(page.path.rsplit('/').next().unwrap()) {
                println!("  {m}");
            }
            if let Some((n, r)) = &report.worst {
                println!("  worst before bending: {n} {r:.2} m");
            }
        }
        if let Ok(dir) = std::env::var("SVG_DIR") {
            let name = top.rsplit('/').next().unwrap();
            std::fs::write(format!("{dir}/{name}.svg"), svg(&doc)).unwrap();
        }
    }
}

/// The map as a picture: legs grey, walls black, other lines blue, symbols red, words green.
fn svg(doc: &cave_doc::Document) -> String {
    let scene = cave_scene::build(doc, true);
    let lines = cave_scene::map_lines_3d(doc, true);
    let all: Vec<[f64; 2]> = scene.legs.iter().flat_map(|l| [[l.from[0], l.from[1]], [l.to[0], l.to[1]]]).collect();
    let (x0, x1) = all.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
    let (y0, y1) = all.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[1]), a.1.max(p[1])));
    let k = 4000.0 / (x1 - x0).max(y1 - y0);
    let at = |x: f64, y: f64| format!("{:.1},{:.1}", (x - x0) * k + 20.0, (y1 - y) * k + 20.0);
    let mut out = format!("<svg xmlns='http://www.w3.org/2000/svg' width='{:.0}' height='{:.0}'><rect width='100%' height='100%' fill='white'/>", (x1 - x0) * k + 40.0, (y1 - y0) * k + 40.0);
    for l in &scene.legs {
        out += &format!("<polyline points='{} {}' stroke='#bbb' stroke-width='2' fill='none'/>", at(l.from[0], l.from[1]), at(l.to[0], l.to[1]));
    }
    for l in &lines {
        let pts: Vec<String> = l.points.iter().map(|p| at(p[0], p[1])).collect();
        if l.kind.starts_with("sym-") || l.kind == "text" {
            let c = if l.kind == "text" { "green" } else { "red" };
            out += &format!("<circle cx='{}' cy='{}' r='3' fill='{c}'/>", pts[0].split(',').next().unwrap(), pts[0].split(',').nth(1).unwrap());
            continue;
        }
        let (c, w) = match l.kind.as_str() {
            "wall" => ("black", 2.5),
            k if k.starts_with("fill-") => ("#08f", 1.0),
            "sketch" => ("#aac", 1.0),
            _ => ("#36c", 1.5),
        };
        let tag = if l.closed { "polygon" } else { "polyline" };
        out += &format!("<{tag} points='{}' stroke='{c}' stroke-width='{w}' fill='none'/>", pts.join(" "));
    }
    out + "</svg>"
}
