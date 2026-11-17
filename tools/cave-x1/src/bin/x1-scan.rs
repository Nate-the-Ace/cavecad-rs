//! Exercises CaveCAD firmware (-cc2) scan control over Bluetooth: status, laser on, start, listen, stop.
//!   tools/cave-x1/run-mac.sh x1-scan [seconds]

use cave_x1::{X1, command};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(8);
    let mut x1 = match X1::find(Duration::from_secs(30)).await {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    println!("Connected to {}.", x1.name());
    x1.drain(Duration::from_millis(1500)).await;
    let t0 = Instant::now();
    let show = |what: &str, r: &Result<cave_instrument::cavway::ScanStatus, cave_x1::Error>| match r {
        Ok(s) => println!(
            "{:6.2}s {what}: result {} mode {} laser {} count {} {:.1}s options 0x{:02X} last end {:?} cc{}",
            t0.elapsed().as_secs_f64(),
            s.result,
            s.mode,
            s.laser,
            s.count,
            s.seconds(),
            s.options,
            s.last_end,
            s.cc_version
        ),
        Err(e) => println!("{:6.2}s {what}: {e}", t0.elapsed().as_secs_f64()),
    };
    let r = x1.scan_command(command::SCAN_STATUS, Duration::from_secs(3)).await;
    show("status", &r);
    let r = x1.scan_command(command::SCAN_START, Duration::from_secs(3)).await;
    show("start with laser off (expect result 1)", &r);
    let _ = x1.laser_on().await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let r = x1.scan_command(command::SCAN_START, Duration::from_secs(3)).await;
    show("start", &r);
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if let Some(s) = x1.next_shot(end.saturating_duration_since(Instant::now())).await {
            println!(
                "{:6.2}s shot: {:.3} m az {:.1} inc {:.1} scan {} first {} count {:?}",
                t0.elapsed().as_secs_f64(),
                s.packet.distance,
                s.packet.azimuth,
                s.packet.inclination,
                s.packet.scan,
                s.packet.sweep_start,
                s.packet.scan_count
            );
        }
    }
    let r = x1.scan_command(command::SCAN_STATUS, Duration::from_secs(3)).await;
    show("status", &r);
    let r = x1.scan_command(command::SCAN_STOP, Duration::from_secs(3)).await;
    show("stop", &r);
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let r = x1.scan_command(command::SCAN_STATUS, Duration::from_secs(3)).await;
    show("status", &r);
    let _ = x1.laser_off().await;
    match x1.set_clock(Duration::from_secs(3)).await {
        Ok(v) => println!("clock set, X1 reads back {v} (local seconds)"),
        Err(e) => println!("clock: {e}"),
    }
    x1.disconnect().await;
}
