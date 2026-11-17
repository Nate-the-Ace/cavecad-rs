//! Looks for a command that starts the X1's scan mode from outside. Sends each candidate single-byte command once, with the
//! laser off, and listens for 5 s: scan mode streams shots with the scan flag. Never sends the commands known to power off,
//! calibrate, change silent mode or touch memory or firmware (0x30 to 0x34, 0x3A to 0x3E).
//!   tools/cave-x1/run-mac.sh x1-probe 35 39 3f

use cave_x1::X1;
use std::time::{Duration, Instant};
use tokio::time::sleep;

const NEVER: std::ops::RangeInclusive<u8> = 0x30..=0x34;
const NEVER2: std::ops::RangeInclusive<u8> = 0x3a..=0x3e;

#[tokio::main]
async fn main() {
    let cmds: Vec<u8> = std::env::args()
        .skip(1)
        .filter_map(|a| u8::from_str_radix(a.trim_start_matches("0x"), 16).ok())
        .filter(|c| !NEVER.contains(c) && !NEVER2.contains(c) && !cave_x1::command::refused(*c))
        .collect();
    let mut x1 = match X1::find(Duration::from_secs(30)).await {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    println!("Connected to {}.", x1.name());
    x1.drain(Duration::from_millis(1500)).await;
    for c in cmds {
        let _ = x1.laser_off().await;
        x1.drain(Duration::from_millis(1500)).await;
        let t = Instant::now();
        let _ = x1.send(c).await;
        let mut got = Vec::new();
        while t.elapsed() < Duration::from_secs(5) {
            if let Some(s) = x1.next_shot(Duration::from_secs(5).saturating_sub(t.elapsed())).await {
                got.push(s);
            }
        }
        let scans = got.iter().filter(|s| s.packet.scan).count();
        println!(
            "0x{c:02x}: {} shot(s) in 5 s, {} scan shot(s){}",
            got.len(),
            scans,
            got.first()
                .map(|s| format!(", first after {} ms, {:.2} m", s.at.duration_since(t).as_millis(), s.packet.distance))
                .unwrap_or_default()
        );
        if scans > 0 {
            println!("SCAN MODE STARTED by 0x{c:02x}: stopping here (press CLR on the X1).");
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }
    let _ = x1.laser_off().await;
    x1.disconnect().await;
}
