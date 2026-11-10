//! The handful of JavaScript behaviours the old readers and writers depended on, reproduced exactly:
//! how text becomes a number (`parseFloat`, and the stricter whole-string conversion), how a number
//! becomes text (`toFixed`, plain `String(n)`), what counts as white space, and the order an object's
//! keys come back in. A port that rounds 0.125 the other way, or lists station "10" after "A1", writes a
//! different file from the same survey.

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::marker::PhantomData;

/// JavaScript's `\s`: the characters a regular expression treats as white space.
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `s.replace(/^\s+|\s+$/g, "")`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `s.replace(/^\s+/, "")`.
pub fn trim_start(s: &str) -> &str {
    s.trim_start_matches(is_space)
}

/// `s.replace(/\s+$/, "")`.
pub fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// `s.split(/\s+/)`: runs of white space separate, and an empty piece is kept at either end.
pub fn split_ws(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_space = false;
    for c in s.chars() {
        if is_space(c) {
            if !in_space {
                out.push(std::mem::take(&mut cur));
                in_space = true;
            }
        } else {
            in_space = false;
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

/// `s.split(/\r\n|\r|\n/)`.
pub fn split_lines(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(std::mem::take(&mut cur));
            }
            '\n' => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// The length of the longest prefix of `s` that reads as a decimal number (sign, digits, point,
/// exponent), or of "Infinity". Zero when there is none.
fn decimal_prefix(s: &str) -> usize {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    if s[i..].starts_with("Infinity") {
        return i + 8;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        let frac_start = i + 1;
        let mut j = frac_start;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if digits > 0 || j > frac_start {
            digits += j - frac_start;
            i = j;
        }
    }
    if digits == 0 {
        return 0;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    i
}

fn read_decimal(s: &str) -> f64 {
    let (neg, body) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let v = if body == "Infinity" {
        f64::INFINITY
    } else {
        // Rust reads "5." and ".5" the same way JavaScript does.
        body.parse::<f64>().unwrap_or(f64::NAN)
    };
    if neg { -v } else { v }
}

/// `parseFloat(s)`: leading white space skipped, then the longest number at the front; NaN when
/// the text does not start with one.
pub fn parse_float(s: &str) -> f64 {
    let t = trim_start(s);
    let n = decimal_prefix(t);
    if n == 0 {
        return f64::NAN;
    }
    read_decimal(&t[..n])
}

/// `Number(s)` (and so what `isFinite(s)` tests): the WHOLE text must be a number, empty text is
/// 0, and 0x / 0o / 0b prefixes are read.
pub fn to_number(s: &str) -> f64 {
    let t = trim(s);
    if t.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(rest) = t.strip_prefix(prefix) {
            return u64::from_str_radix(rest, radix).map_or(f64::NAN, |v| v as f64);
        }
    }
    if decimal_prefix(t) == t.len() {
        read_decimal(t)
    } else {
        f64::NAN
    }
}

/// `isFinite(s)` on a string.
pub fn is_finite_str(s: &str) -> bool {
    to_number(s).is_finite()
}

/// `x.toFixed(digits)`: the exact value rounded half UP (away from zero), not to even, and a
/// negative sign only on a negative number (so -0 is "0.00" but -0.001 is "-0.00").
pub fn to_fixed(x: f64, digits: usize) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if x.abs() >= 1e21 {
        return num_str(x);
    }
    let neg = x < 0.0;
    // Far more digits than a double holds, so the one after the cut is the true one.
    let exact = format!("{:.*}", digits + 40, x.abs());
    let point = exact.find('.').unwrap_or(exact.len());
    let keep = point + 1 + digits;
    let mut out: Vec<u8> = exact.as_bytes()[..keep.min(exact.len())].to_vec();
    if digits == 0 {
        out.truncate(point);
    }
    let next = exact.as_bytes().get(keep).copied().unwrap_or(b'0');
    let next = if digits == 0 {
        exact.as_bytes()[point + 1]
    } else {
        next
    };
    if next >= b'5' {
        let mut i = out.len();
        loop {
            if i == 0 {
                out.insert(0, b'1');
                break;
            }
            i -= 1;
            match out[i] {
                b'.' => continue,
                b'9' => out[i] = b'0',
                d => {
                    out[i] = d + 1;
                    break;
                }
            }
        }
    }
    let s = String::from_utf8(out).unwrap_or_default();
    if neg { format!("-{s}") } else { s }
}

/// `String(x)`: the shortest text that reads back as the same number, in plain notation from
/// 1e-6 up to 1e21 and exponent notation outside it.
pub fn num_str(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if x == 0.0 {
        return "0".into();
    }
    let a = x.abs();
    if (1e-6..1e21).contains(&a) {
        return format!("{x}");
    }
    // Rust writes "1e21" / "1.5e-7"; JavaScript writes "1e+21" / "1.5e-7".
    let e = format!("{x:e}");
    match e.split_once('e') {
        Some((m, exp)) if !exp.starts_with('-') => format!("{m}e+{exp}"),
        _ => e,
    }
}

/// Whether a key is one JavaScript lists before all others, in numeric order: an array index
/// ("0", "7", "12", never "07" or "-1").
fn is_index_key(k: &str) -> bool {
    if k.is_empty() || k.len() > 10 || !k.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if k.len() > 1 && k.starts_with('0') {
        return false;
    }
    k.parse::<u64>().is_ok_and(|v| v < 4_294_967_295)
}

/// A JavaScript object used as a map from names: insertion order, except that index-like names
/// ("1", "20") come first, in numeric order -- which is the order `for (k in obj)` visits them in,
/// and so the order the old writers emitted stations and fixed points in.
#[derive(Clone, Debug, PartialEq)]
pub struct JsObj<V> {
    entries: Vec<(String, V)>,
}

impl<V> Default for JsObj<V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<V> JsObj<V> {
    /// An empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// `obj.hasOwnProperty(k)`.
    pub fn has(&self, k: &str) -> bool {
        self.entries.iter().any(|(key, _)| key == k)
    }

    /// `obj[k]`.
    pub fn get(&self, k: &str) -> Option<&V> {
        self.entries
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v)
    }

    /// `obj[k]` to change in place.
    pub fn get_mut(&mut self, k: &str) -> Option<&mut V> {
        self.entries
            .iter_mut()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v)
    }

    /// `obj[k] = v`: a name already there keeps its place.
    pub fn set(&mut self, k: &str, v: V) {
        match self.get_mut(k) {
            Some(slot) => *slot = v,
            None => self.entries.push((k.to_string(), v)),
        }
    }

    /// Whether there is nothing in it.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many names it holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The entries in the order `for (k in obj)` visits them.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        let mut index: Vec<&(String, V)> = self
            .entries
            .iter()
            .filter(|(k, _)| is_index_key(k))
            .collect();
        index.sort_by_key(|(k, _)| k.parse::<u64>().unwrap_or(0));
        index
            .into_iter()
            .chain(self.entries.iter().filter(|(k, _)| !is_index_key(k)))
            .map(|(k, v)| (k.as_str(), v))
    }
}

impl<V: Serialize> Serialize for JsObj<V> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.entries.len()))?;
        for (k, v) in self.iter() {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for JsObj<V> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V2<V>(PhantomData<V>);
        impl<'de, V: Deserialize<'de>> Visitor<'de> for V2<V> {
            type Value = JsObj<V>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an object")
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(JsObj::new())
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut out = JsObj::new();
                while let Some((k, v)) = a.next_entry::<String, V>()? {
                    out.set(&k, v);
                }
                Ok(out)
            }
        }
        d.deserialize_any(V2(PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_float_reads_the_front_of_the_text() {
        assert_eq!(parse_float("  12.5ft"), 12.5);
        assert_eq!(parse_float("-.5"), -0.5);
        assert_eq!(parse_float("5."), 5.0);
        assert_eq!(parse_float("1e3x"), 1000.0);
        assert_eq!(parse_float("1e"), 1.0);
        assert!(parse_float("P").is_nan());
        assert!(parse_float("").is_nan());
        assert!(parse_float(".").is_nan());
        assert_eq!(parse_float("-Infinity"), f64::NEG_INFINITY);
    }

    #[test]
    fn to_number_wants_the_whole_text() {
        assert_eq!(to_number(""), 0.0);
        assert_eq!(to_number(" 3 "), 3.0);
        assert!(to_number("3m").is_nan());
        assert_eq!(to_number("0x10"), 16.0);
    }

    #[test]
    fn to_fixed_rounds_half_up_like_javascript() {
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(1.005, 2), "1.00"); // 1.005 is really 1.00499999...
        assert_eq!(to_fixed(-0.0, 2), "0.00");
        assert_eq!(to_fixed(-0.001, 2), "-0.00");
        assert_eq!(to_fixed(9.995, 2), "9.99");
        assert_eq!(to_fixed(99.999, 2), "100.00");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(-9.9, 2), "-9.90");
    }

    #[test]
    fn num_str_matches_javascript() {
        assert_eq!(num_str(5.0), "5");
        assert_eq!(num_str(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(num_str(-0.0), "0");
        assert_eq!(num_str(1e21), "1e+21");
        assert_eq!(num_str(1.5e-7), "1.5e-7");
        assert_eq!(num_str(123456.789), "123456.789");
    }

    #[test]
    fn index_keys_come_first_in_numeric_order() {
        let mut o = JsObj::new();
        for k in ["A1", "10", "B", "2", "01"] {
            o.set(k, ());
        }
        let keys: Vec<&str> = o.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["2", "10", "A1", "B", "01"]);
    }

    #[test]
    fn splitting_keeps_empty_ends() {
        assert_eq!(split_ws(" a  b "), ["", "a", "b", ""]);
        assert_eq!(split_ws(""), [""]);
        assert_eq!(split_lines("a\r\nb\rc\n"), ["a", "b", "c", ""]);
    }
}
