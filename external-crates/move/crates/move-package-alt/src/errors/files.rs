use std::{
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};

use append_only_vec::AppendOnlyVec;

use super::PackageResult;

/// A collection that holds a file path and its contents. This is used for when parsing the
/// manifest and lockfiles to enable nice reporting throughout the system.
static FILES: AppendOnlyVec<(PathBuf, String)> = AppendOnlyVec::new();

/// A cheap handle into a global collection of files
#[derive(Copy, Clone)]
pub struct FileHandle {
    /// Invariant: guaranteed to be in FILES
    id: usize,
}

impl FileHandle {
    /// Reads the file located at [path] into the file cache and returns its ID
    pub fn new(path: PathBuf) -> PackageResult<Self> {
        // SAFETY: only fails if lock is poisoned which indicates another thread has panicked
        // already
        let name = path.to_string_lossy().to_string();
        let source = fs::read_to_string(&path)?;

        let id = FILES.push((path, source));
        Ok(Self { id })
    }

    /// Return the path to the file at [id]
    pub fn path(&self) -> &'static Path {
        &FILES[self.id].0
    }

    /// Return the source code for the file at [id]
    pub fn source(&self) -> &'static String {
        &FILES[self.id].1
    }
}

impl Debug for FileHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.path().fmt(f)
    }
}
