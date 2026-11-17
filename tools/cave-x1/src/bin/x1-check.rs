//! A quick health check: connect, take three shots with the laser on, print each reading, its errors and flags.
use cave_x1::X1;
use std::time::Duration;
use tokio::time::sleep;

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
    let stored = x1.drain(Duration::from_millis(1500)).await;
    println!("{} stored shot(s) waiting.", stored.len());
    for _ in 0..3 {
        let _ = x1.laser_on().await;
        sleep(Duration::from_millis(1500)).await;
        match x1.shoot(Duration::from_secs(4)).await {
            Ok(s) => {
                let p = &s.packet;
                println!(
                    "{:.3} m  az {:.1}  inc {:.1}  roll {:.1}  dip {:?}  leg {}  scan {}  calib {}  errors {}  quality {:?}",
                    p.distance, p.azimuth, p.inclination, p.roll, p.sensors.first().map(|x| x.dip), p.leg, p.scan, p.calibration,
                    p.errors.len(), p.quality
                );
            }
            Err(e) => println!("no shot: {e}"),
        }
    }
    let _ = x1.laser_off().await;
    x1.disconnect().await;
}
