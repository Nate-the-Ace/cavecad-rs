//! A cave's project folder on disk: making it with its standard subfolders, writing the cave file
//! into it (taking a dated backup first, and thinning old ones), reading it back, and writing an
//! export into `exports/`. The names and rules come from `cave-folder`; this file only touches disk.

use cave_folder::{BACKUP, CACHE, Place, Stamp};
use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::files::safe_join;

/// What is under a chosen place, for the New cave dialog.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Looked {
    /// The cave's folder (it may not exist yet).
    pub folder: String,
    /// What it holds.
    pub place: Place,
}

/// The names directly inside a folder (hidden ones too), or `None` when there is no folder.
fn names_in(folder: &Path) -> io::Result<Option<Vec<String>>> {
    if !folder.is_dir() {
        return Ok(None);
    }
    let mut out = Vec::new();
    for item in fs::read_dir(folder)? {
        out.push(item?.file_name().to_string_lossy().into_owned());
    }
    out.sort();
    Ok(Some(out))
}

fn refs(names: &[String]) -> Vec<&str> {
    names.iter().map(String::as_str).collect()
}

/// A plain file name (no folders, nothing hidden), or an error saying why not.
fn plain(name: &str) -> io::Result<&str> {
    let ok = !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && name.as_bytes().get(1) != Some(&b':');
    if ok {
        Ok(name)
    } else {
        Err(io::Error::other("that is not a plain file name"))
    }
}

/// What the cave folder for `name` under `parent` would be, and what is there already.
pub fn look(parent: &Path, name: &str) -> io::Result<Looked> {
    let folder = parent.join(cave_folder::folder_name(name));
    let names = names_in(&folder)?;
    Ok(Looked {
        folder: folder.to_string_lossy().into_owned(),
        place: cave_folder::place(names.as_ref().map(|n| refs(n)).as_deref()),
    })
}

/// Makes the cave folder for `name` under `parent` (or uses it when it is already there) and adds
/// the standard subfolders it is missing, plus the hidden cache. Nothing already in it is touched.
/// Gives back the folder and the subfolders made.
pub fn make(parent: &Path, name: &str) -> io::Result<(PathBuf, Vec<String>)> {
    if !parent.is_dir() {
        return Err(io::Error::other("that place is not a folder"));
    }
    let folder = parent.join(cave_folder::folder_name(name));
    fs::create_dir_all(&folder)?;
    let made = ensure(&folder)?;
    Ok((folder, made))
}

/// Adds the standard subfolders (and the hidden cache) a cave folder is missing.
pub fn ensure(folder: &Path) -> io::Result<Vec<String>> {
    let names = names_in(folder)?.unwrap_or_default();
    let mut made = Vec::new();
    for sub in cave_folder::missing(&refs(&names)) {
        fs::create_dir(folder.join(sub))?;
        made.push(sub.to_string());
    }
    if cave_folder::find(&refs(&names), CACHE).is_none() {
        fs::create_dir(folder.join(CACHE))?;
    }
    Ok(made)
}

/// Writes the cave file into its folder. Before an older copy is replaced it is copied into
/// `backup/` (at most once in ten minutes), old backups are thinned, and the new bytes go in
/// through a temporary file so a crash never leaves half a cave.
pub fn write(folder: &Path, file: &str, bytes: &[u8], now: Stamp) -> io::Result<()> {
    let file = plain(file)?;
    if !cave_folder::is_cave_file(file) {
        return Err(io::Error::other("a cave file ends in .cavecad"));
    }
    if !folder.is_dir() {
        return Err(io::Error::other("the cave's folder is not there"));
    }
    let target = folder.join(file);
    if target.is_file() {
        backup(folder, file, &target, now)?;
    }
    let temp = folder.join(format!(".{file}.saving"));
    fs::write(&temp, bytes)?;
    fs::rename(&temp, &target)
}

fn backup(folder: &Path, file: &str, target: &Path, now: Stamp) -> io::Result<()> {
    let stem = &file[..file.len() - cave_folder::EXT.len()];
    let top = names_in(folder)?.unwrap_or_default();
    let dir = folder.join(cave_folder::find(&refs(&top), BACKUP).unwrap_or(BACKUP));
    fs::create_dir_all(&dir)?;
    let names = names_in(&dir)?.unwrap_or_default();
    if cave_folder::backup_due(stem, &refs(&names), now) {
        fs::copy(target, dir.join(cave_folder::backup_name(stem, now)))?;
    }
    let names = names_in(&dir)?.unwrap_or_default();
    for gone in cave_folder::prune(stem, &refs(&names)) {
        fs::remove_file(dir.join(gone))?; // only ever this cave's own backups, inside backup/
    }
    Ok(())
}

/// The cave file's bytes.
pub fn read(folder: &Path, file: &str) -> io::Result<Vec<u8>> {
    let file = plain(file)?;
    if !cave_folder::is_cave_file(file) {
        return Err(io::Error::other("a cave file ends in .cavecad"));
    }
    fs::read(folder.join(file))
}

/// Writes an export into the cave folder's `exports/` (an old cave's `PDF/`), making it when it is
/// missing. Gives back where it went.
pub fn write_export(folder: &Path, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    let name = plain(name)?;
    if !folder.is_dir() {
        return Err(io::Error::other("the cave's folder is not there"));
    }
    let top = names_in(folder)?.unwrap_or_default();
    let dir = safe_join(folder, &cave_folder::exports_folder(&refs(&top)))
        .ok_or_else(|| io::Error::other("bad exports folder"))?;
    fs::create_dir_all(&dir)?;
    let path = dir.join(name);
    fs::write(&path, bytes)?;
    Ok(path)
}

/// Undoes `%xx` escapes (headers carry folder and file names this way, as `encodeURIComponent`
/// writes them).
pub fn unescape(text: &str) -> Option<String> {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("cavecad-folder-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn at(text: &str) -> Stamp {
        Stamp::parse(text).unwrap()
    }

    #[test]
    fn makes_the_folder_with_every_subfolder() {
        let root = temp("make");
        assert_eq!(look(&root, "Truitt: Cave").unwrap().place, Place::Empty);
        let (folder, made) = make(&root, "Truitt: Cave").unwrap();
        assert_eq!(folder, root.join("Truitt Cave"));
        assert_eq!(made, cave_folder::SUBFOLDERS.map(String::from).to_vec());
        for sub in cave_folder::SUBFOLDERS {
            assert!(folder.join(sub).is_dir(), "{sub}");
        }
        assert!(folder.join(CACHE).is_dir());
        assert_eq!(look(&root, "Truitt Cave").unwrap().place, Place::NotACave);
        assert!(
            make(&root.join("missing"), "X").is_err(),
            "the place must exist"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_existing_folder_keeps_what_it_has() {
        let root = temp("existing");
        let folder = root.join("Old Pit");
        fs::create_dir_all(folder.join("Scans")).unwrap();
        fs::create_dir_all(folder.join("PDF")).unwrap();
        fs::write(folder.join("Scans/p1.png"), b"page").unwrap();
        fs::write(folder.join("Old Pit.cavecad"), b"cave").unwrap();
        assert_eq!(
            look(&root, "Old Pit").unwrap().place,
            Place::HasCave {
                file: "Old Pit.cavecad".into()
            }
        );
        let (_, made) = make(&root, "Old Pit").unwrap();
        assert!(!made.contains(&"scans".to_string()) && !made.contains(&"exports".to_string()));
        assert_eq!(fs::read(folder.join("Scans/p1.png")).unwrap(), b"page");
        assert_eq!(fs::read(folder.join("Old Pit.cavecad")).unwrap(), b"cave");
        let out = write_export(&folder, "Old Pit.dxf", b"dxf").unwrap();
        assert_eq!(
            out,
            folder.join("PDF/Old Pit.dxf"),
            "an old cave's PDF folder takes exports"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn saving_backs_up_the_last_copy_once_in_ten_minutes() {
        let root = temp("save");
        let (folder, _) = make(&root, "C").unwrap();
        write(&folder, "C.cavecad", b"one", at("2026-10-26 1400")).unwrap();
        assert_eq!(
            names_in(&folder.join(BACKUP)).unwrap().unwrap().len(),
            0,
            "nothing to back up yet"
        );
        write(&folder, "C.cavecad", b"two", at("2026-10-26 1401")).unwrap();
        write(&folder, "C.cavecad", b"three", at("2026-10-26 1405")).unwrap();
        write(&folder, "C.cavecad", b"four", at("2026-10-26 1412")).unwrap();
        assert_eq!(fs::read(folder.join("C.cavecad")).unwrap(), b"four");
        assert_eq!(
            names_in(&folder.join(BACKUP)).unwrap().unwrap(),
            ["C 2026-10-26 1401.cavecad", "C 2026-10-26 1412.cavecad"]
        );
        assert_eq!(
            fs::read(folder.join("backup/C 2026-10-26 1401.cavecad")).unwrap(),
            b"one"
        );
        assert_eq!(
            fs::read(folder.join("backup/C 2026-10-26 1412.cavecad")).unwrap(),
            b"three"
        );
        assert!(!folder.join(".C.cavecad.saving").exists());
        assert_eq!(read(&folder, "C.cavecad").unwrap(), b"four");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn old_backups_are_thinned_and_nothing_else_goes() {
        let root = temp("prune");
        let (folder, _) = make(&root, "C").unwrap();
        fs::write(folder.join("backup/notes.txt"), b"mine").unwrap();
        for i in 0..32u32 {
            let s = Stamp {
                year: 2026,
                month: 10,
                day: 1 + i / 20,
                hour: i % 20,
                minute: 0,
            };
            fs::write(
                folder.join("backup").join(cave_folder::backup_name("C", s)),
                b"x",
            )
            .unwrap();
        }
        fs::write(folder.join("C.cavecad"), b"now").unwrap();
        write(&folder, "C.cavecad", b"next", at("2026-10-26 1200")).unwrap();
        let left = names_in(&folder.join(BACKUP)).unwrap().unwrap();
        assert!(left.contains(&"notes.txt".to_string()));
        // 33 backups: the newest 30 stay; the 3 older all fall on 2026-10-01 with kept ones
        assert_eq!(left.len(), 30 + 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_names_that_leave_the_folder() {
        let root = temp("names");
        let (folder, _) = make(&root, "C").unwrap();
        for bad in ["../C.cavecad", "a/C.cavecad", ".C.cavecad", "C.txt", ""] {
            assert!(
                write(&folder, bad, b"x", at("2026-10-26 1200")).is_err(),
                "{bad}"
            );
        }
        for bad in ["../x.dxf", "sub/x.dxf", ""] {
            assert!(write_export(&folder, bad, b"x").is_err(), "{bad}");
        }
        assert!(write(&root.join("gone"), "C.cavecad", b"x", at("2026-10-26 1200")).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unescapes_header_names() {
        assert_eq!(
            unescape("Truitt%20Cave%2F%C3%A9").as_deref(),
            Some("Truitt Cave/é")
        );
        assert_eq!(unescape("plain").as_deref(), Some("plain"));
        assert_eq!(unescape("bad%2"), None);
        assert_eq!(unescape("bad%zz"), None);
    }
}
