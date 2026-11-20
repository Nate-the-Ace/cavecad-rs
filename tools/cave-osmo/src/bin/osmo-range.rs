//! Full range test through the library: runs each axis each way until the gimbal stops following, reads where it
//! got to from the gimbal's own attitude report, and brings it back home closed-loop.
//!   cargo run --release --bin osmo-range [dps]
use cave_osmo::{Attitude, Axis, Osmo};
use std::time::{Duration, Instant};

fn line(label: &str, a: &Attitude) -> String {
    let (r, p, y) = a.euler();
    let (az, inc) = a.clamp_axis(1.0);
    format!(
        "{label:<10} joints pan {:7.1} tilt {:6.1} roll {:6.1} | attitude r {:6.1} p {:5.1} y {:6.1} | clamp top az {:6.1} inc {:5.1}",
        a.pan, a.tilt, a.roll, r, p, y, az, inc
    )
}

/// Brings `axis` back to where it was at home, closed-loop on the gimbal's attitude.
async fn come_home(o: &mut Osmo, axis: Axis, came: f64, dps: f64, home: &Attitude) -> Attitude {
    // back the way it came: the short way round can run into a stop
    let a = o.bring_to(axis, home.along(axis), dps, 1.5, 5, -came).await.expect("homing");
    println!("  {}  {:.1}° from home", line("homing", &a), a.angle_to(home));
    a
}

#[tokio::main]
async fn main() {
    // SAFETY: set before any other thread reads the environment
    unsafe { std::env::set_var("OSMO_VERBOSE", "1") };
    let dps: f64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(30.0);
    let mut o = Osmo::find(Duration::from_secs(15)).await.expect("no gimbal found");
    println!("connected to {}", o.name);
    // the first report after connecting can carry zeroed joints: take the second
    let _ = o.fresh_attitude(Instant::now(), Duration::from_secs(5)).await.expect("no attitude report (handshake?)");
    let first = o.fresh_attitude(Instant::now(), Duration::from_secs(3)).await.expect("attitude");
    // start from the pan centre (joint 0), whatever a previous run left
    let p = cave_osmo::om7_pan(first.pan);
    let home = o.bring_to(Axis::Pan, first.along(Axis::Pan) - p, dps, 1.5, 5, -p.signum()).await.expect("centre");
    println!("centred from pan joint {p:.1}");
    println!("{}", line("home", &home));
    // cross+ / cross-: guarded roll to the edge, then creep past it slowly, watching for the flip
    if let Some(c) = std::env::args().nth(2).filter(|a| a.starts_with("cross")) {
        let sign = if c.ends_with('-') { -1.0 } else { 1.0 };
        let edge = match o.guarded_turn(Axis::Roll, sign * 330.0, dps, &cave_osmo::OM7_PORTRAIT).await {
            Ok(a) | Err(cave_osmo::Error::AtLimit(a)) => a,
            Err(e) => panic!("{e}"),
        };
        println!("{}", line("edge", &edge));
        let creep = 5.0;
        let mut last = edge;
        for _ in 0..30 {
            o.turn_rates(0.0, 0.0, sign * creep, Duration::from_millis(1000)).await.expect("turn");
            let now = o.fresh_attitude(Instant::now(), Duration::from_secs(3)).await.expect("attitude");
            let moved = last.angle_to(&now);
            println!("{}  moved {moved:5.1}°", line("creep", &now));
            last = now;
            if moved > 20.0 {
                println!("flipped even at {creep}°/s");
                break;
            }
        }
        o.stop().await.ok();
        o.disconnect().await;
        return;
    }
    let mut results = Vec::new();
    let only = std::env::args().nth(2).unwrap_or_default();
    for (axis, sign, name) in [
        (Axis::Pan, 1.0, "pan +"),
        (Axis::Pan, -1.0, "pan -"),
        (Axis::Tilt, 1.0, "tilt +"),
        (Axis::Tilt, -1.0, "tilt -"),
        (Axis::Roll, -1.0, "roll -"),
        (Axis::Roll, 1.0, "roll +"),
    ] {
        if !only.is_empty() && !name.starts_with(only.as_str()) {
            continue;
        }
        let t0 = Instant::now();
        let before = o.fresh_attitude(Instant::now(), Duration::from_secs(3)).await.expect("attitude");
        // the roll has no stop (it flips over): guarded, it stops at the edge of the safe window instead
        let run = if axis == Axis::Roll {
            match o.guarded_turn(axis, sign * 330.0, dps, &cave_osmo::OM7_PORTRAIT).await {
                Ok(a) | Err(cave_osmo::Error::AtLimit(a)) => Ok((a, a.angle_to(&before))),
                Err(e) => Err(e),
            }
        } else {
            o.run_to_limit(axis, sign * dps, 330.0).await
        };
        let (at, travel) = match run {
            Ok(r) => r,
            Err(cave_osmo::Error::Snapped(a)) => {
                println!("{}  SNAPPED OVER: stopping (put it back in portrait with the rotate button)", line(name, &a));
                break;
            }
            Err(e) => panic!("{e}"),
        };
        let back = come_home(&mut o, axis, sign, dps, &home).await;
        let off = back.angle_to(&home);
        println!("{}  travel {travel:5.1}°  {:4.1} s  back home within {off:.1}°", line(name, &at), t0.elapsed().as_secs_f64());
        results.push((name, at, travel, off));
        if off > 10.0 {
            println!("did not get home ({off:.1}° off); stopping here");
            break;
        }
    }
    println!("\nsummary (joint at the limit, gimbal-reported travel from home):");
    for (name, a, travel, off) in &results {
        let j = match *name {
            n if n.starts_with("pan") => a.pan,
            n if n.starts_with("tilt") => a.tilt,
            _ => a.roll,
        };
        println!("  {name:<7} joint {j:7.1}°  travel {travel:5.1}°  return error {off:.1}°");
    }
    o.disconnect().await;
}
