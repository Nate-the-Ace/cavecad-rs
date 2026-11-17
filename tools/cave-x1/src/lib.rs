//! Talks to a Cavway X1 over Bluetooth from a computer, as building blocks: find it, connect, turn the laser on or off,
//! fire a shot and wait for its reading, read the shots it sends (each acknowledged at once, repeats dropped), and
//! time all of it. The wire format is CaveCAD's own (`cave_instrument::cavway`), the same code the app runs, so what is
//! learned here holds there.
//!
//! ```no_run
//! # async fn demo() -> Result<(), cave_x1::Error> {
//! let mut x1 = cave_x1::X1::find(std::time::Duration::from_secs(15)).await?;
//! x1.drain(std::time::Duration::from_millis(1500)).await; // what it had stored
//! x1.laser_on().await?;
//! tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
//! let shot = x1.shoot(std::time::Duration::from_secs(4)).await?;
//! println!("{:.3} m, {:.1}°, {:.1}°", shot.packet.distance, shot.packet.azimuth, shot.packet.inclination);
//! # Ok(()) }
//! ```

use btleplug::api::{Central, CentralEvent, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Manager, Peripheral};
use cave_instrument::cavway::{self, Packet};
use futures::StreamExt;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use uuid::Uuid;

/// What went wrong.
#[derive(Debug)]
pub enum Error {
    /// No Bluetooth on this computer, or it is off.
    NoBluetooth(String),
    /// No X1 was seen in time (asleep, out of range, or connected to a phone).
    NotFound,
    /// The X1 was found but the link could not be made.
    Link(String),
    /// Nothing came back in time.
    Timeout,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoBluetooth(e) => write!(f, "no Bluetooth: {e}"),
            Error::NotFound => write!(f, "no Cavway X1 found: is it on, close by, and not connected to a phone?"),
            Error::Link(e) => write!(f, "could not talk to the X1: {e}"),
            Error::Timeout => write!(f, "the X1 did not answer in time"),
        }
    }
}

impl std::error::Error for Error {}

/// A shot the X1 sent, when it arrived.
#[derive(Clone, Debug)]
pub struct Shot {
    /// The decoded packet (distance, azimuth, inclination, leg flag, mark, scan flags, errors…).
    pub packet: Packet,
    /// When it arrived on this computer.
    pub at: Instant,
    /// The bytes as received.
    pub raw: Vec<u8>,
}

/// What the real X1 was measured to do (FINDINGS.md has how). Tools and the app plan around these.
pub mod timing {
    use std::time::Duration;
    /// From laser on to the reading, whenever the trigger follows: the X1 waits internally (1.40 to 1.50 s seen).
    pub const LASER_TO_READING: Duration = Duration::from_millis(1500);
    /// Laser already on and settled: trigger to reading, usually (0.36 to 0.39 s seen; sometimes 2.2 s).
    pub const SETTLED_TRIGGER_TO_READING: Duration = Duration::from_millis(400);
    /// Wait at least this long before deciding a shot is lost (a settled shot once took 2.2 s).
    pub const LOST_AFTER: Duration = Duration::from_secs(3);
}

/// The single-byte commands the X1 is known to take.
pub mod command {
    /// Laser on.
    pub const LASER_ON: u8 = cave_instrument::cavway::CMD_LASER_ON;
    /// Laser off.
    pub const LASER_OFF: u8 = cave_instrument::cavway::CMD_LASER_OFF;
    /// Measure, as a press of DIST does with the laser on. Does nothing with the laser off, and the laser goes off
    /// after every shot, so each shot is `LASER_ON` then `TRIGGER` (see [`crate::X1::fire`]).
    pub const TRIGGER: u8 = cave_instrument::cavway::CMD_TRIGGER;

    /// Commands [`crate::X1::send`] refuses: power off, calibration mode, and memory or firmware writes (which take
    /// an address and data; sent bare they could write garbage). FINDINGS.md lists what each is.
    pub fn refused(cmd: u8) -> bool {
        matches!(cmd, 0x30..=0x34 | 0x39..=0x3c | 0x3e)
    }
}

/// A connected X1.
pub struct X1 {
    peripheral: Peripheral,
    write: Characteristic,
    shots: mpsc::UnboundedReceiver<Shot>,
    name: String,
    /// Every write, with when it was sent: for timing.
    pub sent: Vec<(Instant, Vec<u8>)>,
}

const NAME_PREFIX: &str = "CavwayX1";

fn uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).expect("a valid UUID constant")
}

impl X1 {
    /// Finds the first X1 that advertises within `wait` and connects to it.
    pub async fn find(wait: Duration) -> Result<X1, Error> {
        let manager = Manager::new().await.map_err(|e| Error::NoBluetooth(e.to_string()))?;
        let adapter = manager
            .adapters()
            .await
            .map_err(|e| Error::NoBluetooth(e.to_string()))?
            .into_iter()
            .next()
            .ok_or_else(|| Error::NoBluetooth("no adapter".into()))?;
        let mut events = adapter.events().await.map_err(|e| Error::NoBluetooth(e.to_string()))?;
        adapter
            .start_scan(ScanFilter::default())
            .await
            .map_err(|e| Error::NoBluetooth(e.to_string()))?;
        let deadline = Instant::now() + wait;
        let found = loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break None;
            }
            let Ok(Some(ev)) = tokio::time::timeout(left, events.next()).await else { break None };
            let id = match ev {
                CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id) => id,
                _ => continue,
            };
            let Ok(p) = adapter.peripheral(&id).await else { continue };
            let Ok(Some(props)) = p.properties().await else { continue };
            if let Some(name) = props.local_name.filter(|n| n.starts_with(NAME_PREFIX)) {
                break Some((p, name));
            }
        };
        let _ = adapter.stop_scan().await;
        let (peripheral, name) = found.ok_or(Error::NotFound)?;
        X1::open(peripheral, name).await
    }

    async fn open(peripheral: Peripheral, name: String) -> Result<X1, Error> {
        let link = |e: btleplug::Error| Error::Link(e.to_string());
        tokio::time::timeout(Duration::from_secs(15), peripheral.connect())
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(link)?;
        peripheral.discover_services().await.map_err(link)?;
        let chars = peripheral.characteristics();
        let find = |u: &str| chars.iter().find(|c| c.uuid == uuid(u)).cloned();
        let write = find(cavway::WRITE).ok_or_else(|| Error::Link("no write characteristic".into()))?;
        let notify = find(cavway::NOTIFY).ok_or_else(|| Error::Link("no notify characteristic".into()))?;
        peripheral.subscribe(&notify).await.map_err(link)?;
        let mut stream = peripheral.notifications().await.map_err(link)?;
        let (tx, shots) = mpsc::unbounded_channel();
        // every shot is acknowledged the moment it arrives (or the X1 keeps offering it), and a repeat is dropped
        let (ack_peripheral, ack_write) = (peripheral.clone(), write.clone());
        tokio::spawn(async move {
            let mut recent: Vec<Vec<u8>> = Vec::new();
            while let Some(n) = stream.next().await {
                let at = Instant::now();
                let Some(packet) = cavway::decode_packet(&n.value) else { continue };
                let _ = ack_peripheral
                    .write(&ack_write, &cavway::ack_frame(packet.flag), WriteType::WithoutResponse)
                    .await;
                if recent.contains(&n.value) {
                    continue;
                }
                recent.push(n.value.clone());
                if recent.len() > 8 {
                    recent.remove(0);
                }
                if tx.send(Shot { packet, at, raw: n.value }).is_err() {
                    break;
                }
            }
        });
        Ok(X1 {
            peripheral,
            write,
            shots,
            name,
            sent: Vec::new(),
        })
    }

    /// The name it advertises (`CavwayX1-0312`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Sends a single-byte command (see [`command`]); returns when it was sent. Refuses the ones that could power off,
    /// change modes or write memory ([`command::refused`]).
    pub async fn send(&mut self, cmd: u8) -> Result<Instant, Error> {
        if command::refused(cmd) {
            return Err(Error::Link(format!("command 0x{cmd:02x} is refused by this library (see FINDINGS.md)")));
        }
        let frame = cavway::command(cmd);
        self.peripheral
            .write(&self.write, &frame, WriteType::WithoutResponse)
            .await
            .map_err(|e| Error::Link(e.to_string()))?;
        let at = Instant::now();
        self.sent.push((at, frame));
        Ok(at)
    }

    /// Laser on.
    pub async fn laser_on(&mut self) -> Result<Instant, Error> {
        self.send(command::LASER_ON).await
    }

    /// Laser off.
    pub async fn laser_off(&mut self) -> Result<Instant, Error> {
        self.send(command::LASER_OFF).await
    }

    /// Fires a shot and waits up to `wait` for the next shot to arrive (the one it made).
    pub async fn shoot(&mut self, wait: Duration) -> Result<Shot, Error> {
        self.send(command::TRIGGER).await?;
        self.next_shot(wait).await.ok_or(Error::Timeout)
    }

    /// A shot from cold, as the X1 needs it: laser on, trigger, and the reading (about 1.45 s; waits up to `wait`).
    /// Call it only once the previous reading has arrived: a laser on during a measurement loses the next shot.
    pub async fn fire(&mut self, wait: Duration) -> Result<Shot, Error> {
        self.laser_on().await?;
        self.shoot(wait).await
    }

    /// A leg as Station to station takes it: the first shot with the laser already on (aimed), the next two from cold.
    /// About 3.3 s. Returns the three shots; the X1 flags the third as LEG when they agree.
    pub async fn leg(&mut self) -> Result<[Shot; 3], Error> {
        let a = self.shoot(timing::LOST_AFTER).await?;
        let b = self.fire(timing::LOST_AFTER).await?;
        let c = self.fire(timing::LOST_AFTER).await?;
        Ok([a, b, c])
    }

    /// The next shot the X1 sends (fired from here or with its own button), if one comes within `wait`.
    pub async fn next_shot(&mut self, wait: Duration) -> Option<Shot> {
        tokio::time::timeout(wait, self.shots.recv()).await.ok().flatten()
    }

    /// Every shot that arrives until nothing has come for `quiet`: what the X1 had stored, sent as it connects.
    pub async fn drain(&mut self, quiet: Duration) -> Vec<Shot> {
        let mut out = Vec::new();
        while let Some(s) = self.next_shot(quiet).await {
            out.push(s);
        }
        out
    }

    /// Lets go of the X1.
    pub async fn disconnect(self) {
        let _ = self.peripheral.disconnect().await;
    }
}
