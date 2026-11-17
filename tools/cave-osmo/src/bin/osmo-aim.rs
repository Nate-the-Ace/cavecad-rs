//! Points the clamp top at a list of directions through the library's `aim` (inside the safe joint windows, going round
//! the other way when the short way would flip the roll) and reports how close each one landed, by the gimbal's own
//! attitude report.
//!   tools/cave-osmo/run-mac.sh osmo-aim [dps] [az:inc …]
use cave_osmo::aim::{pointing, Direction};
use cave_osmo::{wrap180, Axis, Osmo, OM7_PORTRAIT};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let dps: f64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(30.0);
    let mut targets: Vec<(f64, f64)> = std::env::args()
        .skip(2)
        .filter_map(|a| a.split_once(':').and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?))))
        .collect();
    if targets.is_empty() {
        targets = vec![(90.0, 0.0), (0.0, 45.0), (-90.0, 0.0), (-90.0, -15.0), (0.0, 90.0), (-150.0, 10.0), (90.0, -15.0), (45.0, 20.0), (0.0, 90.0)];
    }
    let mut o = Osmo::find(Duration::from_secs(15)).await.expect("no gimbal found");
    println!("connected to {}", o.name);
    let _ = o.fresh_attitude(Instant::now(), Duration::from_secs(5)).await.expect("no attitude report");
    let first = o.fresh_attitude(Instant::now(), Duration::from_secs(3)).await.expect("attitude");
    // home: pan centred, clamp upright
    let p = cave_osmo::om7_pan(first.pan);
    let _ = o.bring_to(Axis::Pan, first.along(Axis::Pan) - p, dps, 1.5, 5, -p.signum()).await.expect("centre pan");
    let now = o.fresh_attitude(Instant::now(), Duration::from_secs(3)).await.expect("attitude");
    let up = wrap180(-90.0 - now.along(Axis::Roll));
    if up.abs() > 2.0 {
        let _ = o.guarded_turn(Axis::Roll, up, dps, &OM7_PORTRAIT).await;
    }
    let home = o.fresh_attitude(Instant::now(), Duration::from_secs(3)).await.expect("attitude");
    println!("home: joints pan {:.1} tilt {:.1} roll {:.1}", home.pan, home.tilt, home.roll);
    println!("{:>14} {:>16} {:>9} {:>6}  joints (pan tilt roll)", "asked az/inc", "got az/inc", "off", "secs");
    for (az, inc) in targets {
        let t0 = Instant::now();
        let d = Direction { az, inc };
        match o.aim(d, &home, &OM7_PORTRAIT, dps, 1.0).await {
            Ok(a) => {
                let g = pointing(&a, &home);
                let off = if inc > 89.0 { (g.inc - inc).abs() } else { (wrap180(g.az - az).powi(2) + (g.inc - inc).powi(2)).sqrt() };
                let j = a.joints();
                println!(
                    "{:>7.0}/{:<6.0} {:>8.1}/{:<7.1} {:>8.1}° {:>6.1}  {:.0} {:.0} {:.0}",
                    az, inc, g.az, g.inc, off, t0.elapsed().as_secs_f64(), j[0], j[1], j[2]
                );
            }
            Err(cave_osmo::Error::NotFound) => println!("{az:>7.0}/{inc:<6.0} out of reach (would pass a stop or flip the roll): skipped"),
            Err(e) => {
                println!("{az:>7.0}/{inc:<6.0} {e}; stopping");
                break;
            }
        }
    }
    o.disconnect().await;
}
