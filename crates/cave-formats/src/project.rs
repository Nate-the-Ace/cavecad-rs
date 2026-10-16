//! Project files: one file that names many survey files, read as one cave.
//!
//! - **Compass .mak**: `#FILE.DAT,STA[F,e,n,z],STA;` lists the data files and the stations fixed in
//!   them (feet `F` or metres `M`; a station with no brackets is only a link). `@` base location, `&`
//!   datum, `$` UTM zone, `%`/`*` convergence and `!` options are read past. `/` starts a comment.
//! - **Walls .wpj**: `.BOOK`/`.ENDBOOK` nest, `.PATH` is a folder relative to the book it is in,
//!   `.SURVEY` with `.NAME` is a data file (`NAME.SRV`), `.OPTIONS` are `#Units` options that hold
//!   for everything below them. `.REF` (geographic reference) and `.STATUS` are read past.
//! - **Therion thconfig**: `source` lines name the .th files; `input` inside them is followed as
//!   for a single .th.
//!
//! Files are found through a reader the caller gives (`read(path)`), tried as written and, because
//! these projects mostly come from Windows, then without regard to case. Every file the project
//! names but the reader cannot give is a finding, never a silent gap: a cave missing half its
//! passages looks like a smaller cave.

use crate::js::{split_lines, trim};
use crate::model::{Fixed, GeoRef, Survey};
use crate::{compass, therion, walls};

/// A project file format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectFormat {
    /// Compass .mak
    CompassMak,
    /// Walls .wpj
    WallsWpj,
    /// Therion thconfig
    Thconfig,
}

impl ProjectFormat {
    /// What people call it.
    pub fn label(self) -> &'static str {
        match self {
            ProjectFormat::CompassMak => "Compass project (.mak)",
            ProjectFormat::WallsWpj => "Walls project (.wpj)",
            ProjectFormat::Thconfig => "Therion project (thconfig)",
        }
    }
}

/// Whether a file is a project file, by name and then by content.
pub fn detect_project(file_name: &str, content: &str) -> Option<ProjectFormat> {
    let lower = file_name.to_ascii_lowercase();
    let base = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    if base.ends_with(".mak") {
        return Some(ProjectFormat::CompassMak);
    }
    if base.ends_with(".wpj") {
        return Some(ProjectFormat::WallsWpj);
    }
    if base == "thconfig" || base.ends_with(".thconfig") || base.starts_with("thconfig.") {
        return Some(ProjectFormat::Thconfig);
    }
    let lines: Vec<String> = split_lines(content).into_iter().take(200).collect();
    if lines
        .iter()
        .any(|l| trim(l).to_ascii_uppercase().starts_with(".BOOK"))
    {
        return Some(ProjectFormat::WallsWpj);
    }
    if lines.iter().any(|l| {
        let t = trim(l).to_ascii_lowercase();
        t.starts_with('#')
            && t.split(',')
                .next()
                .is_some_and(|f| f.trim_end().ends_with(".dat"))
    }) {
        return Some(ProjectFormat::CompassMak);
    }
    None
}

/// `dir/rel` with `.` and `..` worked out; `rel` as it is when it is absolute.
fn join(dir: &str, rel: &str) -> String {
    let rel = rel.replace('\\', "/");
    let absolute = rel.starts_with('/') || (rel.len() > 2 && rel.as_bytes()[1] == b':');
    let full = if absolute || dir.is_empty() {
        rel
    } else {
        format!("{dir}/{rel}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for p in full.split('/') {
        match p {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            p => parts.push(p),
        }
    }
    let joined = parts.join("/");
    if full.starts_with('/') {
        format!("/{joined}")
    } else {
        joined
    }
}

fn dir_of(path: &str) -> String {
    let p = path.replace('\\', "/");
    match p.rfind('/') {
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}

/// What reading a project gave.
pub struct ProjectRead {
    /// The project's format.
    pub format: ProjectFormat,
    /// Every file it names, merged into one survey (findings included).
    pub survey: Survey,
    /// The files that were read, in order.
    pub files: Vec<String>,
}

/// Reads a project file at `path` and every survey file it names, through `read`.
pub fn read_project(
    format: ProjectFormat,
    path: &str,
    content: &str,
    read: &dyn Fn(&str) -> Option<String>,
) -> ProjectRead {
    match format {
        ProjectFormat::CompassMak => read_mak(path, content, read),
        ProjectFormat::WallsWpj => read_wpj(path, content, read),
        ProjectFormat::Thconfig => read_thconfig(path, content, read),
    }
}

fn missing(survey: &mut Survey, project: &str, name: &str) {
    survey.add_finding(
        "warning",
        "project-file-missing",
        format!("{project} names \"{name}\", and it could not be read -- every passage in it is missing from this import. Pick it together with the project file."),
    );
}

/// Merges `parts` in order (the first decides the unit). An empty project is an empty survey.
fn merge(parts: Vec<Survey>, findings: Survey) -> Survey {
    let mut it = parts.into_iter();
    let mut out = it.next().unwrap_or_default();
    for p in it {
        out.absorb(p);
    }
    for f in findings.parse_findings.into_iter().flatten() {
        out.add_finding(&f.severity, &f.code, f.message);
    }
    out
}

// ---- Compass .mak --------------------------------------------------------------------------

/// The statements of a .mak: each runs from its marker character to the next ';' (white space and
/// line breaks inside one mean nothing), and `/` starts a comment that ends at the next `/` or the
/// end of the line. Folders (`[Name;` ... `];`) are statements too, and are read past.
fn mak_statements(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Option<String> = None;
    let mut comment = false;
    for c in content.chars() {
        if comment {
            // a comment ends at the next '/' or at the end of the line
            if c == '/' || c == '\n' || c == '\r' {
                comment = false;
            }
            continue;
        }
        match cur.as_mut() {
            None => {
                if c == '/' {
                    comment = true;
                } else if !c.is_whitespace() {
                    cur = Some(c.to_string());
                }
            }
            Some(s) => {
                if c == ';' {
                    out.push(std::mem::take(s));
                    cur = None;
                } else if c != '\n' && c != '\r' {
                    s.push(c);
                }
            }
        }
    }
    if let Some(s) = cur.filter(|s| !s.trim().is_empty()) {
        out.push(s);
    }
    out
}

/// Splits on commas outside brackets.
fn split_outside_brackets(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '[' => {
                depth += 1;
                cur.push(c);
            }
            ']' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth <= 0 => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// `NAME[F,e,n,z]`: the station and its fixed position in feet, if it has one. The unit letter (`F`
/// or `M`) comes first, then east, north and vertical; anything that cannot be part of a number
/// separates them.
fn mak_station(item: &str) -> Option<(String, Option<Fixed>)> {
    let item = item.trim();
    if item.is_empty() {
        return None;
    }
    let Some(open) = item.find('[') else {
        return Some((item.to_string(), None));
    };
    let name = item[..open].trim().to_string();
    let inner = item[open + 1..].trim_end_matches(']');
    let Some(unit) = inner.trim_start().chars().next() else {
        return Some((name, None));
    };
    // F feet, M metres; Compass reads any other letter as metres (Survex checked)
    let k = if unit.eq_ignore_ascii_case(&'F') {
        1.0
    } else {
        crate::FEET_PER_METER
    };
    let rest = &inner.trim_start()[unit.len_utf8()..];
    let mut numbers = Vec::new();
    let mut cur = String::new();
    for c in rest.chars().chain(std::iter::once(' ')) {
        let continues =
            c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && cur.is_empty());
        if continues {
            cur.push(c);
        } else if !cur.is_empty() {
            if let Ok(v) = cur.parse::<f64>() {
                numbers.push(v * k);
            }
            cur.clear();
        }
    }
    match numbers[..] {
        [e, n, z, ..] => Some((name, Some(Fixed::at(e, n, z)))),
        _ => Some((name, None)),
    }
}

fn read_mak(path: &str, content: &str, read: &dyn Fn(&str) -> Option<String>) -> ProjectRead {
    let dir = dir_of(path);
    let project = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
    let mut notes = Survey::default();
    let mut parts = Vec::new();
    let mut files = Vec::new();
    let mut fixes: Vec<(String, Fixed)> = Vec::new();
    for st in mak_statements(content) {
        let Some(body) = st.strip_prefix('#') else {
            continue;
        };
        let items = split_outside_brackets(body);
        let file = items[0].trim().to_string();
        if file.is_empty() {
            continue;
        }
        for item in &items[1..] {
            if let Some((name, Some(f))) = mak_station(item) {
                fixes.push((name, f));
            }
        }
        let full = join(&dir, &file);
        match read(&full) {
            Some(text) => {
                let mut s = compass::parse(&text);
                s.convert_units("ft");
                parts.push(s);
                files.push(full);
            }
            None => missing(&mut notes, &project, &file),
        }
    }
    let mut survey = merge(parts, notes);
    survey.convert_units("ft");
    // the project's fixes are the cave's: they are in feet, as Compass keeps everything
    // the first fix of a station holds; a second is ignored, as Compass does
    let mut seen: Vec<String> = Vec::new();
    for (name, f) in fixes {
        if !seen.contains(&name) {
            survey.fixed.set(&name, f);
            seen.push(name);
        }
    }
    if survey.cave_name.is_empty() {
        survey.cave_name = project
            .rsplit_once('.')
            .map_or(project.clone(), |(s, _)| s.to_string());
    }
    ProjectRead {
        format: ProjectFormat::CompassMak,
        survey,
        files,
    }
}

// ---- Walls .wpj ----------------------------------------------------------------------------
//
// The format is not documented; this follows Survex's reader (src/datain.c, reverse-engineered from
// Walls32.exe). An entry runs from its `.BOOK` or `.SURVEY` to the next of `.BOOK`, `.SURVEY` or
// `.ENDBOOK`. `.PATH` and `.OPTIONS` change the entry they are in and hold for everything inside a
// book. `.STATUS` is a bitmap: 0x2 "detached" (not processed with the rest -- except on the root
// book, where Walls always sets it), 0x10000 "an attached file that is not survey data".

const WPJ_DETACHED: u64 = 0x2;

/// `.REF northing easting zone convergence elevation flags latd latm lats lond lonm lons datum "name"`:
/// the place, from its latitude and longitude (flag 0x4: west, 0x8: south).
fn wpj_ref(rest: &str) -> Option<GeoRef> {
    let v: Vec<f64> = rest
        .split_whitespace()
        .take(12)
        .filter_map(|t| t.parse().ok())
        .collect();
    if v.len() < 12 {
        return None;
    }
    let flags = v[5] as u64;
    let lat = (v[6] + v[7] / 60.0 + v[8] / 3600.0) * if flags & 0x8 != 0 { -1.0 } else { 1.0 };
    let lon = (v[9] + v[10] / 60.0 + v[11] / 3600.0) * if flags & 0x4 != 0 { -1.0 } else { 1.0 };
    Some(GeoRef {
        lat,
        lon,
        zone: v[2] as i32,
    })
}
const WPJ_NOT_SURVEY: u64 = 0x10000;

#[derive(Clone)]
struct Frame {
    path: String,
    options: Vec<String>,
    georef: Option<GeoRef>,
}

fn read_wpj(path: &str, content: &str, read: &dyn Fn(&str) -> Option<String>) -> ProjectRead {
    let project = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
    let mut notes = Survey::default();
    let mut parts = Vec::new();
    let mut files = Vec::new();
    let mut frames = vec![Frame {
        path: dir_of(path),
        options: Vec::new(),
        georef: None,
    }];
    let mut title = String::new();
    let (mut depth, mut detached, mut status) = (0usize, 0usize, 0u64);
    let mut in_survey = false;
    let mut name = String::new();
    let mut skipped = 0usize;

    // reads the survey entry that just ended; true when it was left out as detached
    let mut finish = |frames: &mut Vec<Frame>, depth: usize, status: u64, name: &str| -> bool {
        let frame = frames.pop().unwrap_or(Frame {
            path: String::new(),
            options: Vec::new(),
            georef: None,
        });
        if status & WPJ_NOT_SURVEY != 0 {
            return false;
        }
        if status & WPJ_DETACHED != 0 && depth > 0 {
            return true;
        }
        if name.is_empty() {
            return false;
        }
        // Walls runs on Windows, where case does not matter; it writes names in upper case
        let lower = name.to_ascii_lowercase();
        let candidates = [
            format!("{name}.srv"),
            format!("{name}.SRV"),
            format!("{lower}.srv"),
        ]
        .map(|f| join(&frame.path, &f));
        match candidates
            .iter()
            .find_map(|c| read(c).map(|t| (c.clone(), t)))
        {
            Some((full, text)) => {
                let prefix: String = frame
                    .options
                    .iter()
                    .map(|o| format!("#Units {o}\n"))
                    .collect();
                parts.push(walls::parse_with(&format!("{prefix}{text}"), frame.georef));
                files.push(full);
            }
            None => missing(&mut notes, &project, &format!("{name}.SRV")),
        }
        false
    };

    for raw in split_lines(content) {
        let line = trim(&raw);
        let Some(body) = line.strip_prefix('.') else {
            continue;
        };
        let (word, rest) = match body.find(|c: char| c.is_whitespace()) {
            Some(i) => (&body[..i], trim(&body[i..])),
            None => (body, ""),
        };
        let word = word.to_ascii_uppercase();
        if detached > 0 {
            match word.as_str() {
                "BOOK" => detached += 1,
                "ENDBOOK" => detached -= 1,
                _ => {}
            }
            continue;
        }
        if in_survey && matches!(word.as_str(), "SURVEY" | "BOOK" | "ENDBOOK") {
            skipped += usize::from(finish(&mut frames, depth, status, &name));
            in_survey = false;
        }
        match word.as_str() {
            "BOOK" => {
                if title.is_empty() {
                    title = rest.to_string();
                }
                depth += 1;
                let top = frames.last().cloned().unwrap_or(Frame {
                    path: String::new(),
                    options: Vec::new(),
                    georef: None,
                });
                frames.push(top);
            }
            "SURVEY" => {
                let top = frames.last().cloned().unwrap_or(Frame {
                    path: String::new(),
                    options: Vec::new(),
                    georef: None,
                });
                frames.push(top);
                status = 0;
                name.clear();
                in_survey = true;
            }
            "ENDBOOK" => {
                depth = depth.saturating_sub(1);
                if frames.len() > 1 {
                    frames.pop();
                }
            }
            "OPTIONS" => {
                if let Some(f) = frames.last_mut()
                    && !rest.is_empty()
                {
                    f.options.push(rest.to_string());
                }
            }
            "REF" => {
                if let Some(f) = frames.last_mut() {
                    f.georef = wpj_ref(rest);
                }
            }
            "PATH" => {
                if let Some(f) = frames.last_mut()
                    && !rest.is_empty()
                {
                    f.path = join(&f.path, rest);
                }
            }
            "STATUS" => {
                status = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                if status & WPJ_DETACHED != 0 && !in_survey && depth > 1 {
                    // a detached book: everything up to its .ENDBOOK is left out
                    if frames.len() > 1 {
                        frames.pop();
                    }
                    depth -= 1;
                    detached = 1;
                    skipped += 1;
                }
            }
            "NAME" => name = rest.to_string(),
            _ => {}
        }
    }
    if in_survey {
        skipped += usize::from(finish(&mut frames, depth, status, &name));
    }
    if skipped > 0 {
        notes.add_finding(
            "info",
            "walls-detached",
            format!("{project} marks {skipped} part{} as detached, so {} left out, as Walls leaves {} out.", if skipped == 1 { "" } else { "s" }, if skipped == 1 { "it was" } else { "they were" }, if skipped == 1 { "it" } else { "them" }),
        );
    }
    let mut survey = merge(parts, notes);
    if survey.cave_name.is_empty() {
        survey.cave_name = if title.is_empty() {
            project
                .rsplit_once('.')
                .map_or(project.clone(), |(s, _)| s.to_string())
        } else {
            title
        };
    }
    ProjectRead {
        format: ProjectFormat::WallsWpj,
        survey,
        files,
    }
}

// ---- Therion thconfig ----------------------------------------------------------------------

fn read_thconfig(path: &str, content: &str, read: &dyn Fn(&str) -> Option<String>) -> ProjectRead {
    let dir = dir_of(path);
    let project = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
    let mut notes = Survey::default();
    let mut parts = Vec::new();
    let mut files = Vec::new();
    for raw in split_lines(content) {
        let line = trim(raw.split('#').next().unwrap_or(""));
        let tokens = therion::tokenize(line);
        if tokens.len() < 2 || !tokens[0].eq_ignore_ascii_case("source") {
            continue;
        }
        for name in &tokens[1..] {
            let full = join(&dir, name);
            let found = read(&full).map(|t| (full.clone(), t)).or_else(|| {
                let th = format!("{full}.th");
                read(&th).map(|t| (th, t))
            });
            match found {
                Some((p, text)) => {
                    parts.push(therion::parse_with_inputs(&text, &p, read));
                    files.push(p);
                }
                None => missing(&mut notes, &project, name),
            }
        }
    }
    ProjectRead {
        format: ProjectFormat::Thconfig,
        survey: merge(parts, notes),
        files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn reader(files: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = files
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |p: &str| map.get(p).cloned()
    }

    const DAT_A: &str = "CAVE\r\nSURVEY NAME: A\r\nSURVEY DATE: 7 10 2024  COMMENT:\r\nSURVEY TEAM:\r\nAnn\r\nDECLINATION: 0.00  FORMAT: DDDDLRUDLADNF\r\n\r\nFROM TO LENGTH BEARING INC LEFT UP DOWN RIGHT\r\n\r\nA1 A2 10.00 90.00 0.00 1 1 1 1\r\n\u{0C}";
    const DAT_B: &str = "CAVE\r\nSURVEY NAME: B\r\nSURVEY DATE: 7 11 2024  COMMENT:\r\nSURVEY TEAM:\r\nBob\r\nDECLINATION: 0.00  FORMAT: DDDDLRUDLADNF\r\n\r\nFROM TO LENGTH BEARING INC LEFT UP DOWN RIGHT\r\n\r\nA2 B1 20.00 0.00 0.00 1 1 1 1\r\n\u{0C}";

    #[test]
    fn a_compass_project_reads_every_file_and_its_fixes() {
        let mak = "/ a comment line\n@500000.000,4250000.000,300.000,16,0.000;\n&North American 1983;\n[Folder-1;\n#a.dat,A1[M 500000.0 4250000.0 300.0]; /inline/ \n#sub\\b.dat,\n  A2;\n];\n";
        let r = read_project(
            ProjectFormat::CompassMak,
            "proj/cave.mak",
            mak,
            &reader(&[("proj/a.dat", DAT_A), ("proj/sub/b.dat", DAT_B)]),
        );
        assert_eq!(r.files, ["proj/a.dat", "proj/sub/b.dat"]);
        let s = &r.survey;
        assert_eq!(s.shots.len(), 2);
        assert_eq!(s.trips.len(), 2);
        assert_eq!(s.shots[1].trip, 1);
        let f = s.fixed.get("A1").unwrap();
        assert!(
            (f.z.unwrap() - 300.0 * crate::FEET_PER_METER).abs() < 1e-9,
            "metres become feet"
        );
        assert!(s.findings().is_empty());
    }

    #[test]
    fn a_missing_file_is_said_not_skipped() {
        let r = read_project(
            ProjectFormat::CompassMak,
            "cave.mak",
            "#a.dat;\n#gone.dat;",
            &reader(&[("a.dat", DAT_A)]),
        );
        assert_eq!(r.survey.shots.len(), 1);
        assert_eq!(r.survey.findings().len(), 1);
        assert!(r.survey.findings()[0].message.contains("gone.dat"));
    }

    #[test]
    fn a_walls_project_follows_books_paths_and_options() {
        let wpj = ";WALLS Project file\n.BOOK\tMy Cave\n.NAME\tPROJ\n.OPTIONS\tFeet\n.STATUS\t3\n.SURVEY\tEntrance\n.NAME\tENT\n.STATUS\t8\n.BOOK\tLower\n.PATH\tlower\n.OPTIONS\tDecl=2\n.SURVEY\tDeep\n.NAME\tDEEP\n.ENDBOOK\n.ENDBOOK\n";
        let ent = "A1 A2 10 90 0\n";
        let deep = "A2 A3 10 0 0\n";
        let r = read_project(
            ProjectFormat::WallsWpj,
            "w/cave.wpj",
            wpj,
            &reader(&[("w/ENT.SRV", ent), ("w/lower/DEEP.SRV", deep)]),
        );
        assert_eq!(r.files, ["w/ENT.SRV", "w/lower/DEEP.SRV"]);
        let s = &r.survey;
        assert_eq!(s.unit(), "ft", "the book's Feet option holds below it");
        assert_eq!(s.cave_name, "My Cave");
        assert_eq!(s.shots.len(), 2);
        assert!(
            (s.shots[1].azimuth - 2.0).abs() < 1e-9,
            "the inner book's Decl applies to its survey"
        );
        assert!(
            (s.shots[0].azimuth - 90.0).abs() < 1e-9,
            "and not to the outer one"
        );
    }

    #[test]
    fn detached_parts_of_a_walls_project_are_left_out() {
        let wpj = ".BOOK\tCave\n.STATUS\t3\n.SURVEY\tA\n.NAME\tA\n.SURVEY\tOld\n.NAME\tOLD\n.STATUS\t2\n.BOOK\tArchive\n.STATUS\t2\n.SURVEY\tX\n.NAME\tX\n.ENDBOOK\n.SURVEY\tPhotos\n.NAME\tPICS\n.STATUS\t65536\n.SURVEY\tB\n.NAME\tB\n.ENDBOOK\n";
        let files = [
            ("A.SRV", "A1 A2 10 0 0\n"),
            ("OLD.SRV", "Z1 Z2 10 0 0\n"),
            ("X.SRV", "X1 X2 1 0 0\n"),
            ("b.srv", "A2 B1 10 90 0\n"),
        ];
        let r = read_project(ProjectFormat::WallsWpj, "cave.wpj", wpj, &reader(&files));
        assert_eq!(
            r.files,
            ["A.SRV", "b.srv"],
            "the root's detached bit is ignored; detached entries are not"
        );
        assert!(
            r.survey
                .findings()
                .iter()
                .any(|f| f.code == "walls-detached")
        );
    }

    #[test]
    fn a_thconfig_reads_its_sources() {
        let cfg = "# config\nsource main.th\nexport map -o cave.pdf\n";
        let main = "survey cave\n  centreline\n    data normal from to length compass clino\n    1 2 5 0 0\n  endcentreline\nendsurvey\n";
        let r = read_project(
            ProjectFormat::Thconfig,
            "t/thconfig",
            cfg,
            &reader(&[("t/main.th", main)]),
        );
        assert_eq!(r.files, ["t/main.th"]);
        assert_eq!(r.survey.shots.len(), 1);
    }

    #[test]
    fn a_backsight_only_shot_survives_every_format() {
        let dat = "CAVE\r\nSURVEY NAME: A\r\nSURVEY DATE: 7 10 2024  COMMENT:\r\nSURVEY TEAM:\r\nAnn\r\nDECLINATION: 2.00  FORMAT: DDDDLRUDLADadBF  CORRECTIONS: 0.00 0.00 0.00\r\n\r\nFROM TO LENGTH BEARING INC LEFT UP DOWN RIGHT AZM2 INC2\r\n\r\nA1 A2 10.00 -999.00 -999.00 1 1 1 1 270.00 -5.00\r\n\u{0C}";
        let s = compass::parse(dat);
        assert!(s.shots[0].azimuth.is_nan() && s.shots[0].inclination.is_nan());
        assert!((s.shots[0].back_azimuth.unwrap() - 272.0).abs() < 1e-9);
        for f in crate::FORMATS {
            let text = f.write(&mut s.clone());
            let back = f.parse(&text);
            assert_eq!(back.shots.len(), 1, "{}: {text}", f.id());
            let b = &back.shots[0];
            // the bearing the shot is placed on: the foresight, else the reversed backsight
            let az = if b.azimuth.is_finite() {
                b.azimuth
            } else {
                b.back_azimuth.unwrap() - 180.0
            };
            let inc = if b.inclination.is_finite() {
                b.inclination
            } else {
                -b.back_inclination.unwrap()
            };
            assert!(
                (az - 92.0).abs() < 0.01,
                "{}: azimuth {az} from {text}",
                f.id()
            );
            assert!((inc - 5.0).abs() < 0.01, "{}: inclination {inc}", f.id());
        }
    }

    #[test]
    fn compass_corrections_are_added_to_the_readings() {
        let dat = "CAVE\r\nSURVEY NAME: A\r\nSURVEY DATE: 7 10 2024  COMMENT:\r\nSURVEY TEAM:\r\nAnn\r\nDECLINATION: 0.00  FORMAT: DDDDLRUDLADadBF  CORRECTIONS: 1.00 2.00 0.50 CORRECTIONS2: -1.0 -2.0\r\n\r\nFROM TO LENGTH BEARING INC LEFT UP DOWN RIGHT AZM2 INC2\r\n\r\nA1 A2 10.00 90.00 3.00 1 1 1 1 270.00 -3.00\r\n\u{0C}";
        let s = &compass::parse(dat).shots[0];
        assert_eq!((s.distance, s.azimuth, s.inclination), (10.5, 91.0, 5.0));
        assert_eq!(
            (s.back_azimuth, s.back_inclination),
            (Some(269.0), Some(-5.0))
        );
    }

    #[test]
    fn projects_are_told_apart() {
        assert_eq!(
            detect_project("x/CAVE.MAK", ""),
            Some(ProjectFormat::CompassMak)
        );
        assert_eq!(
            detect_project("cave.wpj", ""),
            Some(ProjectFormat::WallsWpj)
        );
        assert_eq!(
            detect_project("thconfig", ""),
            Some(ProjectFormat::Thconfig)
        );
        assert_eq!(
            detect_project("p.txt", ".BOOK  Cave\n.NAME X\n"),
            Some(ProjectFormat::WallsWpj)
        );
        assert_eq!(detect_project("cave.dat", "SURVEY NAME: A"), None);
    }

    #[test]
    fn units_convert_everything() {
        let mut s = compass::parse(DAT_A);
        s.fixed.set("A1", Fixed::at(10.0, 20.0, 30.0));
        s.convert_units("m");
        assert_eq!(s.unit(), "m");
        assert!((s.shots[0].distance - 3.048).abs() < 1e-6);
        assert!(
            (s.start_lrud.as_ref().unwrap().left.unwrap() - 0.3048).abs() < 1e-6,
            "the first station's walls too"
        );
        assert!((s.fixed.get("A1").unwrap().z.unwrap() - 9.144).abs() < 1e-6);
        assert_eq!(s.trips[0].distance_unit.as_deref(), Some("m"));
    }
}
