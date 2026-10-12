//! The storage interface every cave file goes through, plus an in-memory implementation.
//!
//! Local folders, browser-private storage (OPFS), Google Drive, Dropbox and the rest will each
//! implement [`Store`]. The core never touches a real file system, which is what keeps it
//! buildable for the browser. Writes are conditional on a [`Version`] so two writers cannot
//! silently overwrite each other.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

/// A validated, `/`-separated path inside a store. No empty parts, no `.` or `..`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StorePath(String);

impl StorePath {
    /// Parses and validates a path such as `trips/t1/survey.json`.
    pub fn new(s: &str) -> Result<Self, StoreError> {
        let bad = s.is_empty()
            || s.starts_with('/')
            || s.ends_with('/')
            || s.contains('\\')
            || s.contains('\0')
            || s.split('/').any(|p| p.is_empty() || p == "." || p == "..");
        if bad {
            return Err(StoreError::InvalidPath(s.to_string()));
        }
        Ok(StorePath(s.to_string()))
    }

    /// The path as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The parent folder, or `None` for a top-level entry.
    pub fn parent(&self) -> Option<&str> {
        self.0.rfind('/').map(|i| &self.0[..i])
    }
}

impl fmt::Debug for StorePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StorePath({})", self.0)
    }
}

/// An opaque tag that changes whenever a file's content changes (an ETag, a revision id).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Version(pub String);

/// What a write or delete expects to find.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Expect {
    /// Overwrite whatever is there.
    Any,
    /// The file must not exist yet.
    Missing,
    /// The file must still be at this version.
    Version(Version),
}

/// Size and version of one file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Stat {
    /// Size in bytes.
    pub size: u64,
    /// Current version tag.
    pub version: Version,
}

/// Things a store can fail with.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum StoreError {
    /// No file at that path.
    NotFound,
    /// The expectation failed; carries the version actually found (`None` if missing).
    Conflict(Option<Version>),
    /// The path was not valid.
    InvalidPath(String),
    /// A provider-specific failure (network, permission, quota).
    Provider(String),
}

/// A place cave files live. All methods are async because cloud and browser stores are.
#[allow(async_fn_in_trait)]
pub trait Store {
    /// Paths of the files directly or indirectly under `dir` (`""` lists everything).
    async fn list(&self, dir: &str) -> Result<Vec<StorePath>, StoreError>;
    /// Reads a file and the version it was at.
    async fn read(&self, path: &StorePath) -> Result<(Vec<u8>, Version), StoreError>;
    /// Writes a file if `expect` holds, returning the new version.
    async fn write(
        &self,
        path: &StorePath,
        data: &[u8],
        expect: Expect,
    ) -> Result<Version, StoreError>;
    /// Deletes a file if `expect` holds.
    async fn delete(&self, path: &StorePath, expect: Expect) -> Result<(), StoreError>;
    /// Size and version without reading the content.
    async fn stat(&self, path: &StorePath) -> Result<Stat, StoreError>;
}

/// A store held in memory. Used by tests and as the scratch store before a real one is chosen.
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<MemInner>,
}

#[derive(Default)]
struct MemInner {
    files: BTreeMap<StorePath, (Vec<u8>, u64)>,
    clock: u64,
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    fn check(current: Option<&(Vec<u8>, u64)>, expect: &Expect) -> Result<(), StoreError> {
        let found = current.map(|(_, v)| Version(v.to_string()));
        let ok = match expect {
            Expect::Any => true,
            Expect::Missing => found.is_none(),
            Expect::Version(v) => found.as_ref() == Some(v),
        };
        if ok {
            Ok(())
        } else {
            Err(StoreError::Conflict(found))
        }
    }
}

impl Store for MemoryStore {
    async fn list(&self, dir: &str) -> Result<Vec<StorePath>, StoreError> {
        let inner = self.inner.lock().expect("store lock");
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        Ok(inner
            .files
            .keys()
            .filter(|p| p.as_str().starts_with(&prefix))
            .cloned()
            .collect())
    }

    async fn read(&self, path: &StorePath) -> Result<(Vec<u8>, Version), StoreError> {
        let inner = self.inner.lock().expect("store lock");
        let (data, v) = inner.files.get(path).ok_or(StoreError::NotFound)?;
        Ok((data.clone(), Version(v.to_string())))
    }

    async fn write(
        &self,
        path: &StorePath,
        data: &[u8],
        expect: Expect,
    ) -> Result<Version, StoreError> {
        let mut inner = self.inner.lock().expect("store lock");
        Self::check(inner.files.get(path), &expect)?;
        inner.clock += 1;
        let v = inner.clock;
        inner.files.insert(path.clone(), (data.to_vec(), v));
        Ok(Version(v.to_string()))
    }

    async fn delete(&self, path: &StorePath, expect: Expect) -> Result<(), StoreError> {
        let mut inner = self.inner.lock().expect("store lock");
        if !inner.files.contains_key(path) {
            return Err(StoreError::NotFound);
        }
        Self::check(inner.files.get(path), &expect)?;
        inner.files.remove(path);
        Ok(())
    }

    async fn stat(&self, path: &StorePath) -> Result<Stat, StoreError> {
        let inner = self.inner.lock().expect("store lock");
        let (data, v) = inner.files.get(path).ok_or(StoreError::NotFound)?;
        Ok(Stat {
            size: data.len() as u64,
            version: Version(v.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pollster::block_on;

    fn p(s: &str) -> StorePath {
        StorePath::new(s).unwrap()
    }

    #[test]
    fn path_validation() {
        for bad in ["", "/a", "a/", "a//b", "a/../b", "./a", "a\\b"] {
            assert!(StorePath::new(bad).is_err(), "{bad}");
        }
        assert_eq!(p("a/b/c.json").parent(), Some("a/b"));
        assert_eq!(p("c.json").parent(), None);
    }

    #[test]
    fn write_read_round_trip() {
        let s = MemoryStore::new();
        let v = block_on(s.write(&p("a.txt"), b"hi", Expect::Missing)).unwrap();
        assert_eq!(block_on(s.read(&p("a.txt"))).unwrap(), (b"hi".to_vec(), v));
    }

    #[test]
    fn stale_write_conflicts() {
        let s = MemoryStore::new();
        let v1 = block_on(s.write(&p("a"), b"1", Expect::Missing)).unwrap();
        let v2 = block_on(s.write(&p("a"), b"2", Expect::Version(v1.clone()))).unwrap();
        let err = block_on(s.write(&p("a"), b"3", Expect::Version(v1))).unwrap_err();
        assert_eq!(err, StoreError::Conflict(Some(v2)));
        assert_eq!(block_on(s.read(&p("a"))).unwrap().0, b"2");
    }

    #[test]
    fn missing_expectation_fails_on_existing_file() {
        let s = MemoryStore::new();
        block_on(s.write(&p("a"), b"1", Expect::Any)).unwrap();
        assert!(matches!(
            block_on(s.write(&p("a"), b"2", Expect::Missing)),
            Err(StoreError::Conflict(Some(_)))
        ));
    }

    #[test]
    fn list_filters_by_folder() {
        let s = MemoryStore::new();
        for f in ["t1/a", "t1/b", "t2/a", "top"] {
            block_on(s.write(&p(f), b"", Expect::Any)).unwrap();
        }
        assert_eq!(block_on(s.list("t1")).unwrap().len(), 2);
        assert_eq!(block_on(s.list("")).unwrap().len(), 4);
    }

    #[test]
    fn delete_and_stat() {
        let s = MemoryStore::new();
        block_on(s.write(&p("a"), b"abc", Expect::Any)).unwrap();
        assert_eq!(block_on(s.stat(&p("a"))).unwrap().size, 3);
        block_on(s.delete(&p("a"), Expect::Any)).unwrap();
        assert_eq!(block_on(s.stat(&p("a"))).unwrap_err(), StoreError::NotFound);
        assert_eq!(
            block_on(s.delete(&p("a"), Expect::Any)).unwrap_err(),
            StoreError::NotFound
        );
    }
}
