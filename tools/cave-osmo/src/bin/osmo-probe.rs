//! Finds out what an Osmo Mobile accepts. Every run connects, does one thing, and prints everything the gimbal said.
//!
//!   osmo-probe scan                      what's advertising nearby
//!   osmo-probe listen [secs]             connect and log what the gimbal sends by itself
//!   osmo-probe get                       ask for its position (0x04/0x02)
//!   osmo-probe speed <yaw> <pitch> <ms>  turn at a rate (tenths of °/s) for <ms>, then stop
//!   osmo-probe abs <yaw> <pitch> [t]     go to an absolute angle (tenths of °) over t tenths of a second
//!   osmo-probe rel <yaw> <pitch> [t]     turn by an angle
//!   osmo-probe mode lock|follow|fpv
//!   osmo-probe raw <set> <cmd> [hex]     any gimbal-set message (set must be 04)
//!   osmo-probe steps get speed:100:0:1000 abs:100:0:20 rel:-100:0:20 …
//!                                        several of the above over one connection (macOS grabs the gimbal as a
//!                                        keyboard after every disconnect, so one link per run is precious)
use cave_osmo::*;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let what = args.first().map(String::as_str).unwrap_or("listen");
    let num = |i: usize, d: i64| args.get(i).and_then(|s| s.parse::<i64>().ok()).unwrap_or(d);

    if what == "scan" {
        match scan(Duration::from_secs(6)).await {
            Ok(list) => {
                for (name, rssi, fff0) in list {
                    println!("{:<32} rssi {:>5} {}", if name.is_empty() { "(no name)" } else { &name }, rssi.map(|r| r.to_string()).unwrap_or("-".into()), if fff0 { "fff0" } else { "" });
                }
            }
            Err(e) => println!("scan failed: {e}"),
        }
        return;
    }

    let mut osmo = match Osmo::find(Duration::from_secs(12)).await {
        Ok(o) => o,
        Err(e) => {
            println!("connect failed: {e}");
            return;
        }
    };
    let t0 = Instant::now();
    println!("connected: {}", osmo.name);
    for c in &osmo.characteristics {
        println!("  {c}");
    }
    // what it says before we ask anything
    let before = osmo.drain(Duration::from_millis(1500), Duration::from_secs(2)).await;
    report("on connect", &before, t0);

    if what == "steps" {
        for step in &args[1..] {
            let p: Vec<&str> = step.split(':').collect();
            let n = |i: usize, d: i64| p.get(i).and_then(|s| s.parse::<i64>().ok()).unwrap_or(d);
            let mut sent: Vec<(Instant, Vec<u8>)> = Vec::new();
            let r = match p[0] {
                "get" => osmo.send(cmd::SET_GIMBAL, cmd::GET_POSITION, vec![], 0x40).await.map(|b| sent.push((Instant::now(), b))),
                "speed" => {
                    let end = Instant::now() + Duration::from_millis(n(3, 500) as u64);
                    let mut r = Ok(());
                    while Instant::now() < end && r.is_ok() {
                        r = osmo.rotate(n(1, 0) as i16, n(2, 0) as i16, Rotate::Speed, 2).await.map(|b| sent.push((Instant::now(), b)));
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    r.and(osmo.stop().await.map(|b| sent.push((Instant::now(), b))))
                }
                "abs" | "rel" => {
                    let mode = if p[0] == "abs" { Rotate::Absolute } else { Rotate::Relative };
                    osmo.rotate(n(1, 0) as i16, n(2, 0) as i16, mode, n(3, 20) as u8).await.map(|b| sent.push((Instant::now(), b)))
                }
                "wait" => {
                    tokio::time::sleep(Duration::from_millis(n(1, 1000) as u64)).await;
                    Ok(())
                }
                _ => Ok(()),
            };
            println!("=== step {step}");
            if let Err(e) = r {
                println!("send failed: {e}");
            }
            for (at, b) in &sent {
                let hex: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
                println!("sent +{:.2}s {}", at.duration_since(t0).as_secs_f64(), hex.join(" "));
            }
            let after = osmo.drain(Duration::from_millis(2500), Duration::from_secs(4)).await;
            report("after", &after, t0);
        }
        osmo.disconnect().await;
        return;
    }

    let mut sent: Vec<(Instant, Vec<u8>)> = Vec::new();
    let result = match what {
        "listen" => {
            let h = osmo.drain(Duration::from_secs(5), Duration::from_secs(num(1, 10) as u64)).await;
            report("listening", &h, t0);
            Ok(())
        }
        "get" => {
            let r = osmo.send(cmd::SET_GIMBAL, cmd::GET_POSITION, vec![], 0x40).await;
            r.map(|b| sent.push((Instant::now(), b)))
        }
        "speed" => {
            let (yaw, pitch, ms) = (num(1, 0) as i16, num(2, 0) as i16, num(3, 500) as u64);
            let end = Instant::now() + Duration::from_millis(ms);
            let mut r = Ok(());
            while Instant::now() < end && r.is_ok() {
                r = osmo.rotate(yaw, pitch, Rotate::Speed, 2).await.map(|b| sent.push((Instant::now(), b)));
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            r.and(osmo.stop().await.map(|b| sent.push((Instant::now(), b))))
        }
        "abs" | "rel" => {
            let mode = if what == "abs" { Rotate::Absolute } else { Rotate::Relative };
            osmo.rotate(num(1, 0) as i16, num(2, 0) as i16, mode, num(3, 20) as u8).await.map(|b| sent.push((Instant::now(), b)))
        }
        "mode" => {
            let m = match args.get(1).map(String::as_str) {
                Some("follow") => 1,
                Some("fpv") => 2,
                _ => 0,
            };
            osmo.send(cmd::SET_GIMBAL, cmd::SET_MODE, vec![m, 0], 0x00).await.map(|b| sent.push((Instant::now(), b)))
        }
        "raw" => {
            let set = u8::from_str_radix(args.get(1).map(String::as_str).unwrap_or("04"), 16).unwrap_or(0);
            let c = u8::from_str_radix(args.get(2).map(String::as_str).unwrap_or("00"), 16).unwrap_or(0);
            let hex = args.get(3).cloned().unwrap_or_default();
            let payload: Vec<u8> = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()).collect();
            if set != cmd::SET_GIMBAL {
                println!("refused: only gimbal-set (04) messages");
                Ok(())
            } else {
                osmo.send(set, c, payload, 0x40).await.map(|b| sent.push((Instant::now(), b)))
            }
        }
        other => {
            println!("unknown: {other}");
            Ok(())
        }
    };
    if let Err(e) = result {
        println!("send failed: {e}");
    }
    for (at, b) in &sent {
        let hex: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
        println!("sent +{:.2}s {}", at.duration_since(t0).as_secs_f64(), hex.join(" "));
    }
    if what != "listen" {
        let after = osmo.drain(Duration::from_millis(2500), Duration::from_secs(6)).await;
        report("after", &after, t0);
    }
    osmo.disconnect().await;
}

/// Counts by set/cmd, every non-repeating message once, and angle readings where a payload looks like one.
fn report(title: &str, heard: &[Heard], t0: Instant) {
    println!("--- {title}: {} message(s)", heard.len());
    let mut by: BTreeMap<(u8, u8, u8, u8), (usize, Vec<&Heard>)> = BTreeMap::new();
    for h in heard {
        let m = &h.message;
        let e = by.entry((m.sender, m.receiver, m.set, m.cmd)).or_default();
        e.0 += 1;
        if e.1.len() < 3 || e.1.last().map(|l| l.message.payload != m.payload).unwrap_or(true) && e.1.len() < 6 {
            e.1.push(h);
        }
    }
    for ((s, r, set, c), (n, samples)) in by {
        println!("  {s:02x}→{r:02x} {set:02x}/{c:02x} ×{n}");
        for h in samples {
            let ch = h.on.to_string();
            let mut line = format!("    +{:.2}s {} {}", h.at.duration_since(t0).as_secs_f64(), &ch[4..8], h.message.summary());
            if set == cmd::SET_GIMBAL && c == cmd::PUSH_POSITION {
                if let Some(a) = decode_angles(&h.message.payload) {
                    line += &format!("  → pitch {:.1} roll {:.1} yaw {:.1}", a.pitch, a.roll, a.yaw);
                }
            }
            println!("{line}");
        }
    }
}
