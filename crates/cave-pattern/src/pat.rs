//! AutoCAD hatch patterns (`.pat` text): reading them into fills, and the standard ANSI and AR patterns that ship with the app.
//!
//! A `.pat` file holds one or more patterns. Each starts with a header line `*NAME, description` and has one line per line
//! family: `angle, x, y, shift, spacing [, dash, ...]` (a dash more than 0 is drawn, less than 0 is a gap, 0 is a dot). `;`
//! starts a comment. Lengths are millimetres unless the file says `;%UNITS=INCH`.
//!
//! The pattern files under `pat/` are the metric ANSI31-38 and AR-* definitions as distributed with QCAD (the app CaveCAD
//! used to be built on).

use crate::fill::MAX_DASHES;
use crate::{FillDef, Layer, MAX_MM, MAX_PARTS, valid_name};

/// Why a `.pat` file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatError {
    /// There is no `*NAME` header line, so no pattern.
    NoPattern,
    /// A line that should be numbers is not (1-based line number).
    BadLine(usize),
}

impl std::fmt::Display for PatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatError::NoPattern => write!(
                f,
                "No pattern found: a pattern starts with a line like *NAME, description."
            ),
            PatError::BadLine(n) => write!(
                f,
                "Line {n} is not a pattern line (angle, x, y, shift, spacing, then dashes)."
            ),
        }
    }
}

/// A name for a pattern header that the library accepts: letters, digits, spaces, `-`, `_` and `.`, at most 40.
fn clean_name(raw: &str) -> String {
    let s: String = raw
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_.".contains(c) {
                c
            } else {
                '-'
            }
        })
        .take(40)
        .collect();
    let s = s.trim().to_string();
    if valid_name(&s) {
        s
    } else {
        "Imported pattern".into()
    }
}

/// Reads every pattern in a `.pat` file, every length multiplied by `scale` (on top of an inch file's 25.4). A pattern with
/// more families or dashes than a fill can hold keeps the first ones; lengths are kept within the fill's limits.
pub fn parse_pat(text: &str, scale: f64) -> Result<Vec<FillDef>, PatError> {
    let inches = text
        .lines()
        .any(|l| l.trim().to_ascii_uppercase().replace(' ', "") == ";%UNITS=INCH");
    let k = scale * if inches { 25.4 } else { 1.0 };
    let clamp = |v: f64| v.clamp(-MAX_MM, MAX_MM);
    let mut out: Vec<FillDef> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(head) = line.strip_prefix('*') {
            let name = clean_name(head.split(',').next().unwrap_or(""));
            out.push(FillDef {
                name,
                layers: Vec::new(),
                tint: false,
            });
            continue;
        }
        let Some(def) = out.last_mut() else {
            return Err(PatError::BadLine(i + 1));
        };
        let nums: Result<Vec<f64>, _> = line.split(',').map(|t| t.trim().parse::<f64>()).collect();
        let Ok(nums) = nums else {
            return Err(PatError::BadLine(i + 1));
        };
        if nums.len() < 5 || nums.iter().any(|v| !v.is_finite()) {
            return Err(PatError::BadLine(i + 1));
        }
        if def.layers.len() >= MAX_PARTS {
            continue;
        }
        let mut spacing = clamp(nums[4] * k);
        if spacing.abs() < crate::MIN_MM {
            spacing = crate::MIN_MM.copysign(if spacing == 0.0 { 1.0 } else { spacing });
        }
        let angle = nums[0].rem_euclid(360.0);
        def.layers.push(Layer::PatLines {
            angle_deg: if angle > 180.0 { angle - 360.0 } else { angle },
            x_mm: clamp(nums[1] * k),
            y_mm: clamp(nums[2] * k),
            shift_mm: clamp(nums[3] * k),
            spacing_mm: spacing,
            dashes_mm: nums[5..]
                .iter()
                .take(MAX_DASHES)
                .map(|d| clamp(d * k))
                .collect(),
        });
    }
    out.retain(|d| !d.layers.is_empty());
    if out.is_empty() {
        return Err(PatError::NoPattern);
    }
    Ok(out)
}

/// How much smaller than real life the AR (architectural) patterns are drawn: their bricks and boards are real sizes (a brick is
/// 203 mm long), so at 1/16 a brick is about 13 mm on paper.
pub const AR_SCALE: f64 = 1.0 / 16.0;

const ANSI: &[&str] = &[
    include_str!("../pat/ansi31.pat"),
    include_str!("../pat/ansi32.pat"),
    include_str!("../pat/ansi33.pat"),
    include_str!("../pat/ansi34.pat"),
    include_str!("../pat/ansi35.pat"),
    include_str!("../pat/ansi36.pat"),
    include_str!("../pat/ansi37.pat"),
    include_str!("../pat/ansi38.pat"),
];

const AR: &[&str] = &[
    include_str!("../pat/ar-b816.pat"),
    include_str!("../pat/ar-b816c.pat"),
    include_str!("../pat/ar-b88.pat"),
    include_str!("../pat/ar-brelm.pat"),
    include_str!("../pat/ar-brstd.pat"),
    include_str!("../pat/ar-conc.pat"),
    include_str!("../pat/ar-hbone.pat"),
    include_str!("../pat/ar-parq1.pat"),
    include_str!("../pat/ar-rroof.pat"),
    include_str!("../pat/ar-rshke.pat"),
    include_str!("../pat/ar-sand.pat"),
];

/// The standard AutoCAD ANSI31-38 and AR-* hatch patterns as fills.
pub fn standard_fills() -> Vec<FillDef> {
    let mut out = Vec::new();
    for (texts, scale) in [(ANSI, 1.0), (AR, AR_SCALE)] {
        for t in texts {
            out.extend(parse_pat(t, scale).expect("a shipped .pat file reads"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_standard_pattern_reads_and_is_valid() {
        let fills = standard_fills();
        let names: Vec<&str> = fills.iter().map(|f| f.name.as_str()).collect();
        for n in [
            "ANSI31", "ANSI38", "AR-B816", "AR-CONC", "AR-SAND", "AR-RSHKE", "AR-PARQ1",
        ] {
            assert!(names.contains(&n), "{n} missing from {names:?}");
        }
        assert_eq!(fills.len(), 19);
        for f in &fills {
            f.validate(&|_| true)
                .unwrap_or_else(|e| panic!("{}: {e}", f.name));
        }
    }

    #[test]
    fn a_pat_line_keeps_its_numbers_and_inches_are_converted() {
        let f = &parse_pat(
            "*TEST, a test\n; comment\n45, 0,0, 0,3.175\n0, 1,2, 3,4, 5,-6,0\n",
            1.0,
        )
        .unwrap()[0];
        assert_eq!(f.name, "TEST");
        assert_eq!(f.layers.len(), 2);
        let Layer::PatLines {
            angle_deg,
            x_mm,
            y_mm,
            shift_mm,
            spacing_mm,
            dashes_mm,
        } = &f.layers[1]
        else {
            panic!()
        };
        assert_eq!(
            (*angle_deg, *x_mm, *y_mm, *shift_mm, *spacing_mm),
            (0.0, 1.0, 2.0, 3.0, 4.0)
        );
        assert_eq!(dashes_mm, &vec![5.0, -6.0, 0.0]);
        let inch = &parse_pat(";%UNITS=INCH\n*I\n0, 0,0, 0,1\n", 1.0).unwrap()[0];
        let Layer::PatLines { spacing_mm, .. } = &inch.layers[0] else {
            panic!()
        };
        assert!((spacing_mm - 25.4).abs() < 1e-9);
    }

    #[test]
    fn a_bad_file_is_refused_with_its_line() {
        assert_eq!(parse_pat("no header", 1.0), Err(PatError::BadLine(1)));
        assert_eq!(
            parse_pat("*A\n45, x, 0, 0, 1\n", 1.0),
            Err(PatError::BadLine(2))
        );
        assert_eq!(
            parse_pat("; only a comment\n", 1.0),
            Err(PatError::NoPattern)
        );
    }

    #[test]
    fn ansi31_fills_a_square_with_45_degree_lines() {
        let f = &standard_fills()[0];
        let sq = [[0.0, 0.0], [0.05, 0.0], [0.05, 0.05], [0.0, 0.05]]; // 50 mm on paper at 1:1000 is 0.05 m per mm... see paper_m
        let m = crate::fill_marks(f, &|_| None, &sq, 1, 0.001, 1.0);
        assert!(m.lines.len() > 10, "{}", m.lines.len());
        for l in &m.lines {
            let (dx, dy) = (l[1][0] - l[0][0], l[1][1] - l[0][1]);
            assert!((dx - dy).abs() < 1e-9, "every line runs at 45 degrees");
        }
    }

    #[test]
    fn dashes_leave_gaps_and_dots_make_dots() {
        let f = &parse_pat("*D\n0, 0,0, 0,1, 2,-2,0,-2\n", 1.0).unwrap()[0];
        let sq = [[0.0, 0.0], [0.02, 0.0], [0.02, 0.0105], [0.0, 0.0105]];
        let m = crate::fill_marks(f, &|_| None, &sq, 1, 0.001, 1.0);
        assert!(!m.circles.is_empty(), "the 0 in the dashes is a dot");
        for l in &m.lines {
            let len = (l[1][0] - l[0][0]).hypot(l[1][1] - l[0][1]);
            assert!(len <= 0.002 + 1e-9, "no dash is longer than 2 mm on paper");
        }
    }
}
