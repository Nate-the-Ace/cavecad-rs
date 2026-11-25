//! A blocking raw byte link to the X1 over Bluetooth, for `x1flash --ble`.
//!
//! The X1 (and its bootloader, in boot mode) is a Nordic UART: bytes written to the write characteristic go to the
//! device, its answers arrive as notifications. Unlike [`crate::X1`] this does not interpret anything and never
//! acknowledges shots: the flasher speaks the bootloader's own protocol on top. Writes go out in 20-byte pieces
//! with a short gap, because the X1's Bluetooth module takes 20 bytes a write (what the phone app does too).
//!
//! macOS only lets a program use Bluetooth if it runs as an app that says why (`run-mac.sh` wraps it).

use btleplug::api::{
    Central, CentralEvent, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Manager, Peripheral};
use cave_instrument::cavway;
use futures::StreamExt;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Bytes per Bluetooth write (the X1's module limit) and the pause after each.
pub const PIECE: usize = 20;
pub const GAP: Duration = Duration::from_millis(12);

const NAME_PREFIX: &str = "CavwayX1";

pub struct RawLink {
    rt: tokio::runtime::Runtime,
    peripheral: Peripheral,
    write: Characteristic,
    rx: mpsc::Receiver<Vec<u8>>,
    name: String,
}

fn uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).expect("a valid UUID constant")
}

/// Names of X1s that advertise within `wait` (for `x1flash ble-scan`).
pub fn scan(wait: Duration) -> Result<Vec<String>, String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let adapter = adapter().await?;
        let mut events = adapter.events().await.map_err(|e| e.to_string())?;
        adapter.start_scan(ScanFilter::default()).await.map_err(|e| e.to_string())?;
        let deadline = Instant::now() + wait;
        let mut names: Vec<String> = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let Ok(Some(ev)) = tokio::time::timeout(left, events.next()).await else {
                break;
            };
            let (CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id)) = ev else {
                continue;
            };
            let Ok(p) = adapter.peripheral(&id).await else { continue };
            let Ok(Some(props)) = p.properties().await else { continue };
            if let Some(n) = props.local_name.filter(|n| n.starts_with(NAME_PREFIX)) {
                if !names.contains(&n) {
                    names.push(n);
                }
            }
        }
        let _ = adapter.stop_scan().await;
        Ok(names)
    })
}

async fn adapter() -> Result<btleplug::platform::Adapter, String> {
    Manager::new()
        .await
        .map_err(|e| format!("no Bluetooth: {e}"))?
        .adapters()
        .await
        .map_err(|e| format!("no Bluetooth: {e}"))?
        .into_iter()
        .next()
        .ok_or_else(|| "no Bluetooth adapter (is Bluetooth on, and is this running as an app that may use it?)".to_string())
}

impl RawLink {
    /// Finds an X1 advertising within `wait` (the first, or the one whose name contains `only`) and connects.
    pub fn connect(wait: Duration, only: Option<&str>) -> Result<RawLink, String> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        let (peripheral, name, write, notifications) = rt.block_on(async {
            let adapter = adapter().await?;
            let mut events = adapter.events().await.map_err(|e| e.to_string())?;
            adapter.start_scan(ScanFilter::default()).await.map_err(|e| e.to_string())?;
            let deadline = Instant::now() + wait;
            let found = loop {
                let left = deadline.saturating_duration_since(Instant::now());
                let Ok(Some(ev)) = tokio::time::timeout(left, events.next()).await else {
                    break None;
                };
                let (CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id)) = ev else {
                    continue;
                };
                let Ok(p) = adapter.peripheral(&id).await else { continue };
                let Ok(Some(props)) = p.properties().await else { continue };
                if let Some(n) = props
                    .local_name
                    .filter(|n| n.starts_with(NAME_PREFIX) && only.is_none_or(|o| n.contains(o)))
                {
                    break Some((p, n));
                }
            };
            let _ = adapter.stop_scan().await;
            let (p, name) = found.ok_or_else(|| {
                "no X1 found over Bluetooth: is it on (in boot mode: both arrows + DIST), close, and not connected to a phone or another app?"
                    .to_string()
            })?;
            let link = |e: btleplug::Error| format!("Bluetooth link: {e}");
            tokio::time::timeout(Duration::from_secs(15), p.connect())
                .await
                .map_err(|_| "Bluetooth connect timed out".to_string())?
                .map_err(link)?;
            p.discover_services().await.map_err(link)?;
            let chars = p.characteristics();
            let find = |u: &str| chars.iter().find(|c| c.uuid == uuid(u)).cloned();
            let write = find(cavway::WRITE).ok_or("the X1 has no UART write characteristic")?;
            let notify = find(cavway::NOTIFY).ok_or("the X1 has no UART notify characteristic")?;
            p.subscribe(&notify).await.map_err(link)?;
            let stream = p.notifications().await.map_err(link)?;
            Ok::<_, String>((p, name, write, stream))
        })?;
        let (tx, rx) = mpsc::channel();
        rt.spawn(async move {
            let mut stream = notifications;
            while let Some(n) = stream.next().await {
                if tx.send(n.value).is_err() {
                    break;
                }
            }
        });
        Ok(RawLink { rt, peripheral, write, rx, name })
    }

    /// The name it advertises (`CavwayX1-0312`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Writes `bytes` in 20-byte pieces, a short gap after each. Returns once all are written.
    pub fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.rt.block_on(async {
            for part in bytes.chunks(PIECE) {
                self.peripheral
                    .write(&self.write, part, WriteType::WithoutResponse)
                    .await
                    .map_err(|e| format!("Bluetooth write failed: {e}"))?;
                tokio::time::sleep(GAP).await;
            }
            Ok(())
        })
    }

    /// Whatever arrives within `ms` (returns once something has, plus anything else already queued).
    pub fn recv(&mut self, ms: u32) -> Vec<u8> {
        let mut out = match self.rx.recv_timeout(Duration::from_millis(u64::from(ms))) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        while let Ok(more) = self.rx.try_recv() {
            out.extend(more);
        }
        out
    }

    /// Drops anything received and not yet read.
    pub fn discard(&mut self) {
        while self.rx.try_recv().is_ok() {}
    }

    pub fn disconnect(self) {
        let RawLink { rt, peripheral, .. } = self;
        let _ = rt.block_on(peripheral.disconnect());
    }
}
