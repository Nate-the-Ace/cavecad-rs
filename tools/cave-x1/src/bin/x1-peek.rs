//! Reads X1 memory with `3d` (read only: nothing on the X1 changes) and prints the bytes. Addresses are the X1's own
//! map (FIRMWARE.md): E000 = the app's version block, E004 = the bootloader's version, 8000 = the clock.
//!   tools/cave-x1/run-mac.sh x1-peek E000:124 E004:4

use cave_x1::X1;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let reads: Vec<(u16, usize)> = std::env::args()
        .skip(1)
        .filter_map(|a| {
            let (addr, n) = a.split_once(':').unwrap_or((&a, "4"));
            Some((u16::from_str_radix(addr.trim_start_matches("0x"), 16).ok()?, n.parse().ok()?))
        })
        .collect();
    let mut x1 = match X1::find(Duration::from_secs(30)).await {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    println!("Connected to {}.", x1.name());
    let stored = x1.drain(Duration::from_millis(1500)).await;
    println!("{} stored shot(s) drained.", stored.len());
    for (addr, n) in reads {
        match x1.read_memory(addr, n, Duration::from_secs(3)).await {
            Ok(r) => {
                println!("{addr:04X} ({} bytes):", r.data.len());
                for (i, chunk) in r.data.chunks(16).enumerate() {
                    let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
                    let txt: String =
                        chunk.iter().map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' }).collect();
                    println!("  +{:02x}: {:<48} {txt}", i * 16, hex.join(" "));
                }
            }
            Err(e) => println!("{addr:04X}: {e}"),
        }
    }
    x1.disconnect().await;
}
