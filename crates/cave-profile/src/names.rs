//! Station names carry the surveyor's intent: which run a station is in and where in the run.

use std::cmp::Ordering;

/// A station name split on its trailing group of like characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Split {
    /// Everything before the trailing group: the run key.
    pub base: String,
    /// The trailing group: the sequence within the run (empty for a bare name like `A`).
    pub seq: String,
}

fn class(c: char) -> u8 {
    match c {
        '0'..='9' => 0,
        'a'..='z' => 1,
        'A'..='Z' => 2,
        _ => 3,
    }
}

/// Splits a name on its trailing group of like characters, or `None` when it is not a station
/// name: empty, or a splay (splays carry a dot, as in `A3.1`). A one-group name (`A`, `12`) has
/// no trailing group to peel off, so the whole name is the base and the sequence is empty: it is
/// that run's origin.
pub fn split_name(name: &str) -> Option<Split> {
    if name.is_empty() || name.contains('.') {
        return None;
    }
    let chars: Vec<char> = name.chars().collect();
    let mut start = chars.len() - 1;
    while start > 0 && class(chars[start - 1]) == class(chars[chars.len() - 1]) {
        start -= 1;
    }
    if start == 0 {
        return Some(Split {
            base: name.to_string(),
            seq: String::new(),
        });
    }
    Some(Split {
        base: chars[..start].iter().collect(),
        seq: chars[start..].iter().collect(),
    })
}

/// The run a station belongs to.
pub fn run_key_of(name: &str) -> Option<String> {
    split_name(name).map(|s| s.base)
}

/// The station a run ties in at, read from its own key by dropping a trailing lowercase group:
/// `A13a` -> `A13`, `A13a2b` -> `A13a2`. `None` for a letter run (`B`), which gets its parent from
/// the graph. Takes a run KEY, not a station name.
pub fn tie_name_of_run(run_key: &str) -> Option<String> {
    let trimmed = run_key.trim_end_matches(|c: char| c.is_ascii_lowercase());
    (trimmed.len() != run_key.len() && !trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Total order for a run's members. Empty sequence (the origin) first; numbers compare as numbers
/// (A10 follows A9); numbered before lettered; ties between distinct texts fall back to text.
pub fn seq_order(a: &str, b: &str) -> Ordering {
    if a.is_empty() || b.is_empty() {
        return a.is_empty().cmp(&b.is_empty()).reverse();
    }
    let digits = |s: &str| s.bytes().all(|c| c.is_ascii_digit());
    match (digits(a), digits(b)) {
        (true, true) => {
            let n = |s: &str| s.parse::<f64>().unwrap_or(f64::MAX);
            n(a).total_cmp(&n(b)).then_with(|| a.cmp(b))
        }
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a.cmp(b),
    }
}

/// The numeric sequence of a name, or `f64::MAX` when it has none.
pub fn seq_num_of(name: &str) -> f64 {
    match split_name(name) {
        Some(s) if !s.seq.is_empty() && s.seq.bytes().all(|c| c.is_ascii_digit()) => {
            s.seq.parse().unwrap_or(f64::MAX)
        }
        _ => f64::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(n: &str) -> (String, String) {
        let s = split_name(n).unwrap();
        (s.base, s.seq)
    }

    #[test]
    fn the_old_docblock_examples() {
        assert_eq!(split("A20"), ("A".into(), "20".into()));
        assert_eq!(split("A13a1"), ("A13a".into(), "1".into()));
        assert_eq!(split("A13b1"), ("A13b".into(), "1".into()));
        assert_eq!(split("A13a2b1"), ("A13a2b".into(), "1".into()));
        assert_eq!(split("B1"), ("B".into(), "1".into()));
    }

    #[test]
    fn single_group_names_are_the_runs_origin() {
        assert_eq!(split("A"), ("A".into(), String::new()));
        assert_eq!(split("12"), ("12".into(), String::new()));
        assert_eq!(split_name(""), None);
        assert_eq!(split_name("A3.1"), None, "splays are not stations");
    }

    #[test]
    fn odd_names_still_group() {
        assert_eq!(split("A#"), ("A".into(), "#".into()));
        assert_eq!(split("A1-"), ("A1".into(), "-".into()));
    }

    #[test]
    fn tie_comes_from_a_trailing_lowercase_group() {
        assert_eq!(tie_name_of_run("A13a").as_deref(), Some("A13"));
        assert_eq!(tie_name_of_run("A13a2b").as_deref(), Some("A13a2"));
        assert_eq!(tie_name_of_run("B"), None, "a letter run asks the graph");
        assert_eq!(tie_name_of_run("a"), None);
    }

    #[test]
    fn sequence_order_is_total_and_numeric() {
        let mut v = vec!["10", "9", "", "2", "b", "a", "01", "1"];
        v.sort_by(|a, b| seq_order(a, b));
        assert_eq!(v, vec!["", "01", "1", "2", "9", "10", "a", "b"]);
        assert_eq!(seq_order("1", "1"), Ordering::Equal);
    }

    #[test]
    fn seq_num_falls_back_to_max() {
        assert_eq!(seq_num_of("A12"), 12.0);
        assert_eq!(seq_num_of("A"), f64::MAX);
        assert_eq!(seq_num_of("A1a"), f64::MAX);
    }
}
