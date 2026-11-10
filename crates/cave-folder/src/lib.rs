//! A cave's project folder (spec: `docs/specs/2026-10-26-new-cave-folder.md`): the subfolders every
//! cave gets, the folder and file names a cave's name turns into, and when a save takes a backup and
//! which old backups go. Only names and rules live here; the desktop app and the browser each do the
//! reading and writing their own way, and both ask these functions what to do.
//!
//! The `.cavecad` file is the cave. Nothing in a subfolder is needed to open it.

use serde::Serialize;

/// The cave file's extension.
pub const EXT: &str = ".cavecad";

/// The folders every cave folder has, made at once even when empty. Lowercase English, never
/// translated: a folder made in one language must be found by the app running in another.
pub const SUBFOLDERS: [&str; 9] = [
    SCANS, SURVEY, IMAGES, LIDAR, SURFACE, DOCUMENTS, TRIPS, EXPORTS, BACKUP,
];

/// Scanned notebook pages.
pub const SCANS: &str = "scans";
/// Raw instrument downloads and imported survey files, kept as they arrived.
pub const SURVEY: &str = "survey";
/// Photographs.
pub const IMAGES: &str = "images";
/// 3D captures.
pub const LIDAR: &str = "lidar";
/// Aerial imagery, elevation models, GPS tracks.
pub const SURFACE: &str = "surface";
/// Landowner agreements, permits, trip reports, the cave description.
pub const DOCUMENTS: &str = "documents";
/// Trip plan packets and field sheets.
pub const TRIPS: &str = "trips";
/// Where exports go by default.
pub const EXPORTS: &str = "exports";
/// Dated earlier versions of the cave file.
pub const BACKUP: &str = "backup";
/// The hidden folder of things the app can make again (safe to delete).
pub const CACHE: &str = ".cavecad";
/// The old app's export folder: read as `exports/` in a cave folder that has it, never renamed.
pub const OLD_EXPORTS: &str = "PDF";

/// The folders that are location data: left out of a sanitized package unless asked for by name.
pub const LOCATION_DATA: [&str; 6] = [SCANS, SURVEY, IMAGES, LIDAR, SURFACE, DOCUMENTS];

/// The newest backups always kept; older ones are thinned to one a day.
pub const KEEP_RECENT: usize = 30;

/// Shortest gap between two backups, minutes.
pub const BACKUP_GAP_MINUTES: i64 = 10;

/// The folder a cave's name becomes: characters a folder cannot have turn into spaces, runs of
/// spaces shrink to one, and leading or trailing spaces and dots go. `New cave` when nothing is left.
pub fn folder_name(name: &str) -> String {
    let swapped: String = name
        .chars()
        .map(|c| {
            if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let joined = swapped.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = joined.trim_matches(|c: char| c == '.' || c == ' ');
    if trimmed.is_empty() {
        "New cave".into()
    } else {
        trimmed.into()
    }
}

/// The cave file's name inside its folder: `<folder name>.cavecad`.
pub fn cave_file_name(name: &str) -> String {
    format!("{}{EXT}", folder_name(name))
}

/// True for a name that is a cave file.
pub fn is_cave_file(name: &str) -> bool {
    name.len() > EXT.len() && name.to_ascii_lowercase().ends_with(EXT) && !name.starts_with('.')
}

/// The entry in `existing` that is the folder `wanted`, ignoring case (`Scans/` counts as `scans/`).
pub fn find<'a>(existing: &[&'a str], wanted: &str) -> Option<&'a str> {
    existing
        .iter()
        .copied()
        .find(|e| e.eq_ignore_ascii_case(wanted))
}

/// The standard subfolders not yet in `existing` (any casing counts as there), in order. The old
/// app's `PDF/` stands in for `exports/`.
pub fn missing(existing: &[&str]) -> Vec<&'static str> {
    SUBFOLDERS
        .iter()
        .copied()
        .filter(|s| find(existing, s).is_none())
        .filter(|s| !(*s == EXPORTS && find(existing, OLD_EXPORTS).is_some()))
        .collect()
}

/// The folder exports go to: `exports/` (in whatever casing it exists), else an old cave's `PDF/`,
/// else `exports` (to be made).
pub fn exports_folder(existing: &[&str]) -> String {
    find(existing, EXPORTS)
        .or_else(|| find(existing, OLD_EXPORTS))
        .unwrap_or(EXPORTS)
        .to_string()
}

/// What a chosen place holds under the cave's folder name, and so what New cave does there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Place {
    /// Nothing there yet: make the folder.
    Empty,
    /// A folder with a cave file in it: offer to open that cave instead.
    HasCave {
        /// The cave file's name.
        file: String,
    },
    /// A folder with no cave in it: ask before using it, then add only what is missing.
    NotACave,
}

/// Says what a folder that may already exist holds, from the names inside it (`None`: no folder).
pub fn place(inside: Option<&[&str]>) -> Place {
    let Some(names) = inside else {
        return Place::Empty;
    };
    let mut caves: Vec<&str> = names.iter().copied().filter(|n| is_cave_file(n)).collect();
    caves.sort_unstable();
    match caves.first() {
        Some(file) => Place::HasCave {
            file: (*file).into(),
        },
        None => Place::NotACave,
    }
}

/// A moment to the minute, in the person's own time zone (the caller reads the clock).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stamp {
    /// Year.
    pub year: i32,
    /// Month, 1 to 12.
    pub month: u32,
    /// Day, 1 to 31.
    pub day: u32,
    /// Hour, 0 to 23.
    pub hour: u32,
    /// Minute, 0 to 59.
    pub minute: u32,
}

impl Stamp {
    /// Reads `yyyy-mm-dd hhmm`.
    pub fn parse(text: &str) -> Option<Stamp> {
        let b = text.as_bytes();
        if b.len() != 15 || b[4] != b'-' || b[7] != b'-' || b[10] != b' ' {
            return None;
        }
        let num = |r: std::ops::Range<usize>| -> Option<u32> {
            let s = text.get(r)?;
            s.bytes()
                .all(|c| c.is_ascii_digit())
                .then(|| s.parse().ok())?
        };
        let s = Stamp {
            year: num(0..4)? as i32,
            month: num(5..7)?,
            day: num(8..10)?,
            hour: num(11..13)?,
            minute: num(13..15)?,
        };
        let ok = (1..=12).contains(&s.month)
            && (1..=31).contains(&s.day)
            && s.hour < 24
            && s.minute < 60;
        ok.then_some(s)
    }

    /// Writes `yyyy-mm-dd hhmm`.
    pub fn text(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }

    /// Minutes since 1970-01-01 00:00 (for differences only).
    fn minutes(&self) -> i64 {
        // days from the civil date (Howard Hinnant's algorithm)
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        (days * 24 + i64::from(self.hour)) * 60 + i64::from(self.minute)
    }

    /// The calendar day, for keeping one backup a day.
    fn date(&self) -> (i32, u32, u32) {
        (self.year, self.month, self.day)
    }
}

/// A backup's file name: `<stem> yyyy-mm-dd hhmm.cavecad`, where `stem` is the cave file's name
/// without `.cavecad`.
pub fn backup_name(stem: &str, at: Stamp) -> String {
    format!("{stem} {}{EXT}", at.text())
}

/// When a file in `backup/` is one of this cave's backups, the moment it was taken.
pub fn backup_stamp(stem: &str, name: &str) -> Option<Stamp> {
    let rest = name.strip_prefix(stem)?.strip_prefix(' ')?;
    let rest = rest.strip_suffix(EXT)?;
    Stamp::parse(rest)
}

/// Whether a save at `now` takes a backup: when no backup of this cave in `names` (the files in
/// `backup/`) is less than ten minutes old.
pub fn backup_due(stem: &str, names: &[&str], now: Stamp) -> bool {
    let now = now.minutes();
    !names
        .iter()
        .filter_map(|n| backup_stamp(stem, n))
        .any(|s| (now - s.minutes()).abs() < BACKUP_GAP_MINUTES)
}

/// The backups to delete from `names` (the files in `backup/`): the newest thirty stay, and of the
/// older ones the newest of each day stays. Files that are not this cave's backups are never named.
pub fn prune(stem: &str, names: &[&str]) -> Vec<String> {
    let mut mine: Vec<(Stamp, &str)> = names
        .iter()
        .filter_map(|n| backup_stamp(stem, n).map(|s| (s, *n)))
        .collect();
    mine.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1))); // newest first
    let mut days_kept = Vec::new();
    let mut gone = Vec::new();
    for (i, (stamp, name)) in mine.into_iter().enumerate() {
        if i < KEEP_RECENT {
            days_kept.push(stamp.date());
            continue;
        }
        if days_kept.contains(&stamp.date()) {
            gone.push(name.to_string());
        } else {
            days_kept.push(stamp.date());
        }
    }
    gone
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Stamp {
        Stamp::parse(text).unwrap()
    }

    #[test]
    fn names_a_folder_a_disk_can_hold() {
        assert_eq!(folder_name("Truitt Cave"), "Truitt Cave");
        assert_eq!(folder_name("A/B: C?"), "A B C");
        assert_eq!(folder_name("  ..Pit.. "), "Pit");
        assert_eq!(folder_name("<>|"), "New cave");
        assert_eq!(folder_name(""), "New cave");
        assert_eq!(cave_file_name("Big: Room"), "Big Room.cavecad");
    }

    #[test]
    fn finds_subfolders_in_any_casing_and_the_old_pdf_folder() {
        assert_eq!(missing(&[]), SUBFOLDERS.to_vec());
        let old = ["Scans", "PDF", "images", "backup", "lidar"];
        assert_eq!(
            missing(&old),
            ["survey", "surface", "documents", "trips"],
            "PDF stands in for exports"
        );
        assert_eq!(exports_folder(&old), "PDF");
        assert_eq!(exports_folder(&["Exports", "PDF"]), "Exports");
        assert_eq!(exports_folder(&[]), "exports");
        assert_eq!(find(&old, "scans"), Some("Scans"));
    }

    #[test]
    fn says_what_a_chosen_place_holds() {
        assert_eq!(place(None), Place::Empty);
        assert_eq!(place(Some(&["scans", "notes.txt"])), Place::NotACave);
        assert_eq!(
            place(Some(&["b.cavecad", "A.CaveCAD", ".x.cavecad"])),
            Place::HasCave {
                file: "A.CaveCAD".into()
            }
        );
        assert_eq!(
            place(Some(&[".cavecad"])),
            Place::NotACave,
            "the cache folder is not a cave"
        );
    }

    #[test]
    fn stamps_read_and_write_and_refuse_nonsense() {
        assert_eq!(at("2026-10-26 0905").text(), "2026-10-26 0905");
        for bad in [
            "2026-10-26 905",
            "2026-13-01 0000",
            "2026-10-26 2460",
            "2026-1x-26 0000",
            "",
        ] {
            assert_eq!(Stamp::parse(bad), None, "{bad}");
        }
        assert_eq!(
            at("2026-03-01 0000").minutes() - at("2026-02-28 2350").minutes(),
            10
        );
        assert_eq!(
            at("2024-03-01 0000").minutes() - at("2024-02-28 0000").minutes(),
            2 * 24 * 60,
            "leap year"
        );
    }

    #[test]
    fn backs_up_at_most_once_in_ten_minutes() {
        let stem = "Truitt Cave";
        let names = [
            backup_name(stem, at("2026-10-26 1400")),
            "Other Cave 2026-10-26 1409.cavecad".into(),
            "notes.txt".into(),
        ];
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        assert!(!backup_due(stem, &names, at("2026-10-26 1409")));
        assert!(backup_due(stem, &names, at("2026-10-26 1410")));
        assert!(backup_due(stem, &[], at("2026-10-26 1410")));
        assert_eq!(
            backup_stamp(stem, "Truitt Cave 2026-10-26 1400.cavecad"),
            Some(at("2026-10-26 1400"))
        );
        assert_eq!(
            backup_stamp(stem, "Truitt Cave Extra 2026-10-26 1400.cavecad"),
            None
        );
    }

    #[test]
    fn keeps_thirty_newest_then_one_a_day_and_nothing_else_is_touched() {
        let stem = "C";
        let mut names: Vec<String> = Vec::new();
        // 40 backups on 2026-10-20, ten minutes apart, then three on each of two older days
        for i in 0..40 {
            names.push(backup_name(
                stem,
                at(&format!("2026-10-20 {:02}{:02}", i / 6, (i % 6) * 10)),
            ));
        }
        for d in ["2026-10-18", "2026-10-19"] {
            for t in ["0900", "1200", "1500"] {
                names.push(backup_name(stem, at(&format!("{d} {t}"))));
            }
        }
        names.push("keep me.txt".into());
        names.push("Other 2026-01-01 0000.cavecad".into());
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let gone = prune(stem, &refs);
        // the 10 oldest on the 20th share a day with the kept 30; on the 18th and 19th the 1500 stays
        assert_eq!(gone.len(), 10 + 2 + 2);
        assert!(!gone.contains(&"C 2026-10-19 1500.cavecad".to_string()));
        assert!(gone.contains(&"C 2026-10-19 0900.cavecad".to_string()));
        assert!(
            !gone
                .iter()
                .any(|g| g.starts_with("keep") || g.starts_with("Other"))
        );
        assert!(prune(stem, &refs[..5]).is_empty());
    }
}
