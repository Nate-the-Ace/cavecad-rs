//! Stable identifiers for everything the survey stores (stations, shots, entities, layers, trips).
//!
//! An [`Id`] is never a list position or a counter shared between people, so two collaborators
//! working offline can never mint the same one. Station *labels* (`A5`) are user-facing text and
//! live elsewhere; the `Id` is what every reference points at.
//!
//! This crate does no I/O and reads no clock or entropy: the caller supplies entropy through an
//! [`IdSource`], which keeps it identical on desktop and in the browser (wasm32).

use core::fmt;
use core::str::FromStr;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A 128-bit stable identifier, shown as 26 Crockford base32 characters.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(u128);

impl Id {
    /// Wraps a raw 128-bit value.
    pub const fn from_u128(v: u128) -> Self {
        Id(v)
    }

    /// The raw 128-bit value.
    pub const fn as_u128(self) -> u128 {
        self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = [0u8; 26];
        let mut v = self.0;
        for slot in out.iter_mut().rev() {
            *slot = ALPHABET[(v & 31) as usize];
            v >>= 5;
        }
        f.write_str(core::str::from_utf8(&out).map_err(|_| fmt::Error)?)
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({self})")
    }
}

/// Why a string is not a valid [`Id`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseIdError {
    /// Not exactly 26 characters.
    Length,
    /// A character outside the Crockford alphabet.
    Character,
    /// The first character would overflow 128 bits.
    Overflow,
}

impl FromStr for Id {
    type Err = ParseIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();
        if bytes.len() != 26 {
            return Err(ParseIdError::Length);
        }
        let mut v: u128 = 0;
        for (i, b) in bytes.iter().enumerate() {
            let up = b.to_ascii_uppercase();
            let d = ALPHABET
                .iter()
                .position(|&a| a == up)
                .ok_or(ParseIdError::Character)? as u128;
            if i == 0 && d > 7 {
                return Err(ParseIdError::Overflow);
            }
            v = (v << 5) | d;
        }
        Ok(Id(v))
    }
}

impl serde::Serialize for Id {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Id {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str> as serde::Deserialize>::deserialize(d)?;
        text.parse()
            .map_err(|e| serde::de::Error::custom(format!("bad id: {e:?}")))
    }
}

/// Where new ids come from. The app wires this to real entropy; tests use [`SeqIds`].
pub trait IdSource {
    /// Returns a fresh id, never one returned before by this source.
    fn next_id(&mut self) -> Id;
}

/// Deterministic ids for tests and reproducible fixtures: `(namespace, 0)`, `(namespace, 1)`, ...
#[derive(Debug, Clone)]
pub struct SeqIds {
    namespace: u64,
    next: u64,
}

impl SeqIds {
    /// A sequence in its own namespace, so two sequences never collide.
    pub fn new(namespace: u64) -> Self {
        SeqIds { namespace, next: 0 }
    }
}

impl IdSource for SeqIds {
    fn next_id(&mut self) -> Id {
        let id = Id(((self.namespace as u128) << 64) | self.next as u128);
        self.next += 1;
        id
    }
}

/// Ids built from caller-supplied random words (the browser's `crypto.getRandomValues`, the
/// OS generator on desktop). Two 64-bit words per id.
pub struct EntropyIds<F: FnMut() -> u64> {
    words: F,
}

impl<F: FnMut() -> u64> EntropyIds<F> {
    /// Wraps a source of random 64-bit words.
    pub fn new(words: F) -> Self {
        EntropyIds { words }
    }
}

impl<F: FnMut() -> u64> IdSource for EntropyIds<F> {
    fn next_id(&mut self) -> Id {
        let hi = (self.words)() as u128;
        let lo = (self.words)() as u128;
        Id((hi << 64) | lo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_parse_round_trip() {
        for v in [
            0u128,
            1,
            u128::MAX,
            0x0123_4567_89ab_cdef_fedc_ba98_7654_3210,
        ] {
            let id = Id::from_u128(v);
            assert_eq!(id.to_string().len(), 26);
            assert_eq!(id.to_string().parse::<Id>(), Ok(id));
        }
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert_eq!("".parse::<Id>(), Err(ParseIdError::Length));
        assert_eq!("U".repeat(26).parse::<Id>(), Err(ParseIdError::Character));
        assert_eq!("Z".repeat(26).parse::<Id>(), Err(ParseIdError::Overflow));
    }

    #[test]
    fn parse_is_case_insensitive() {
        let id = Id::from_u128(0xdead_beef);
        assert_eq!(id.to_string().to_lowercase().parse::<Id>(), Ok(id));
    }

    #[test]
    fn seq_ids_are_unique_and_namespaced() {
        let mut a = SeqIds::new(1);
        let mut b = SeqIds::new(2);
        let (a0, a1, b0) = (a.next_id(), a.next_id(), b.next_id());
        assert_ne!(a0, a1);
        assert_ne!(a0, b0);
    }

    #[test]
    fn entropy_ids_use_two_words() {
        let mut n = 0u64;
        let mut src = EntropyIds::new(|| {
            n += 1;
            n
        });
        assert_eq!(src.next_id().as_u128(), (1u128 << 64) | 2);
    }
}
