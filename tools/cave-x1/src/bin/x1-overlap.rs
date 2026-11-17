//! Does a laser-on sent while a shot is still measuring carry over to the next shot? If it does, a leg's later shots
//! skip most of the X1's ~1.45 s warm-up. Same setup as x1-timing: X1 still, pointed at a wall, not on a phone.

use cave_x1::X1;
use std::time::{Duration, Instant};
use tokio::time::sleep;

const WAIT: Duration = Duration::from_secs(4);

#[tokio::main]
async fn main() {
    let mut x1 = match X1::find(Duration::from_secs(30)).await {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    println!("Connected to {}.", x1.name());
    x1.drain(Duration::from_millis(1500)).await;
    for early in [50u64, 150, 300] {
        let mut out = Vec::new();
        for _ in 0..4 {
            let _ = x1.laser_off().await;
            x1.drain(Duration::from_millis(1200)).await;
            let _ = x1.laser_on().await;
            sleep(Duration::from_millis(1500)).await;
            let _ = x1.send(cave_x1::command::TRIGGER).await;
            sleep(Duration::from_millis(early)).await;
            let _ = x1.laser_on().await; // while the first shot measures
            let first = x1.next_shot(WAIT).await;
            let t = Instant::now();
            let second = if first.is_some() { x1.shoot(WAIT).await.ok() } else { None };
            out.push(match (first, second) {
                (Some(_), Some(s)) => format!("{}", s.at.duration_since(t).as_millis()),
                (Some(_), None) => "x".into(),
                _ => "first-x".into(),
            });
        }
        println!("laser on {early} ms into a shot, then trigger on its reading: second shot ms [{}]", out.join(" "));
    }
    // and laser on twice in a row right after a reading (in case the first is swallowed while the X1 is busy)
    let mut out = Vec::new();
    for _ in 0..4 {
        let _ = x1.laser_off().await;
        x1.drain(Duration::from_millis(1200)).await;
        let _ = x1.laser_on().await;
        sleep(Duration::from_millis(1500)).await;
        if x1.shoot(WAIT).await.is_err() {
            out.push("first-x".to_string());
            continue;
        }
        let t = x1.laser_on().await.unwrap_or_else(|_| Instant::now());
        let _ = x1.send(cave_x1::command::TRIGGER).await;
        out.push(match x1.next_shot(WAIT).await {
            Some(s) => s.at.duration_since(t).as_millis().to_string(),
            None => "x".into(),
        });
    }
    println!("laser on + trigger together after a reading: ms [{}]", out.join(" "));
    let _ = x1.laser_off().await;
    x1.disconnect().await;
}
