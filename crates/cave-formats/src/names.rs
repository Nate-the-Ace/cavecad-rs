//! Station names each format can hold. A name typed here ("Big Room 1", "é1") can break a format
//! that splits on blanks, starts comments with `;`, or has a length limit; before this, writing one
//! silently lost the legs that used it. Names a format can hold are left exactly as they are.

use crate::Format;
use crate::model::Survey;
use std::collections::{HashMap, HashSet};

/// Whether a character may stand in a station name in `f`.
fn allowed(f: Format, c: char) -> bool {
    match f {
        // cavern's own set; `.` separates survey levels, which is how nested names are written
        Format::Survex => c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'),
        Format::Therion => c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '\'' | '+'),
        // `:` is Walls's own prefix separator
        Format::Walls => {
            c.is_ascii_graphic()
                && !matches!(c, ';' | ',' | '#' | '<' | '>' | '*' | '(' | ')' | '"' | '$')
        }
        // printable ASCII; a blank or a comma would split the line
        Format::Compass => c.is_ascii_graphic() && c != ',',
        // the app's own CSV writes names unquoted: a comma or a line break would split the row
        Format::Csv => c != ',' && !c.is_control(),
    }
}

/// The longest name `f` holds, in characters. (Old Compass versions stopped at 12; the old app
/// wrote longer names anyway, and shortening them would turn `TestSurvey.S1` into a stranger, so
/// no limit is enforced.)
fn longest(_f: Format) -> usize {
    200
}

/// Names that mean "no station" in `f`.
fn reserved(f: Format, name: &str) -> bool {
    match f {
        Format::Survex => matches!(name, "-" | "." | ".."),
        Format::Therion => matches!(name, "-" | "." | ".."),
        Format::Walls => name.chars().all(|c| c == '-'),
        Format::Csv | Format::Compass => name == "-",
    }
}

/// Renames, in place, every station `f` cannot hold, to one it can that no other station uses.
/// Returns the renames (old, new), in name order.
pub(crate) fn make_safe(f: Format, survey: &mut Survey) -> Vec<(String, String)> {
    let mut all: Vec<String> = Vec::new();
    for s in &survey.shots {
        for n in [&s.from, &s.to] {
            if !n.is_empty() {
                all.push(n.clone());
            }
        }
    }
    all.extend(survey.fixed.iter().map(|(n, _)| n.to_string()));
    all.sort();
    all.dedup();
    let ok = |n: &str| {
        !reserved(f, n) && n.chars().count() <= longest(f) && n.chars().all(|c| allowed(f, c))
    };
    let mut taken: HashSet<String> = all.iter().filter(|n| ok(n)).cloned().collect();
    let mut renames: HashMap<String, String> = HashMap::new();
    for n in all.iter().filter(|n| !ok(n)) {
        let mut base: String = n
            .chars()
            .map(|c| if allowed(f, c) { c } else { '_' })
            .collect();
        if reserved(f, &base) || base.is_empty() {
            base = format!("_{base}");
        }
        let fit = |s: &str, room: usize| s.chars().take(room).collect::<String>();
        let mut pick = fit(&base, longest(f));
        let mut k = 2;
        while taken.contains(&pick) {
            let tail = format!("_{k}");
            pick = format!(
                "{}{tail}",
                fit(&base, longest(f).saturating_sub(tail.len()))
            );
            k += 1;
        }
        taken.insert(pick.clone());
        renames.insert(n.clone(), pick);
    }
    if renames.is_empty() {
        return Vec::new();
    }
    let swap = |n: &mut String| {
        if let Some(r) = renames.get(n.as_str()) {
            *n = r.clone();
        }
    };
    for s in &mut survey.shots {
        swap(&mut s.from);
        swap(&mut s.to);
    }
    let fixed: Vec<(String, crate::model::Fixed)> = survey
        .fixed
        .iter()
        .map(|(n, v)| {
            (
                renames.get(n).cloned().unwrap_or_else(|| n.to_string()),
                v.clone(),
            )
        })
        .collect();
    survey.fixed = Default::default();
    for (n, v) in fixed {
        survey.fixed.set(&n, v);
    }
    let mut out: Vec<(String, String)> = renames.into_iter().collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Shot;

    fn survey(names: &[&str]) -> Survey {
        let mut s = Survey::default();
        for w in names.windows(2) {
            s.shots.push(Shot {
                from: w[0].into(),
                to: w[1].into(),
                distance: 10.0,
                ..Shot::default()
            });
        }
        s
    }

    #[test]
    fn every_leg_survives_every_format() {
        let names = [
            "A1",
            "a b",
            "é1",
            "x:y",
            "main.1",
            "VERYLONGSTATIONNAME01",
            "-",
            "x;y",
            "q\"t",
            "a_b",
        ];
        for f in crate::FORMATS {
            let mut s = survey(&names);
            let n = s.shots.len();
            let text = f.write(&mut s);
            let back = f.parse(&text);
            assert_eq!(back.shots.len(), n, "{f:?}:\n{text}");
            // a renamed station is still one station: legs still join up
            for w in back.shots.windows(2) {
                assert_eq!(w[0].to, w[1].from, "{f:?}");
            }
        }
    }

    #[test]
    fn a_rename_never_takes_a_name_in_use() {
        let mut s = survey(&["a b", "a_b", "a?b"]);
        let r = make_safe(Format::Survex, &mut s);
        let mut names: Vec<&str> = s
            .shots
            .iter()
            .flat_map(|x| [x.from.as_str(), x.to.as_str()])
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 3, "{r:?}");
        assert!(r.iter().all(|(old, _)| old != "a_b"));
    }

    #[test]
    fn valid_names_are_untouched() {
        let mut s = survey(&["A1", "main.2", "x-3"]);
        assert!(make_safe(Format::Survex, &mut s).is_empty());
    }
}
