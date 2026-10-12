//! Safety net that keeps coordinates out of the log.
//!
//! The first rule is not to log coordinates at all. This catches slips: two decimal numbers with
//! four or more decimal places next to each other (latitude/longitude style) are replaced.

const HIDDEN: &str = "[location hidden]";

struct Num {
    start: usize,
    end: usize,
}

/// Finds numbers like `-12.34567` (an optional sign, digits, a point, at least four decimals).
fn precise_numbers(s: &str) -> Vec<Num> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let mut j = i;
        if b[j] == b'-' || b[j] == b'+' {
            j += 1;
        }
        let int_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > int_start && j < b.len() && b[j] == b'.' {
            let frac_start = j + 1;
            let mut k = frac_start;
            while k < b.len() && b[k].is_ascii_digit() {
                k += 1;
            }
            if k - frac_start >= 4 {
                out.push(Num { start, end: k });
                i = k;
                continue;
            }
            i = k.max(i + 1);
            continue;
        }
        i = if j > i { j } else { i + 1 };
    }
    out
}

/// Replaces coordinate-looking number pairs with `[location hidden]`.
pub fn redact(s: &str) -> String {
    let nums = precise_numbers(s);
    let mut out = String::with_capacity(s.len());
    let mut cursor = 0;
    let mut i = 0;
    while i < nums.len() {
        let paired = i + 1 < nums.len()
            && s[nums[i].end..nums[i + 1].start]
                .chars()
                .all(|c| matches!(c, ',' | ';' | ' ' | '\t'));
        if paired {
            out.push_str(&s[cursor..nums[i].start]);
            out.push_str(HIDDEN);
            cursor = nums[i + 1].end;
            i += 2;
        } else {
            i += 1;
        }
    }
    out.push_str(&s[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn hides_lat_lon_pairs() {
        assert_eq!(
            redact("entrance at 36.12345, -86.54321 ok"),
            "entrance at [location hidden] ok"
        );
        assert_eq!(redact("36.1234;-86.5432"), "[location hidden]");
    }

    #[test]
    fn keeps_ordinary_numbers() {
        for s in [
            "closure 0.4%",
            "loop 3 of 12",
            "length 12.5 m",
            "z 100.25, 3.5",
            "v1.2.3",
        ] {
            assert_eq!(redact(s), s);
        }
    }

    #[test]
    fn single_precise_number_is_kept() {
        assert_eq!(redact("closure error 0.000123"), "closure error 0.000123");
    }

    #[test]
    fn multiple_pairs() {
        assert_eq!(
            redact("a 1.2345 2.3456 b 3.4567,4.5678"),
            "a [location hidden] b [location hidden]"
        );
    }
}
