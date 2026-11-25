//! The part between "bytes arrived" and "a reading exists", shared by every transport (Web Bluetooth and the
//! simulator) so the simulator exercises the same decoding, duplicate handling and acknowledging as a real
//! instrument: decode, acknowledge, drop repeats, report.
//!
//! A link holds no clock and calls nothing back: every method returns the [`LinkOut`]s it caused, in order.

use crate::bric::{self, ErrorPair, Meta, Primary};
use crate::caveble::{self, LegFilter};
use crate::{Mark, Reading, cavway, cavway_callouts, hex, sap5, tidy};
use serde::Serialize;

/// Something a link did with a frame.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LinkOut {
    /// A line for the capture log (the raw frame as hex, or a note about it) and where it came from.
    Frame {
        /// Hex or a note in brackets.
        hex: String,
        /// Where it came from.
        from: String,
    },
    /// A shot is ready.
    Reading {
        /// The shot.
        reading: Reading,
    },
    /// A calibration shot (for the calibration guide; never surveyed): where the instrument pointed.
    Calibration {
        /// Degrees.
        azimuth: f64,
        /// Degrees, up positive.
        inclination: f64,
    },
    /// Bytes to write back to the instrument (an acknowledgement).
    Send {
        /// The message, already framed.
        bytes: Vec<u8>,
    },
}

fn frame(hex: impl Into<String>, from: &str) -> LinkOut {
    LinkOut::Frame {
        hex: hex.into(),
        from: from.into(),
    }
}

// ---- the SAP6 (CaveBLE) ----

/// The SAP6's path from bytes to readings: decode, acknowledge every leg, drop a repeat.
#[derive(Clone, Debug)]
pub struct LegLink {
    filter: LegFilter,
    source: String,
}

impl LegLink {
    /// A link whose readings say they came from `source`.
    pub fn new(source: &str) -> Self {
        LegLink {
            filter: LegFilter::default(),
            source: source.into(),
        }
    }

    /// A frame arrived on the leg characteristic.
    pub fn on_frame(&mut self, bytes: &[u8], from: &str) -> Vec<LinkOut> {
        let mut out = vec![frame(hex(bytes), from)];
        let Some(leg) = caveble::decode_leg(bytes) else {
            out.push(frame("(not a CaveBLE leg: ignored)", from));
            return out;
        };
        // Always: an unacknowledged frame is sent again forever.
        out.push(LinkOut::Send {
            bytes: vec![caveble::ack_for(leg.seq)],
        });
        if self.filter.is_new(bytes) {
            out.push(LinkOut::Reading {
                reading: Reading {
                    roll: Some(tidy(leg.roll, 3)),
                    ..Reading::new(
                        tidy(leg.distance, 4),
                        tidy(leg.azimuth, 3),
                        tidy(leg.inclination, 3),
                        &self.source,
                    )
                },
            });
        }
        out
    }

    /// Forgets the last frame.
    pub fn reset(&mut self) {
        self.filter.reset();
    }
}

// ---- the X1 ----

/// How many recent packets are remembered to recognise one the X1 sent again.
const RECENT: usize = 8;

/// The X1's path from bytes to readings: decode, acknowledge (every packet, a repeat too, or the X1 keeps
/// offering it), drop a packet that is a byte-for-byte repeat of a recent one (every real shot differs, if
/// only by its timestamp), leave calibration shots out, report the rest. Shots are NOT grouped into legs here:
/// that is [`crate::legs`].
#[derive(Clone, Debug)]
pub struct X1Link {
    recent: Vec<String>,
    source: String,
    tolerance: cavway::Tolerance,
    /// The X1's feature list, once it has been read (`1.1.7-cc9` and later answer for it).
    callouts: Option<Vec<String>>,
    list_reader: cavway_callouts::ListReader,
}

impl X1Link {
    /// A link whose readings say they came from `source`.
    pub fn new(source: &str) -> Self {
        X1Link {
            recent: Vec::new(),
            source: source.into(),
            tolerance: cavway::Tolerance::default(),
            callouts: None,
            list_reader: cavway_callouts::ListReader::default(),
        }
    }

    /// The memory reads that ask the X1 for its feature list. Write them to the X1 after connecting; the answers come back
    /// through [`X1Link::on_frame`], which then names a picked feature. An X1 that cannot answer (stock firmware, or a
    /// `-cc` build before 9) just never does.
    pub fn list_request(&mut self) -> Vec<Vec<u8>> {
        self.callouts = None;
        self.list_reader.reset();
        cavway_callouts::read_frames()
    }

    /// The feature list read from the X1, if it has answered.
    pub fn callouts(&self) -> Option<&[String]> {
        self.callouts.as_deref()
    }

    /// The name of a picked feature: from the X1's own list, or `Feature N` while that is not known.
    fn callout_name(&self, n: u8) -> String {
        self.callouts
            .as_ref()
            .and_then(|l| l.get(usize::from(n).checked_sub(1)?))
            .cloned()
            .unwrap_or_else(|| format!("Feature {n}"))
    }

    /// How touchy to be about the X1's accuracy complaints (see [`cavway::Tolerance`]).
    pub fn set_tolerance(&mut self, t: cavway::Tolerance) {
        self.tolerance = t;
    }

    /// A notification arrived on the X1's read characteristic.
    pub fn on_frame(&mut self, bytes: &[u8], from: &str) -> Vec<LinkOut> {
        let key = hex(bytes);
        let mut out = vec![frame(key.clone(), from)];
        if let Some(reply) = cavway::parse_reply(bytes).filter(|_| bytes[0] == 0x3d) {
            if let Some(names) = self.list_reader.feed(&reply) {
                out.push(frame(
                    format!("(the X1's feature list: {})", names.join(", ")),
                    from,
                ));
                self.callouts = Some(names);
            }
            return out;
        }
        let Some(p) = cavway::decode_packet(bytes) else {
            out.push(frame(
                if bytes.len() < cavway::SHORT_PACKET_BYTES {
                    "(too short for a shot packet: ignored)"
                } else {
                    "(not a shot packet: ignored)"
                },
                from,
            ));
            return out;
        };
        out.push(LinkOut::Send {
            bytes: cavway::ack_frame(p.flag),
        });
        if self.recent.contains(&key) {
            return out;
        }
        self.recent.push(key);
        if self.recent.len() > RECENT {
            self.recent.remove(0);
        }
        if p.calibration {
            out.push(frame(
                "(calibration shot: acknowledged, not surveyed)",
                from,
            ));
            out.push(LinkOut::Calibration {
                azimuth: tidy(p.azimuth, 3),
                inclination: tidy(p.inclination, 3),
            });
            return out;
        }
        out.push(LinkOut::Reading {
            reading: Reading {
                roll: Some(tidy(p.roll, 3)),
                leg: Some(p.leg && !p.scan),
                mark: Some(p.mark),
                callout: p.callout.map(|n| self.callout_name(n)),
                scan: p.scan.then_some(true),
                sweep_start: (p.scan && p.sweep_start).then_some(true),
                error: cavway::describe_errors(&cavway::beyond(&p.errors, self.tolerance)),
                quality: p.quality,
                ..Reading::new(
                    tidy(p.distance, 4),
                    tidy(p.azimuth, 3),
                    tidy(p.inclination, 3),
                    &self.source,
                )
            },
        });
        out
    }

    /// Forgets the recent packets.
    pub fn reset(&mut self) {
        self.recent.clear();
    }
}

// ---- the SAP5 ----

/// The SAP5's path from bytes to readings: each notification is one 8-byte DistoX packet. There is nothing to
/// acknowledge. A packet byte for byte the same as one of the last few is the same shot repeated (the
/// sequence bit flips with every real packet, so two real shots never match).
#[derive(Clone, Debug)]
pub struct Sap5Link {
    recent: Vec<String>,
    source: String,
}

impl Sap5Link {
    /// A link whose readings say they came from `source`.
    pub fn new(source: &str) -> Self {
        Sap5Link {
            recent: Vec::new(),
            source: source.into(),
        }
    }

    /// A notification arrived.
    pub fn on_frame(&mut self, bytes: &[u8], from: &str) -> Vec<LinkOut> {
        let key = hex(bytes);
        let mut out = vec![frame(key.clone(), from)];
        let Some(p) = sap5::decode_packet(bytes, true) else {
            out.push(frame("(not a DistoX data packet: ignored)", from));
            return out;
        };
        if self.recent.contains(&key) {
            return out;
        }
        self.recent.push(key);
        if self.recent.len() > 4 {
            self.recent.remove(0);
        }
        out.push(LinkOut::Reading {
            reading: Reading {
                roll: Some(tidy(p.roll, 3)),
                mark: Some(Mark::None),
                ..Reading::new(
                    tidy(p.distance, 4),
                    tidy(p.azimuth, 3),
                    tidy(p.inclination, 3),
                    &self.source,
                )
            },
        });
        out
    }

    /// Forgets the recent packets.
    pub fn reset(&mut self) {
        self.recent.clear();
    }
}

// ---- the BRIC ----

/// Which of a BRIC shot's three indications arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BricKind {
    /// When, distance, azimuth, inclination.
    Primary,
    /// Index, dip, roll and so on.
    Meta,
    /// Up to two errors.
    Errors,
}

impl BricKind {
    /// Reads the name the transport uses (`primary`, `meta`, `errors`).
    pub fn parse(s: &str) -> Option<BricKind> {
        match s {
            "primary" => Some(BricKind::Primary),
            "meta" => Some(BricKind::Meta),
            "errors" => Some(BricKind::Errors),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            BricKind::Primary => "primary",
            BricKind::Meta => "meta",
            BricKind::Errors => "errors",
        }
    }
}

#[derive(Clone, Debug)]
struct Pending {
    primary: Primary,
    meta: Option<Meta>,
    errors: Option<[ErrorPair; 2]>,
}

/// The BRIC's path from indications to readings. A shot is three indications (primary, then meta, then
/// errors): it is complete when the errors arrive. If they never do (some firmware sends the primary alone) it
/// is written anyway when the host calls [`BricLink::flush`] ([`bric::SETTLE_MS`] after the primary; the link
/// keeps no timer, so the host asks [`BricLink::has_pending`]), or as soon as the next shot's primary shows it
/// has been left behind. A primary the same as the last one is a repeat. A shot with no length is not a shot.
/// A jump in the running index is reported (shots were missed).
#[derive(Clone, Debug)]
pub struct BricLink {
    last_primary: Option<String>,
    last_index: Option<u32>,
    pending: Option<Pending>,
    source: String,
}

impl BricLink {
    /// A link whose readings say they came from `source`.
    pub fn new(source: &str) -> Self {
        BricLink {
            last_primary: None,
            last_index: None,
            pending: None,
            source: source.into(),
        }
    }

    /// An indication arrived: `kind` says which characteristic.
    pub fn on_frame(&mut self, kind: BricKind, bytes: &[u8]) -> Vec<LinkOut> {
        let name = kind.name();
        let mut out = vec![frame(hex(bytes), name)];
        match kind {
            BricKind::Primary => {
                let Some(primary) = bric::decode_primary(bytes) else {
                    out.push(frame("(not a BRIC primary indication: ignored)", name));
                    return out;
                };
                let key = hex(bytes);
                if self.last_primary.as_deref() == Some(key.as_str()) {
                    return out;
                }
                self.last_primary = Some(key);
                // A shot still waiting for its errors has been left behind: write it as it is.
                out.extend(self.flush());
                self.pending = Some(Pending {
                    primary,
                    meta: None,
                    errors: None,
                });
            }
            BricKind::Meta => {
                if let (Some(meta), Some(p)) = (bric::decode_meta(bytes), self.pending.as_mut()) {
                    p.meta = Some(meta);
                }
            }
            BricKind::Errors => {
                let errors = bric::decode_errors(bytes);
                if let Some(p) = self.pending.as_mut() {
                    p.errors = errors;
                    out.extend(self.flush());
                }
            }
        }
        out
    }

    /// True while a shot is waiting for the rest of its indications: the host should call
    /// [`BricLink::flush`] after [`bric::SETTLE_MS`] if nothing else arrives first.
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Writes the shot being assembled, with whatever of it has arrived.
    pub fn flush(&mut self) -> Vec<LinkOut> {
        let Some(p) = self.pending.take() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(meta) = &p.meta {
            if meta.kind == 0
                && let Some(last) = self.last_index
                && meta.index > last + 1
            {
                out.push(frame(
                    format!(
                        "(missed {} shot(s) before number {})",
                        meta.index - last - 1,
                        meta.index
                    ),
                    "primary",
                ));
            }
            self.last_index = Some(meta.index);
        }
        if p.primary.distance <= 0.01 {
            out.push(frame("(a shot with no length: ignored)", "primary"));
            return out;
        }
        out.push(LinkOut::Reading {
            reading: Reading {
                roll: p.meta.as_ref().map(|m| tidy(m.roll, 3)),
                mark: Some(Mark::None),
                error: bric::describe_errors(p.errors.as_ref()),
                ..Reading::new(
                    tidy(p.primary.distance, 4),
                    tidy(p.primary.azimuth, 3),
                    tidy(p.primary.inclination, 3),
                    &self.source,
                )
            },
        });
        out
    }

    /// Writes what is waiting, and forgets the last primary and index.
    pub fn reset(&mut self) -> Vec<LinkOut> {
        let out = self.flush();
        self.last_primary = None;
        self.last_index = None;
        out
    }
}

#[cfg(test)]
mod x1_callout_tests {
    use super::*;
    use crate::cavway::{PacketSpec, encode_packet};

    /// A shot packet with a feature picked on the X1: bytes 54 and 55 are the pick and 0xCA.
    fn shot_with_pick(n: u8) -> Vec<u8> {
        let mut p = encode_packet(&PacketSpec::default());
        p[54] = n;
        p[55] = 0xca;
        p
    }

    fn reading(out: Vec<LinkOut>) -> Reading {
        out.into_iter()
            .find_map(|o| match o {
                LinkOut::Reading { reading } => Some(reading),
                _ => None,
            })
            .expect("a reading")
    }

    /// The X1's answers to the list request, for the list `names`.
    fn answers(names: &[&str]) -> Vec<Vec<u8>> {
        let slot =
            cavway_callouts::slot_bytes(&names.iter().map(|n| n.to_string()).collect::<Vec<_>>())
                .unwrap();
        (0..8)
            .map(|i| {
                let at = i * 60;
                let n = 60.min(426 - at);
                let a = 0xc400 + at;
                let mut v = vec![0x3d, (a & 0xff) as u8, (a >> 8) as u8, n as u8];
                v.extend_from_slice(&slot[at..at + n]);
                v
            })
            .collect()
    }

    #[test]
    fn a_pick_is_named_from_the_list_read_from_the_x1() {
        let mut link = X1Link::new("x1");
        // before the list is known, a pick is honest about it
        assert_eq!(
            reading(link.on_frame(&shot_with_pick(2), "t"))
                .callout
                .as_deref(),
            Some("Feature 2")
        );
        let asks = link.list_request();
        assert_eq!(asks.len(), 8);
        for a in answers(&["Pit", "Water crawl", "Bats"]) {
            let out = link.on_frame(&a, "t");
            assert!(
                out.iter()
                    .all(|o| !matches!(o, LinkOut::Reading { .. } | LinkOut::Send { .. }))
            );
        }
        assert_eq!(link.callouts().map(<[String]>::len), Some(3));
        let mut p = shot_with_pick(2);
        p[17] ^= 1; // a different timestamp: not a repeat of the first shot
        assert_eq!(
            reading(link.on_frame(&p, "t")).callout.as_deref(),
            Some("Water crawl")
        );
        let mut q = shot_with_pick(9); // past the end of this list
        q[17] ^= 2;
        assert_eq!(
            reading(link.on_frame(&q, "t")).callout.as_deref(),
            Some("Feature 9")
        );
        // a shot with no pick has none
        let mut plain = encode_packet(&PacketSpec::default());
        plain[17] ^= 4;
        assert_eq!(reading(link.on_frame(&plain, "t")).callout, None);
        // asking again forgets the list (a reconnect to another X1)
        let _ = link.list_request();
        assert_eq!(link.callouts(), None);
    }
}
