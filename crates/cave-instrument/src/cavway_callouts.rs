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
