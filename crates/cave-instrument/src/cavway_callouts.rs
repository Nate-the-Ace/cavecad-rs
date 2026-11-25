//! The feature list inside an X1 firmware file (`1.1.7-cc7` and later): read it, or rewrite it and get a file to flash.
//!
//! The X1's feature picker (hold `<` with the laser on) shows a list kept as plain data in the firmware: a block that
//! starts with `CCAL`, a count (0 to 32) and 32 name slots of 13 bytes (up to 12 ASCII characters and a 0). Changing
//! the names is a change of those bytes; the update file has no checksum of its own (the uploader computes the packet
//! CRCs from the file it is given), so the result flashes like any other. See `tools/x1-patch/cc.s` (`callout_block`).

/// Most names the firmware's list holds.
pub const MAX_NAMES: usize = 32;
/// Longest name, in characters.
pub const MAX_NAME_LEN: usize = 12;

const MAGIC: &[u8; 4] = b"CCAL";
const SLOT: usize = MAX_NAME_LEN + 1;
const HEAD: usize = 8;
const BLOCK: usize = HEAD + SLOT * MAX_NAMES;

/// Why a list could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalloutError {
    /// The file has no feature list: it is not a `-cc7` or later build.
    NoList,
    /// The list was found more than once, or its layout is damaged.
    Damaged(String),
    /// More than [`MAX_NAMES`] names.
    TooMany(usize),
    /// A name that is empty, too long, or uses a character the X1's screen font may not have.
    BadName(String, &'static str),
}

impl std::fmt::Display for CalloutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CalloutError::NoList => write!(
                f,
                "this firmware has no feature list (it needs a 1.1.7-cc7 or later build)"
            ),
            CalloutError::Damaged(m) => write!(f, "the feature list in this file is damaged: {m}"),
            CalloutError::TooMany(n) => write!(f, "{n} names; the X1 holds at most {MAX_NAMES}"),
            CalloutError::BadName(n, why) => write!(f, "\"{n}\": {why}"),
        }
    }
}

impl std::error::Error for CalloutError {}

fn locate(image: &[u8]) -> Result<usize, CalloutError> {
    let mut hits = image
        .windows(MAGIC.len())
        .enumerate()
        .filter(|(_, w)| *w == MAGIC)
        .map(|(i, _)| i);
    let at = hits.next().ok_or(CalloutError::NoList)?;
    if hits.next().is_some() {
        return Err(CalloutError::Damaged(
            "the CCAL marker appears more than once".into(),
        ));
    }
    if at + BLOCK > image.len() {
        return Err(CalloutError::Damaged(
            "the list runs past the end of the file".into(),
        ));
    }
    Ok(at)
}

/// The names in a firmware file's feature list, in the order the X1 shows them.
pub fn callouts(image: &[u8]) -> Result<Vec<String>, CalloutError> {
    let at = locate(image)?;
    let n = usize::from(image[at + 4]);
    if n > MAX_NAMES {
        return Err(CalloutError::Damaged(format!("count {n}")));
    }
    (0..n)
        .map(|i| {
            let slot = &image[at + HEAD + i * SLOT..at + HEAD + (i + 1) * SLOT];
            let end = slot
                .iter()
                .position(|&b| b == 0)
                .ok_or_else(|| CalloutError::Damaged(format!("name {} has no end", i + 1)))?;
            Ok(String::from_utf8_lossy(&slot[..end]).into_owned())
        })
        .collect()
}

/// Checks a list: 1 to [`MAX_NAMES`] names of 1 to [`MAX_NAME_LEN`] printable ASCII characters (no leading or trailing
/// space), none repeated ignoring case. An empty list is allowed (the picker then does not open).
pub fn check(names: &[String]) -> Result<(), CalloutError> {
    if names.len() > MAX_NAMES {
        return Err(CalloutError::TooMany(names.len()));
    }
    for (i, n) in names.iter().enumerate() {
        if n.is_empty() {
            return Err(CalloutError::BadName(n.clone(), "a name is empty"));
        }
        if n.len() > MAX_NAME_LEN {
            return Err(CalloutError::BadName(
                n.clone(),
                "longer than 12 characters",
            ));
        }
        if !n.bytes().all(|b| (0x20..0x7f).contains(&b)) {
            return Err(CalloutError::BadName(
                n.clone(),
                "only plain keyboard characters (letters, digits, punctuation)",
            ));
        }
        if n.starts_with(' ') || n.ends_with(' ') {
            return Err(CalloutError::BadName(
                n.clone(),
                "starts or ends with a space",
            ));
        }
        if names[..i].iter().any(|m| m.eq_ignore_ascii_case(n)) {
            return Err(CalloutError::BadName(
                n.clone(),
                "appears twice (names are compared ignoring case)",
            ));
        }
    }
    Ok(())
}

/// Where a computer writes a list into a running X1 (`1.1.7-cc8` and later), with the stock memory-write: the RAM buffer
/// for the list at 0xC000 to 0xC1FF, then a write of `C0` to the commit address, then a read of the result there. The
/// firmware checks the list's CRC and programs it into the next free slot of a spare flash sector: no reflash, and the
/// stored shots stay. See `tools/x1-patch/cc.s` (`cc_commit`).
pub const BUFFER_ADDRESS: u16 = 0xC000;
/// The address a `C0` is written to, to commit; and where the result (4 bytes) is read from.
pub const COMMIT_ADDRESS: u16 = 0xC200;
/// The first cc build that takes a pushed list.
pub const PUSH_MIN_CC_VERSION: u8 = 8;
/// Bytes per write. The stock write handler takes up to 124, but how long a frame the X1's receiver accepts is unknown,
/// so these are small (a frame is 5 + 1 + 4 + 32 + 2 bytes, sent in 20-byte pieces over Bluetooth).
const CHUNK: usize = 32;

/// The 426 bytes a list is stored as: the block (`CCAL`, count, names) and its CRC-16/Modbus, little-endian.
pub fn slot_bytes(names: &[String]) -> Result<Vec<u8>, CalloutError> {
    check(names)?;
    let mut b = vec![0u8; BLOCK];
    b[..4].copy_from_slice(MAGIC);
    b[4] = names.len() as u8;
    for (i, n) in names.iter().enumerate() {
        let s = HEAD + i * SLOT;
        b[s..s + n.len()].copy_from_slice(n.as_bytes());
    }
    let crc = crate::cavway_flash::crc16_modbus(&b);
    b.extend_from_slice(&crc.to_le_bytes());
    Ok(b)
}

/// The frames that push a list to a running X1, in order: the list in 32-byte writes, then the commit. Send each and wait
/// for its answer; then read [`COMMIT_ADDRESS`] (4 bytes) and decode it with [`PushStatus::parse`].
pub fn push_frames(names: &[String]) -> Result<Vec<Vec<u8>>, CalloutError> {
    let slot = slot_bytes(names)?;
    let mut frames: Vec<Vec<u8>> = slot
        .chunks(CHUNK)
        .enumerate()
        .map(|(i, part)| write_frame(BUFFER_ADDRESS + (i * CHUNK) as u16, part))
        .collect();
    frames.push(write_frame(COMMIT_ADDRESS, &[0xC0, 0, 0, 0]));
    Ok(frames)
}

/// A memory write (`3e`, address, count, bytes). The firmware's handler needs at least 4 bytes in the frame body.
fn write_frame(address: u16, data: &[u8]) -> Vec<u8> {
    let mut p = vec![
        0x3e,
        (address & 0xff) as u8,
        (address >> 8) as u8,
        data.len() as u8,
    ];
    p.extend_from_slice(data);
    crate::cavway::frame(&p)
}

/// What the X1 says after a commit (4 bytes read from [`COMMIT_ADDRESS`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushStatus {
    /// 0 stored; 2 not a valid list; 3 no room left; 4 the slot was not erased; 5 the flash write failed; 0xFF not finished.
    pub result: u8,
    /// The slot it went to (0 to 254).
    pub slot: u8,
    /// Names in the list now in use.
    pub count: u8,
    /// The cc firmware build.
    pub cc_version: u8,
}

impl PushStatus {
    /// Reads the 4 status bytes.
    pub fn parse(b: &[u8]) -> Option<PushStatus> {
        (b.len() >= 4).then(|| PushStatus {
            result: b[0],
            slot: b[1],
            count: b[2],
            cc_version: b[3],
        })
    }

    /// A sentence for the outcome.
    pub fn message(&self) -> String {
        match self.result {
            0 => format!("stored in slot {} ({} names)", self.slot, self.count),
            2 => "the X1 did not accept the list (it failed its own check; try again)".into(),
            3 => "the X1's list storage is full (255 updates); reflash the firmware to clear it"
                .into(),
            4 => "the next storage slot was not empty; reflash the firmware to clear it".into(),
            5 => "the X1 could not write its flash; nothing was changed".into(),
            0xff => "the X1 did not finish (is it running firmware 1.1.7-cc8 or later?)".into(),
            n => format!("unexpected result {n}"),
        }
    }
}

/// A copy of `image` with its feature list replaced by `names` (checked first). Nothing else changes.
pub fn set_callouts(image: &[u8], names: &[String]) -> Result<Vec<u8>, CalloutError> {
    check(names)?;
    let at = locate(image)?;
    let mut out = image.to_vec();
    out[at + 4] = names.len() as u8;
    out[at + HEAD..at + BLOCK].fill(0);
    for (i, n) in names.iter().enumerate() {
        let s = at + HEAD + i * SLOT;
        out[s..s + n.len()].copy_from_slice(n.as_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(names: &[&str]) -> Vec<u8> {
        let mut v = vec![0xffu8; 300];
        let mut block = vec![0u8; BLOCK];
        block[..4].copy_from_slice(MAGIC);
        block[4] = names.len() as u8;
        for (i, n) in names.iter().enumerate() {
            block[HEAD + i * SLOT..HEAD + i * SLOT + n.len()].copy_from_slice(n.as_bytes());
        }
        v.extend(block);
        v.extend([0xee; 50]);
        v
    }
    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn reads_the_list() {
        assert_eq!(
            callouts(&image(&["Lead", "Drip"])).unwrap(),
            s(&["Lead", "Drip"])
        );
        assert_eq!(callouts(&image(&[])).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn rewrites_only_the_list() {
        let a = image(&["Lead", "Squeeze", "Drip"]);
        let b = set_callouts(&a, &s(&["Pit", "Twelve chars", "x"])).unwrap();
        assert_eq!(callouts(&b).unwrap(), s(&["Pit", "Twelve chars", "x"]));
        assert_eq!(b.len(), a.len());
        let at = locate(&a).unwrap();
        assert_eq!(a[..at], b[..at]);
        assert_eq!(a[at + BLOCK..], b[at + BLOCK..]);
        // shrinking leaves no remnants of the old names
        let c = set_callouts(&b, &s(&["A"])).unwrap();
        assert!(c[at + HEAD + SLOT..at + BLOCK].iter().all(|&x| x == 0));
        assert_eq!(callouts(&c).unwrap(), s(&["A"]));
    }

    #[test]
    fn rejects_bad_lists() {
        let a = image(&["Lead"]);
        let many: Vec<String> = (0..33).map(|i| format!("n{i}")).collect();
        assert_eq!(set_callouts(&a, &many), Err(CalloutError::TooMany(33)));
        assert!(matches!(
            set_callouts(&a, &s(&[""])),
            Err(CalloutError::BadName(..))
        ));
        assert!(matches!(
            set_callouts(&a, &s(&["Thirteen char"])),
            Err(CalloutError::BadName(..))
        ));
        assert!(matches!(
            set_callouts(&a, &s(&["Café"])),
            Err(CalloutError::BadName(..))
        ));
        assert!(matches!(
            set_callouts(&a, &s(&[" x"])),
            Err(CalloutError::BadName(..))
        ));
        assert!(matches!(
            set_callouts(&a, &s(&["Drip", "drip"])),
            Err(CalloutError::BadName(..))
        ));
        let max: Vec<String> = (0..32).map(|i| format!("name{i}")).collect();
        assert!(set_callouts(&a, &max).is_ok());
    }

    #[test]
    fn needs_one_list() {
        assert_eq!(callouts(&[0u8; 100]), Err(CalloutError::NoList));
        let mut two = image(&["a"]);
        two.extend(image(&["b"]));
        assert!(matches!(callouts(&two), Err(CalloutError::Damaged(_))));
        let cut = image(&["a"])[..310].to_vec();
        assert!(matches!(callouts(&cut), Err(CalloutError::Damaged(_))));
    }

    #[test]
    fn a_list_is_pushed_as_writes_then_a_commit() {
        let names = s(&["Pit", "Water crawl", "Bats"]);
        let slot = slot_bytes(&names).unwrap();
        assert_eq!(slot.len(), 426);
        assert_eq!(&slot[..5], b"CCAL\x03");
        let crc = u16::from_le_bytes([slot[424], slot[425]]);
        assert_eq!(crc, crate::cavway_flash::crc16_modbus(&slot[..424]));
        let frames = push_frames(&names).unwrap();
        assert_eq!(frames.len(), 15); // 14 writes (13 x 32 + 10) and the commit
        // the first write: data:, length 36, 3e, 00 c0, 32, the first 32 bytes, CR LF
        assert_eq!(&frames[0][..5], b"data:");
        assert_eq!(&frames[0][5..10], &[36, 0x3e, 0x00, 0xc0, 32]);
        assert_eq!(&frames[0][10..42], &slot[..32]);
        assert_eq!(&frames[13][5..10], &[4 + 10, 0x3e, 0xa0, 0xc1, 10]); // 0xC1A0, the last 10 bytes
        assert_eq!(
            frames[14],
            crate::cavway::frame(&[0x3e, 0x00, 0xc2, 4, 0xc0, 0, 0, 0])
        );
        assert!(
            frames
                .iter()
                .all(|f| f.len() <= 5 + 1 + 4 + 32 + 2 && f[5] >= 8)
        );
        assert!(push_frames(&s(&["Drip", "drip"])).is_err());
    }

    #[test]
    fn the_status_is_decoded() {
        assert_eq!(
            PushStatus::parse(&[0, 3, 7, 8]),
            Some(PushStatus {
                result: 0,
                slot: 3,
                count: 7,
                cc_version: 8
            })
        );
        assert_eq!(PushStatus::parse(&[0, 3]), None);
        assert!(
            PushStatus::parse(&[0, 3, 7, 8])
                .unwrap()
                .message()
                .contains("slot 3")
        );
        assert!(
            PushStatus::parse(&[3, 0, 0, 8])
                .unwrap()
                .message()
                .contains("full")
        );
        assert!(
            PushStatus::parse(&[0xff, 0, 0, 0])
                .unwrap()
                .message()
                .contains("cc8")
        );
    }

    #[test]
    fn the_built_firmware_list_matches_the_decoder_defaults() {
        // the list built into cc.s is the default of `cavway::CALLOUTS`
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/x1-patch/build/CVWY_X1_1.1.7-cc7.bin");
        if let Ok(bytes) = std::fs::read(p) {
            let got = callouts(&bytes).unwrap();
            assert_eq!(
                got,
                crate::cavway::CALLOUTS
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
            );
        }
    }
}
