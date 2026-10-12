//! Reading a scans folder from disk: what is in it, one page's bytes, and the one rule that keeps a
//! request inside the folder the person chose.

use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Pictures and PDFs: what the Scans panel can show.
const EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "svg", "avif", "pdf",
];

/// Most pages one folder may hold (a guard against pointing at the whole disk).
pub const MAX_FILES: usize = 20_000;

/// One page in the folder.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// Path from the folder, with `/` between folders on every system.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
}

fn wanted(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Every page under `root`, at any depth, in path order. Hidden files and folders (a leading dot) and
/// anything that is not a picture or PDF are left out.
pub fn list(root: &Path) -> io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    walk(root, "", &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<Entry>) -> io::Result<()> {
    for item in fs::read_dir(dir)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let kind = item.file_type()?;
        let path = format!("{prefix}{name}");
        if kind.is_dir() {
            walk(&item.path(), &format!("{path}/"), out)?;
        } else if kind.is_file() && wanted(&name) {
            if out.len() >= MAX_FILES {
                return Err(io::Error::other("too many files in that folder"));
            }
            out.push(Entry {
                path,
                size: item.metadata()?.len(),
            });
        }
    }
    Ok(())
}

/// `root` joined with a relative path, but only when the result stays inside `root`: no absolute
/// paths, no `..`, nothing empty. `None` otherwise. (An empty `rel` is the folder itself.)
pub fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.starts_with(['/', '\\']) || rel.as_bytes().get(1) == Some(&b':') {
        return None; // an absolute path or a drive: not inside the folder
    }
    let mut path = root.to_path_buf();
    for part in rel.split(['/', '\\']).filter(|p| !p.is_empty()) {
        let comp = Path::new(part);
        let mut parts = comp.components();
        match (parts.next(), parts.next()) {
            (Some(Component::Normal(_)), None) => path.push(part),
            _ => return None, // `..`, a drive, a root: not a plain name
        }
    }
    Some(path)
}

/// One page's bytes.
pub fn read(root: &Path, rel: &str) -> io::Result<Vec<u8>> {
    let path = safe_join(root, rel)
        .ok_or_else(|| io::Error::other("that path is outside the scans folder"))?;
    if !wanted(rel) {
        return Err(io::Error::other("that is not a scan"));
    }
    fs::read(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> PathBuf {
        let root = std::env::temp_dir().join(format!("cavecad-files-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("2024 Scans/Team A")).unwrap();
        fs::create_dir_all(root.join(".hidden")).unwrap();
        fs::write(root.join("cover.JPG"), b"jpg").unwrap();
        fs::write(root.join("notes.txt"), b"no").unwrap();
        fs::write(root.join(".DS_Store"), b"no").unwrap();
        fs::write(root.join(".hidden/p.png"), b"no").unwrap();
        fs::write(root.join("2024 Scans/p2.png"), b"22").unwrap();
        fs::write(root.join("2024 Scans/Team A/p1.pdf"), b"pdf!").unwrap();
        root
    }

    #[test]
    fn lists_pictures_and_pdfs_at_every_depth_with_slashes() {
        let root = tree();
        let got: Vec<_> = list(&root)
            .unwrap()
            .into_iter()
            .map(|e| (e.path, e.size))
            .collect();
        assert_eq!(
            got,
            [
                ("2024 Scans/Team A/p1.pdf".to_string(), 4),
                ("2024 Scans/p2.png".to_string(), 2),
                ("cover.JPG".to_string(), 3),
            ]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_request_can_never_leave_the_folder() {
        let root = Path::new("/scans");
        assert_eq!(
            safe_join(root, "a/b.png"),
            Some(PathBuf::from("/scans/a/b.png"))
        );
        assert_eq!(safe_join(root, ""), Some(PathBuf::from("/scans")));
        assert_eq!(
            safe_join(root, "a\\b.png"),
            Some(PathBuf::from("/scans/a/b.png"))
        );
        for bad in [
            "../x.png",
            "a/../../x.png",
            "/etc/passwd",
            "a/./b.png",
            "..",
        ] {
            assert_eq!(safe_join(root, bad), None, "{bad}");
        }
    }

    #[test]
    fn reads_a_page_and_refuses_anything_else() {
        let root = tree();
        assert_eq!(read(&root, "2024 Scans/p2.png").unwrap(), b"22");
        assert!(read(&root, "notes.txt").is_err(), "not a scan");
        assert!(read(&root, "../outside.png").is_err(), "outside the folder");
        assert!(read(&root, "missing.png").is_err());
        let _ = fs::remove_dir_all(root);
    }
}
