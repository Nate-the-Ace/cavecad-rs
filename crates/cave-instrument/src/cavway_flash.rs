//! Flashing the Cavway X1: its update file, the bootloader's three-step protocol, and a flasher that drives it.
//!
//! Sans-IO like the rest of this crate: [`Flasher`] says what to write and how long to wait, the host (a Mac
//! serial port, the browser's Bluetooth, an iPhone) moves the bytes and keeps the clock. Nothing here touches a
//! device, so every step can be run against [`SimBootloader`].
//!
//! Written from a disassembly of the X1's own bootloader (version 0x0A; see tools/cave-x1/FIRMWARE.md), which is
//! the authority on what it accepts. The bootloader listens on the USB cable and on Bluetooth alike, in boot mode
//! only (hold both arrow keys while pressing DIST). It never erases itself, and it replaces the app only after the
//! whole image has arrived and checks out, so a flash that stops part way leaves the old firmware in place:
//!
//! 1. `K`: erase the staging area (where stored shots live: they are lost). Reply `4B 01`.
//! 2. `L` index, 128 bytes, CRC-16/Modbus: write one packet into staging. Reply `4C` index, status, CRC.
//! 3. `M` sum of all packet CRCs: on a match, erase the app and copy staging over it. Reply `4D 00` (done) or
//!    `4D 01` (sum wrong; the app was not touched).

use crate::cavway::frame;

/// The first 8 bytes of every X1 update file.
pub const IMAGE_ID: [u8; 8] = [0x11, 0x23, 0x55, 0x6e, 0x7c, 0xef, 0x6d, 0x5b];
/// The update file's header (ID, build time, version); it is never sent to the X1.
pub const HEADER_BYTES: usize = 256;
/// Bytes of firmware in one packet.
pub const PACKET_DATA: usize = 128;
/// Where the app starts in the X1's flash: byte 256 of the file lands here.
pub const APP_BASE: u32 = 0x0800_4000;
/// The most firmware the app area holds: up to the app's own marker word at 0x0803FF00.
pub const MAX_PAYLOAD: usize = 0x3_bf00;

/// The bootloader's commands.
pub mod cmd {
    /// Erase staging, ready for packets.
    pub const START: u8 = 0x4b;
    /// One packet.
    pub const PACKET: u8 = 0x4c;
    /// All sent: check and install.
    pub const END: u8 = 0x4d;
    /// Bootloader version (also a "who is there" that the app ignores).
    pub const HELLO: u8 = 0x4a;
    /// Power off (in boot mode and in the app alike).
    pub const POWER_OFF: u8 = 0x34;
}

/// CRC-16/Modbus (poly 0xA001 reflected, start 0xFFFF): what the bootloader checks each packet with.
pub fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xffff;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xa001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// Why a file is not something to flash.
#[derive(Clone, Debug, PartialEq)]
pub enum ImageError {
    /// Shorter than a header plus a little code.
    TooShort(usize),
    /// The first 8 bytes are not the X1's update-file ID.
    NotX1Image,
    /// More firmware than the app area holds: flashing it would overwrite staging mid-copy.
    TooBig(usize),
    /// The part after the header does not start like the X1's app (version block, then the startup code).
    NotAnApp,
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageError::TooShort(n) => write!(f, "too short to be X1 firmware ({n} bytes)"),
            ImageError::NotX1Image => {
                write!(f, "not a Cavway X1 update file (its first bytes are wrong)")
            }
            ImageError::TooBig(n) => {
                write!(
                    f,
                    "{n} bytes of firmware, more than the X1's app area holds ({MAX_PAYLOAD})"
                )
            }
            ImageError::NotAnApp => write!(
                f,
                "the file has the X1 header, but what follows is not X1 app code"
            ),
        }
    }
}

impl std::error::Error for ImageError {}

/// An X1 update file, checked.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    /// Version from the header (major, minor, patch).
    pub version: [u8; 3],
    /// Build time from the header, Unix seconds.
    pub built: u32,
    /// The firmware itself (everything after the header), as it lands at [`APP_BASE`].
    pub payload: Vec<u8>,
    /// Things worth a second look that do not stop a flash.
    pub warnings: Vec<String>,
}

impl Image {
    /// Checks an update file: the ID, the size, and that the firmware starts the way the X1's app does.
    pub fn parse(bytes: &[u8]) -> Result<Image, ImageError> {
        if bytes.len() < HEADER_BYTES + 0x40 {
            return Err(ImageError::TooShort(bytes.len()));
        }
        if bytes[..8] != IMAGE_ID {
            return Err(ImageError::NotX1Image);
        }
        let payload = bytes[HEADER_BYTES..].to_vec();
        if payload.len() > MAX_PAYLOAD {
            return Err(ImageError::TooBig(payload.len()));
        }
        // The app opens with a 32-byte version block, then ARMCC's startup: `ldr.w sp, [pc, #12]`.
        if payload[0x20..0x24] != [0xdf, 0xf8, 0x0c, 0xd0] {
            return Err(ImageError::NotAnApp);
        }
        let version = [bytes[12], bytes[13], bytes[14]];
        let built = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let mut warnings = Vec::new();
        if payload[..3] != version {
            warnings.push(format!(
                "the header says {}, the firmware inside says {}",
                version_text(version),
                version_text([payload[0], payload[1], payload[2]])
            ));
        }
        Ok(Image {
            version,
            built,
            payload,
            warnings,
        })
    }

    /// "1.1.4", or "1.1.7-cc1" for CaveCAD firmware.
    pub fn version_text(&self) -> String {
        if self.payload[..3] == self.version {
            build_name(&self.payload)
        } else {
            version_text(self.version)
        }
    }

    /// How many packets the firmware takes.
    pub fn packets(&self) -> usize {
        self.payload.len().div_ceil(PACKET_DATA)
    }

    /// Packet `i`'s 128 bytes; the last is padded with 0xFF (erased flash).
    pub fn packet(&self, i: usize) -> [u8; PACKET_DATA] {
        let mut out = [0xff; PACKET_DATA];
        let from = i * PACKET_DATA;
        let to = (from + PACKET_DATA).min(self.payload.len());
        if from < to {
            out[..to - from].copy_from_slice(&self.payload[from..to]);
        }
        out
    }
}

/// "1.1.4".
pub fn version_text(v: [u8; 3]) -> String {
    format!("{}.{}.{}", v[0], v[1], v[2])
}

/// The firmware's name from the first 8 bytes of the app (its version block): "1.1.7", or "1.1.7-cc1" for CaveCAD
/// firmware, which marks itself with `CC 20 nn 20` after the version (harmless Thumb: `movs r0,#0xcc; movs r0,#nn`).
pub fn build_name(block: &[u8]) -> String {
    let base = version_text([block[0], block[1], block[2]]);
    match block.get(4..8) {
        Some([0xcc, 0x20, n, 0x20]) => format!("{base}-cc{n}"),
        _ => base,
    }
}

/// The framed command that starts a flash.
pub fn start_frame() -> Vec<u8> {
    frame(&[cmd::START])
}

/// The framed packet `index`.
pub fn packet_frame(index: u16, data: &[u8; PACKET_DATA]) -> Vec<u8> {
    let crc = crc16_modbus(data);
    let mut p = Vec::with_capacity(3 + PACKET_DATA + 2);
    p.push(cmd::PACKET);
    p.extend_from_slice(&index.to_le_bytes());
    p.extend_from_slice(data);
    p.extend_from_slice(&crc.to_le_bytes());
    frame(&p)
}

/// The framed end, with the sum of every packet's CRC.
pub fn end_frame(sum: u32) -> Vec<u8> {
    let mut p = vec![cmd::END];
    p.extend_from_slice(&sum.to_le_bytes());
    frame(&p)
}

/// The framed "who is there": the bootloader answers with its version, the app stays silent.
pub fn hello_frame() -> Vec<u8> {
    frame(&[cmd::HELLO])
}

/// The framed power off.
pub fn power_off_frame() -> Vec<u8> {
    frame(&[cmd::POWER_OFF])
}

/// What the bootloader said. Its replies are not framed: just the bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BootReply {
    /// To `K`: staging erased (`ok`) or not.
    Start {
        /// The erase worked.
        ok: bool,
    },
    /// To `L`.
    Packet {
        /// The packet it is about.
        index: u16,
        /// 0 written, 1 the flash write failed, 2 the CRC was wrong or the index too high.
        status: u8,
        /// CRC of what is now in flash (status 0), or of what arrived.
        crc: u16,
    },
    /// To `M`: installed (`ok`), or the sum did not match and the app was left alone.
    End {
        /// Installed.
        ok: bool,
    },
    /// To `J`: the bootloader's version word.
    Hello {
        /// 0x000A on the X1s seen so far.
        version: u16,
    },
}

/// Finds the first complete bootloader reply in `buf`: the reply and how many bytes it used up (anything before
/// it is noise and included). `None` until one has fully arrived.
pub fn parse_boot_reply(buf: &[u8]) -> Option<(BootReply, usize)> {
    for (at, &b) in buf.iter().enumerate() {
        let rest = &buf[at..];
        let reply = match b {
            cmd::START if rest.len() >= 2 => (BootReply::Start { ok: rest[1] == 1 }, 2),
            cmd::PACKET if rest.len() >= 6 => (
                BootReply::Packet {
                    index: u16::from_le_bytes([rest[1], rest[2]]),
                    status: rest[3],
                    crc: u16::from_le_bytes([rest[4], rest[5]]),
                },
                6,
            ),
            cmd::END if rest.len() >= 2 => (BootReply::End { ok: rest[1] == 0 }, 2),
            cmd::HELLO if rest.len() >= 3 => (
                BootReply::Hello {
                    version: u16::from_le_bytes([rest[1], rest[2]]),
                },
                3,
            ),
            cmd::START | cmd::PACKET | cmd::END | cmd::HELLO => return None,
            _ => continue,
        };
        return Some((reply.0, at + reply.1));
    }
    None
}

/// Where a flash is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Not begun.
    Idle,
    /// Waiting for staging to be erased.
    Erasing,
    /// Sending packets.
    Sending,
    /// All sent; the X1 is checking and copying the new firmware in.
    Installing,
    /// Installed.
    Done,
    /// Stopped (see the error).
    Failed,
}

/// Why a flash stopped. Each says whether the X1's existing firmware is still intact.
#[derive(Clone, Debug, PartialEq)]
pub enum FlashError {
    /// No answer to the start: the X1 is not in boot mode, or not connected.
    NoBootloader,
    /// The X1 could not erase its staging area.
    EraseFailed,
    /// A packet was refused or kept going missing.
    Packet {
        /// Which.
        index: u16,
        /// The last status, if any arrived.
        status: Option<u8>,
    },
    /// The X1 said the checksum did not match, and kept its old firmware.
    SumRejected,
    /// Too many packets were resent after silence to work out the X1's checksum; it kept its old firmware.
    TooManyResends(usize),
    /// No answer to the final step: the copy may or may not have run.
    NoAnswerAtEnd,
}

impl FlashError {
    /// True when the firmware already on the X1 is known to be untouched.
    pub fn app_untouched(&self) -> bool {
        !matches!(self, FlashError::NoAnswerAtEnd)
    }
}

impl std::fmt::Display for FlashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlashError::NoBootloader => write!(
                f,
                "the X1 did not answer: put it in boot mode (hold both arrows, press DIST) and check the cable"
            ),
            FlashError::EraseFailed => write!(
                f,
                "the X1 could not prepare for the new firmware; its firmware is unchanged"
            ),
            FlashError::Packet { index, status } => match status {
                Some(1) => write!(
                    f,
                    "the X1 could not write packet {index}; its firmware is unchanged, start again"
                ),
                Some(s) => write!(
                    f,
                    "packet {index} was refused (status {s}); its firmware is unchanged, start again"
                ),
                None => write!(
                    f,
                    "packet {index} got no answer; its firmware is unchanged, start again"
                ),
            },
            FlashError::TooManyResends(n) => write!(
                f,
                "{n} packets had to be resent, too many to confirm the transfer; the X1's firmware is unchanged. \
                 Check the cable and start again"
            ),
            FlashError::SumRejected => {
                write!(
                    f,
                    "the X1 found the transfer incomplete and kept its old firmware; start again"
                )
            }
            FlashError::NoAnswerAtEnd => write!(
                f,
                "no answer after the final step: the X1 may have installed the firmware or not. If it will not start, \
                 enter boot mode and flash again"
            ),
        }
    }
}

impl std::error::Error for FlashError {}

/// What to do next.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Write these bytes, then wait up to `wait_ms` for an answer (feed it to [`Flasher::on_bytes`]; on no
    /// answer call [`Flasher::on_timeout`]).
    Send {
        /// The framed bytes.
        bytes: Vec<u8>,
        /// How long to wait for the answer.
        wait_ms: u32,
    },
    /// Installed.
    Done,
    /// Stopped.
    Failed(FlashError),
}

/// How long the bootloader gets to erase 256 KB of staging (two 128 KB sectors take about 2 to 4 s).
pub const ERASE_WAIT_MS: u32 = 12_000;
/// How long for one packet's answer.
pub const PACKET_WAIT_MS: u32 = 2_500;
/// How long to check and copy the app in (erasing five sectors, then up to 240 KB of copying).
pub const INSTALL_WAIT_MS: u32 = 20_000;
/// How often a packet is sent again after a lost or garbled answer.
pub const PACKET_TRIES: u8 = 3;
/// Packets resent after silence whose double counting is tried at the end (2^n sums at most).
pub const MAX_UNSURE: usize = 4;

/// Drives one flash. Call [`Flasher::begin`], then for each [`Step::Send`] write the bytes and pass back what
/// arrives ([`Flasher::on_bytes`]) or that nothing did ([`Flasher::on_timeout`]). It never sends the final
/// install step unless every packet was confirmed.
#[derive(Clone, Debug)]
pub struct Flasher {
    image: Image,
    phase: Phase,
    next: usize,
    tries: u8,
    sum: u32,
    rx: Vec<u8>,
    /// The current packet was resent after silence: the X1 may have written (and counted) the first copy.
    resent_after_silence: bool,
    /// CRCs of packets that may be counted twice by the X1, and how many resends there were in all.
    unsure: Vec<u16>,
    resends: usize,
    /// The sums still to offer at the end, most likely first.
    sums: Vec<u32>,
}

impl Flasher {
    /// A flash of `image`.
    pub fn new(image: Image) -> Flasher {
        Flasher {
            image,
            phase: Phase::Idle,
            next: 0,
            tries: 0,
            sum: 0,
            rx: Vec::new(),
            resent_after_silence: false,
            unsure: Vec::new(),
            resends: 0,
            sums: Vec::new(),
        }
    }

    /// The first step: erase staging.
    pub fn begin(&mut self) -> Step {
        self.phase = Phase::Erasing;
        self.rx.clear();
        Step::Send {
            bytes: start_frame(),
            wait_ms: ERASE_WAIT_MS,
        }
    }

    /// Where it is.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Packets confirmed, and how many there are.
    pub fn progress(&self) -> (usize, usize) {
        (self.next, self.image.packets())
    }

    /// The image being flashed.
    pub fn image(&self) -> &Image {
        &self.image
    }

    /// Packets sent more than once so far.
    pub fn resends(&self) -> usize {
        self.resends
    }

    /// The bootloader adds the CRC of every packet it writes, so a packet resent because its answer was lost
    /// is counted twice, while one resent because it never arrived is counted once; from here there is no telling
    /// which. Every combination is offered (all twice first: a lost answer is the usual case). A wrong sum is
    /// refused before anything is erased, so trying costs nothing.
    fn end_sums(&self) -> Vec<u32> {
        let n = self.unsure.len();
        let mut subsets: Vec<u32> = (0..1u32 << n).collect();
        subsets.sort_by_key(|m| std::cmp::Reverse(m.count_ones()));
        subsets
            .into_iter()
            .map(|m| {
                (0..n)
                    .filter(|i| m & (1 << i) != 0)
                    .fold(self.sum, |acc, i| {
                        acc.wrapping_add(u32::from(self.unsure[i]))
                    })
            })
            .collect()
    }

    fn send_packet(&mut self) -> Step {
        self.rx.clear();
        let i = self.next;
        Step::Send {
            bytes: packet_frame(i as u16, &self.image.packet(i)),
            wait_ms: PACKET_WAIT_MS,
        }
    }

    fn fail(&mut self, e: FlashError) -> Step {
        self.phase = Phase::Failed;
        Step::Failed(e)
    }

    /// Bytes arrived. Returns the next step once a whole answer is in, `None` while waiting for more.
    pub fn on_bytes(&mut self, bytes: &[u8]) -> Option<Step> {
        self.rx.extend_from_slice(bytes);
        loop {
            let (reply, used) = parse_boot_reply(&self.rx)?;
            self.rx.drain(..used);
            match (self.phase, reply) {
                (Phase::Erasing, BootReply::Start { ok: true }) => {
                    self.phase = Phase::Sending;
                    self.next = 0;
                    self.sum = 0;
                    self.tries = 0;
                    self.unsure.clear();
                    self.resends = 0;
                    self.resent_after_silence = false;
                    return Some(self.send_packet());
                }
                (Phase::Erasing, BootReply::Start { ok: false }) => {
                    return Some(self.fail(FlashError::EraseFailed));
                }
                (Phase::Sending, BootReply::Packet { index, status, crc })
                    if usize::from(index) == self.next =>
                {
                    let want = crc16_modbus(&self.image.packet(self.next));
                    if status == 0 && crc == want {
                        self.sum = self.sum.wrapping_add(u32::from(want));
                        if self.resent_after_silence {
                            self.unsure.push(want);
                            self.resent_after_silence = false;
                        }
                        self.next += 1;
                        self.tries = 0;
                        if self.next == self.image.packets() {
                            if self.unsure.len() > MAX_UNSURE {
                                return Some(
                                    self.fail(FlashError::TooManyResends(self.unsure.len())),
                                );
                            }
                            self.phase = Phase::Installing;
                            self.rx.clear();
                            self.sums = self.end_sums();
                            let first = self.sums.remove(0);
                            return Some(Step::Send {
                                bytes: end_frame(first),
                                wait_ms: INSTALL_WAIT_MS,
                            });
                        }
                        return Some(self.send_packet());
                    }
                    // 2 = it arrived damaged and nothing was written: safe to send again. 1 = a write failed: stop.
                    if status == 2 && self.tries + 1 < PACKET_TRIES {
                        self.tries += 1;
                        self.resends += 1;
                        return Some(self.send_packet());
                    }
                    return Some(self.fail(FlashError::Packet {
                        index,
                        status: Some(status),
                    }));
                }
                (Phase::Installing, BootReply::End { ok: true }) => {
                    self.phase = Phase::Done;
                    return Some(Step::Done);
                }
                (Phase::Installing, BootReply::End { ok: false }) => {
                    if self.sums.is_empty() {
                        return Some(self.fail(FlashError::SumRejected));
                    }
                    let next = self.sums.remove(0);
                    self.rx.clear();
                    return Some(Step::Send {
                        bytes: end_frame(next),
                        wait_ms: INSTALL_WAIT_MS,
                    });
                }
                // a stale or unrelated answer: keep listening
                _ => continue,
            }
        }
    }

    /// Nothing arrived in time.
    pub fn on_timeout(&mut self) -> Step {
        match self.phase {
            Phase::Erasing => self.fail(FlashError::NoBootloader),
            Phase::Sending if self.tries + 1 < PACKET_TRIES => {
                self.tries += 1;
                self.resends += 1;
                self.resent_after_silence = true;
                self.send_packet()
            }
            Phase::Sending => {
                let index = self.next as u16;
                self.fail(FlashError::Packet {
                    index,
                    status: None,
                })
            }
            Phase::Installing => self.fail(FlashError::NoAnswerAtEnd),
            Phase::Idle | Phase::Done | Phase::Failed => Step::Failed(FlashError::NoBootloader),
        }
    }
}

/// A pretend X1 in boot mode, built from the disassembly of the real bootloader: for tests and dry runs.
/// Feed it whatever would be written to the cable and it returns what the X1 would answer.
#[derive(Clone, Debug)]
pub struct SimBootloader {
    /// The app area as it stands (starts as the "old" firmware).
    pub app: Vec<u8>,
    staging: Vec<Option<u8>>,
    highest: Option<usize>,
    rx: Vec<u8>,
    /// Packets whose answer is dropped once (as a lost reply on a bad link).
    pub drop_answer_to: Vec<u16>,
    /// Packets that arrive damaged once (the X1 answers status 2).
    pub damage: Vec<u16>,
    /// Packets that never arrive (lost on the way in) once.
    pub lose: Vec<u16>,
    /// Powered off by a `4` command.
    pub off: bool,
    erased: bool,
    /// As the real bootloader keeps it: the CRC of every packet written, added up (rewrites count again).
    counted: u32,
}

impl SimBootloader {
    /// A bootloader in front of `app`.
    pub fn new(app: Vec<u8>) -> SimBootloader {
        SimBootloader {
            app,
            staging: vec![None; 0x4_0000],
            highest: None,
            rx: Vec::new(),
            drop_answer_to: Vec::new(),
            damage: Vec::new(),
            lose: Vec::new(),
            off: false,
            erased: false,
            counted: 0,
        }
    }

    /// Bytes from the host; returns the answer bytes.
    pub fn receive(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.rx.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some((payload, used)) = take_frame(&self.rx) {
            self.rx.drain(..used);
            out.extend(self.answer(&payload));
        }
        out
    }

    fn answer(&mut self, p: &[u8]) -> Vec<u8> {
        match (p.len(), p.first().copied()) {
            (1, Some(cmd::START)) => {
                self.staging.iter_mut().for_each(|b| *b = Some(0xff));
                self.highest = None;
                self.erased = true;
                self.counted = 0;
                vec![cmd::START, 1]
            }
            (1, Some(cmd::HELLO)) => vec![cmd::HELLO, 0x0a, 0x00],
            (1, Some(cmd::POWER_OFF)) => {
                self.off = true;
                Vec::new()
            }
            (133, Some(cmd::PACKET)) => {
                let index = u16::from_le_bytes([p[1], p[2]]);
                if let Some(at) = self.lose.iter().position(|&d| d == index) {
                    self.lose.remove(at);
                    return Vec::new();
                }
                let mut data: Vec<u8> = p[3..131].to_vec();
                if let Some(at) = self.damage.iter().position(|&d| d == index) {
                    self.damage.remove(at);
                    data[7] ^= 0x40;
                }
                let sent_crc = u16::from_le_bytes([p[131], p[132]]);
                let crc = crc16_modbus(&data);
                let at = usize::from(index) * PACKET_DATA;
                let status = if crc != sent_crc || at + PACKET_DATA > self.staging.len() {
                    2
                } else if !self.erased
                    || self.staging[at..at + PACKET_DATA]
                        .iter()
                        .zip(&data)
                        .any(|(s, d)| *s != Some(0xff) && *s != Some(*d))
                {
                    1
                } else {
                    for (i, d) in data.iter().enumerate() {
                        self.staging[at + i] = Some(*d);
                    }
                    self.highest = Some(self.highest.map_or(at, |h| h.max(at)));
                    self.counted = self.counted.wrapping_add(u32::from(crc));
                    0
                };
                if let Some(at) = self.drop_answer_to.iter().position(|&d| d == index) {
                    self.drop_answer_to.remove(at);
                    return Vec::new();
                }
                let mut r = vec![cmd::PACKET];
                r.extend_from_slice(&index.to_le_bytes());
                r.push(status);
                r.extend_from_slice(&crc.to_le_bytes());
                r
            }
            (5, Some(cmd::END)) => {
                let want = u32::from_le_bytes([p[1], p[2], p[3], p[4]]);
                let Some(highest) = self.highest else {
                    return vec![cmd::END, 1];
                };
                if self.counted != want || self.counted == 0 {
                    return vec![cmd::END, 1];
                }
                // the real bootloader copies whole kilobytes up to the highest packet written
                let end = (highest + PACKET_DATA).div_ceil(0x400) * 0x400;
                self.app = self.staging[..end]
                    .iter()
                    .map(|b| b.unwrap_or(0xff))
                    .collect();
                vec![cmd::END, 0]
            }
            _ => Vec::new(),
        }
    }
}

/// Pulls one `data:` frame out of `buf`: its payload and the bytes used.
fn take_frame(buf: &[u8]) -> Option<(Vec<u8>, usize)> {
    let at = buf.windows(5).position(|w| w == b"data:")?;
    let len = usize::from(*buf.get(at + 5)?);
    let end = at + 6 + len + 2;
    if buf.len() < end {
        return None;
    }
    Some((buf[at + 6..at + 6 + len].to_vec(), end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(len: usize, version: [u8; 3]) -> Vec<u8> {
        let mut f = vec![0u8; HEADER_BYTES];
        f[..8].copy_from_slice(&IMAGE_ID);
        f[8..12].copy_from_slice(&0x6972_479fu32.to_be_bytes());
        f[12..15].copy_from_slice(&version);
        let mut p: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
        p[..4].copy_from_slice(&[version[0], version[1], version[2], 0]);
        p[0x20..0x24].copy_from_slice(&[0xdf, 0xf8, 0x0c, 0xd0]);
        f.extend(p);
        f
    }

    fn run(f: &mut Flasher, x1: &mut SimBootloader) -> Step {
        let mut step = f.begin();
        for _ in 0..100_000 {
            match step {
                Step::Send { bytes, .. } => {
                    let answer = x1.receive(&bytes);
                    step = if answer.is_empty() {
                        f.on_timeout()
                    } else {
                        // answers can arrive in pieces
                        let (a, b) = answer.split_at(answer.len() / 2);
                        match f.on_bytes(a) {
                            Some(s) => s,
                            None => f.on_bytes(b).unwrap_or_else(|| f.on_timeout()),
                        }
                    };
                }
                done => return done,
            }
        }
        panic!("never finished");
    }

    #[test]
    fn cc_firmware_names_itself() {
        assert_eq!(build_name(&[1, 1, 7, 0, 0, 0, 0, 0]), "1.1.7");
        assert_eq!(build_name(&[1, 1, 7, 0, 0xcc, 0x20, 1, 0x20]), "1.1.7-cc1");
    }

    #[test]
    fn crc_matches_the_modbus_check_value() {
        assert_eq!(crc16_modbus(b"123456789"), 0x4b37);
    }

    #[test]
    fn the_real_1_1_4_file_reads_as_1_1_4() {
        let path = concat!(
            env!("HOME"),
            "/Documents/github/cavway-upstream/CavwayAssist/Resources/CVWY_X1_Firm_1_1_4.bin"
        );
        let Ok(bytes) = std::fs::read(path) else {
            return;
        }; // not on every machine
        let img = Image::parse(&bytes).unwrap();
        assert_eq!(img.version_text(), "1.1.4");
        assert_eq!(img.payload.len(), 0x17318);
        assert_eq!(img.packets(), 743);
        assert!(img.warnings.is_empty());
    }

    #[test]
    fn files_that_are_not_x1_firmware_are_refused() {
        assert_eq!(Image::parse(&[0; 100]), Err(ImageError::TooShort(100)));
        let mut f = image(4096, [1, 1, 4]);
        f[0] = 0;
        assert_eq!(Image::parse(&f), Err(ImageError::NotX1Image));
        let mut f = image(4096, [1, 1, 4]);
        f[HEADER_BYTES + 0x20] = 0;
        assert_eq!(Image::parse(&f), Err(ImageError::NotAnApp));
        assert_eq!(
            Image::parse(&image(MAX_PAYLOAD + 1, [1, 1, 4])),
            Err(ImageError::TooBig(MAX_PAYLOAD + 1))
        );
        assert!(Image::parse(&image(MAX_PAYLOAD, [1, 1, 4])).is_ok());
    }

    #[test]
    fn a_mismatched_version_block_is_a_warning() {
        let mut f = image(4096, [1, 1, 4]);
        f[HEADER_BYTES + 2] = 9;
        assert_eq!(Image::parse(&f).unwrap().warnings.len(), 1);
    }

    #[test]
    fn last_packet_is_padded_with_erased_flash() {
        let img = Image::parse(&image(200, [1, 1, 4])).unwrap();
        assert_eq!(img.packets(), 2);
        assert_eq!(img.packet(1)[72..], [0xff; 56]);
        assert_eq!(img.packet(1)[..72], img.payload[128..]);
    }

    #[test]
    fn packet_frame_is_what_the_bootloader_parses() {
        let data = [0x5a; PACKET_DATA];
        let f = packet_frame(0x0102, &data);
        assert_eq!(&f[..6], b"data:\x85");
        assert_eq!(&f[6..9], &[0x4c, 0x02, 0x01]);
        assert_eq!(&f[137..139], &crc16_modbus(&data).to_le_bytes());
        assert_eq!(&f[139..], b"\r\n");
    }

    #[test]
    fn a_clean_flash_installs_the_new_firmware() {
        let img = Image::parse(&image(10_000, [1, 1, 7])).unwrap();
        let mut x1 = SimBootloader::new(vec![0xaa; 5000]);
        let mut f = Flasher::new(img.clone());
        assert_eq!(run(&mut f, &mut x1), Step::Done);
        assert_eq!(&x1.app[..img.payload.len()], &img.payload[..]);
        assert_eq!(f.progress(), (79, 79));
    }

    #[test]
    fn lost_and_damaged_packets_are_resent() {
        let img = Image::parse(&image(10_000, [1, 1, 7])).unwrap();
        let mut x1 = SimBootloader::new(vec![0xaa; 5000]);
        x1.damage = vec![3, 40];
        x1.drop_answer_to = vec![5];
        let mut f = Flasher::new(img.clone());
        // a dropped answer means the packet WAS written; resending identical bytes is accepted
        assert_eq!(run(&mut f, &mut x1), Step::Done);
        assert_eq!(&x1.app[..img.payload.len()], &img.payload[..]);
    }

    #[test]
    fn a_packet_lost_on_the_way_in_is_counted_once() {
        let img = Image::parse(&image(10_000, [1, 1, 7])).unwrap();
        let mut x1 = SimBootloader::new(vec![0xaa; 5000]);
        x1.lose = vec![12];
        x1.drop_answer_to = vec![30, 31];
        let mut f = Flasher::new(img.clone());
        assert_eq!(run(&mut f, &mut x1), Step::Done);
        assert_eq!(&x1.app[..img.payload.len()], &img.payload[..]);
        assert_eq!(f.resends(), 3);
    }

    #[test]
    fn too_many_silent_resends_stop_before_install() {
        let img = Image::parse(&image(10_000, [1, 1, 7])).unwrap();
        let old = vec![0xaa; 5000];
        let mut x1 = SimBootloader::new(old.clone());
        x1.drop_answer_to = (1..=6).collect();
        let mut f = Flasher::new(img);
        assert_eq!(
            run(&mut f, &mut x1),
            Step::Failed(FlashError::TooManyResends(6))
        );
        assert_eq!(x1.app, old);
    }

    #[test]
    fn a_packet_that_keeps_failing_stops_before_the_app_is_touched() {
        let img = Image::parse(&image(10_000, [1, 1, 7])).unwrap();
        let old = vec![0xaa; 5000];
        let mut x1 = SimBootloader::new(old.clone());
        x1.damage = vec![9, 9, 9];
        let mut f = Flasher::new(img);
        let Step::Failed(e) = run(&mut f, &mut x1) else {
            panic!()
        };
        assert_eq!(
            e,
            FlashError::Packet {
                index: 9,
                status: Some(2)
            }
        );
        assert!(e.app_untouched());
        assert_eq!(x1.app, old);
    }

    #[test]
    fn silence_at_the_start_means_not_in_boot_mode() {
        let img = Image::parse(&image(1000, [1, 1, 7])).unwrap();
        let mut f = Flasher::new(img);
        f.begin();
        assert_eq!(f.on_timeout(), Step::Failed(FlashError::NoBootloader));
    }

    #[test]
    fn replies_are_found_among_noise_and_in_pieces() {
        assert_eq!(parse_boot_reply(&[0x00, 0x4b]), None);
        assert_eq!(
            parse_boot_reply(&[0x00, 0x4b, 0x01]),
            Some((BootReply::Start { ok: true }, 3))
        );
        assert_eq!(
            parse_boot_reply(&[0x4a, 0x0a, 0x00]),
            Some((BootReply::Hello { version: 10 }, 3))
        );
        assert_eq!(
            parse_boot_reply(&[0x4c, 0x05, 0x00, 0x00, 0x34, 0x12]),
            Some((
                BootReply::Packet {
                    index: 5,
                    status: 0,
                    crc: 0x1234
                },
                6
            ))
        );
    }
}
