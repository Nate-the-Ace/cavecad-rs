//! Prints a sweep plan as Swift-probe steps, so a plan from the library can be run on the bench:
//!   tools/cave-osmo/run-swift.sh steps $(cargo run -q --bin osmo-plan -- ring 30 1500)
//! `ring <step°> <shot pause ms> [flip] [x1]`: a pause stands in for the X1's shot, or with `x1` a real `shoot` step.
//! `flip` rolls the other way: the OM7's long roll side depends on how the clamp got into portrait (FINDINGS.md).
use cave_osmo::sweep;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let step: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30.0);
    let shot_ms: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1500);
    let flip = args.iter().any(|a| a == "flip");
    let x1 = args.iter().any(|a| a == "x1");
    let mut plan = sweep::ring(sweep::Mount::LaserUp, step);
    if flip {
        plan.iter_mut().for_each(|m| m.roll = -m.roll);
    }
    let mut out = Vec::new();
    for m in &plan {
        let secs = m.seconds();
        if secs > 0.0 {
            let t = |d: f64| ((d / secs) * 10.0).round() as i64;
            out.push(format!("speed:{}:{}:{}:{}", t(m.yaw), t(m.pitch), (secs * 1000.0).round() as i64, t(m.roll)));
        }
        if m.shoot && x1 {
            out.push(format!("wait:{}", (sweep::SETTLE_S * 1000.0) as u64));
            out.push("shoot".into());
        } else {
            out.push(format!("wait:{}", (sweep::SETTLE_S * 1000.0) as u64 + if m.shoot { shot_ms } else { 0 }));
        }
    }
    println!("{}", out.join(" "));
}
