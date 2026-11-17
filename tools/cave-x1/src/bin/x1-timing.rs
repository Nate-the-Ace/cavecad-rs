//! Times the X1's remote shots, to make Station to station as fast as the X1 allows. Leave the X1 still, pointed at a
//! wall a few metres away, awake and NOT connected to a phone. Every shot it takes here is acknowledged and stays only in
//! the X1's memory (nothing reaches a cave).
//!
//! Each test is run `REPS` times. Times are from the command that starts the shot to the reading arriving.
//!   cargo run --release --bin x1-timing            (all tests)
//!   cargo run --release --bin x1-timing -- quick   (fewer repeats)

use cave_x1::X1;
use std::time::{Duration, Instant};
use tokio::time::sleep;

const WAIT: Duration = Duration::from_secs(4);

fn ms(d: Duration) -> String {
    format!("{:>5}", d.as_millis())
}

struct Row {
    name: String,
    times: Vec<Option<Duration>>,
}

impl Row {
    fn print(&self) {
        let ok: Vec<_> = self.times.iter().flatten().copied().collect();
        let fails = self.times.len() - ok.len();
        let avg = if ok.is_empty() { None } else { Some(ok.iter().sum::<Duration>() / ok.len() as u32) };
        let max = ok.iter().max();
        println!(
            "{:<44} ok {}/{}  avg {} ms  max {} ms  [{}]",
            self.name,
            ok.len(),
            self.times.len(),
            avg.map(ms).unwrap_or_else(|| "    -".into()),
            max.map(|d| ms(*d)).unwrap_or_else(|| "    -".into()),
            self.times
                .iter()
                .map(|t| t.map(|d| d.as_millis().to_string()).unwrap_or_else(|| "x".into()))
                .collect::<Vec<_>>()
                .join(" "),
        );
        let _ = fails;
    }
}

/// Fires once and returns the time from `start` to the reading, or None.
async fn fire(x1: &mut X1, start: Instant) -> Option<Duration> {
    match x1.shoot(WAIT).await {
        Ok(s) => Some(s.at.duration_since(start)),
        Err(_) => None,
    }
}

/// Rests between tries so each starts from the same state: laser off, a pause, anything late swallowed.
async fn rest(x1: &mut X1) {
    let _ = x1.laser_off().await;
    x1.drain(Duration::from_millis(1200)).await;
}

#[tokio::main]
async fn main() {
    let quick = std::env::args().any(|a| a == "quick");
    let reps = if quick { 3 } else { 5 };
    println!("Looking for a Cavway X1 (wake it; disconnect it from the phone)…");
    let mut x1 = match X1::find(Duration::from_secs(30)).await {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    println!("Connected to {}.", x1.name());
    let stored = x1.drain(Duration::from_millis(2000)).await;
    println!("{} stored shot(s) were waiting (acknowledged, ignored).", stored.len());
    let mut rows = Vec::new();

    // 1. The laser on and settled: the X1's own measuring time.
    let mut r = Row { name: "laser on 1500 ms before, then trigger".into(), times: vec![] };
    for _ in 0..reps {
        rest(&mut x1).await;
        let _ = x1.laser_on().await;
        sleep(Duration::from_millis(1500)).await;
        let t = Instant::now();
        r.times.push(fire(&mut x1, t).await);
    }
    r.print();
    rows.push(r);

    // 2. Laser on then trigger after d: time from laser on to reading.
    for d in [0u64, 100, 250, 500, 750, 1000] {
        let mut r = Row { name: format!("laser on, trigger after {d} ms (from laser on)"), times: vec![] };
        for _ in 0..reps {
            rest(&mut x1).await;
            let t = x1.laser_on().await.unwrap_or_else(|_| Instant::now());
            sleep(Duration::from_millis(d)).await;
            r.times.push(fire(&mut x1, t).await);
        }
        r.print();
        rows.push(r);
    }

    // 3. Trigger alone with the laser off: does it shoot by itself?
    let mut r = Row { name: "trigger only, laser off".into(), times: vec![] };
    for _ in 0..reps {
        rest(&mut x1).await;
        let t = Instant::now();
        r.times.push(fire(&mut x1, t).await);
    }
    r.print();
    rows.push(r);

    // 4. Right after a shot: trigger again at once, no laser on (does the laser stay on after a remote shot?).
    let mut r = Row { name: "second trigger straight after a shot".into(), times: vec![] };
    for _ in 0..reps {
        rest(&mut x1).await;
        let _ = x1.laser_on().await;
        sleep(Duration::from_millis(1500)).await;
        let _ = x1.shoot(WAIT).await;
        let t = Instant::now();
        r.times.push(fire(&mut x1, t).await);
    }
    r.print();
    rows.push(r);

    // 5. Three in a row, as Station to station, laser on before each later shot at the best delay seen in test 2.
    let best = rows[1..7]
        .iter()
        .zip([0u64, 100, 250, 500, 750, 1000])
        .filter(|(r, _)| r.times.iter().all(Option::is_some))
        .min_by_key(|(r, _)| r.times.iter().flatten().sum::<Duration>())
        .map(|(_, d)| d)
        .unwrap_or(1000);
    let mut r = Row { name: format!("leg of 3 (laser on + {best} ms before shots 2, 3)"), times: vec![] };
    let mut legs = 0;
    for _ in 0..reps {
        rest(&mut x1).await;
        let _ = x1.laser_on().await;
        sleep(Duration::from_millis(1500)).await;
        let t = Instant::now();
        let mut ok = true;
        let mut last = None;
        for i in 0..3 {
            if i > 0 {
                let _ = x1.laser_on().await;
                sleep(Duration::from_millis(best)).await;
            }
            match x1.shoot(WAIT).await {
                Ok(s) => last = Some(s),
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if let Some(s) = &last
            && s.packet.leg
        {
            legs += 1;
        }
        r.times.push(if ok { last.map(|s| s.at.duration_since(t)) } else { None });
    }
    r.print();
    println!("  the X1 flagged {legs} of {reps} as a leg");
    let _ = x1.laser_off().await;
    x1.disconnect().await;
}
