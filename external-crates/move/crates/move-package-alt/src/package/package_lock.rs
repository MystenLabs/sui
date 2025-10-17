use fs4::fs_std::FileExt;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use crate::git::get_cache_path;

pub struct PackageSystemLock {
    file: File,
}

impl PackageSystemLock {
    pub fn new_for_file(path: PathBuf) -> anyhow::Result<Self> {
        Self::new_for_path(path, false)
    }

    /// Acquire a lock for doing git operations sequentially
    pub fn new_for_git() -> anyhow::Result<Self> {
        let path = cache_path_for("git").expect("failed to get git cache folder lock");
        Self::new_for_path(path, true)
    }

    /// We do sequential operations per package (we acquire lock per package path).
    pub fn new_for_project(path: &Path) -> anyhow::Result<Self> {
        let project_lock_path = cache_path_for(digest_path(path).as_str())
            .expect("failed to get git cache folder lock");
        Self::new_for_path(project_lock_path, true)
    }

    pub fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    fn new_for_path(path: PathBuf, should_truncate: bool) -> anyhow::Result<Self> {
        let lock = OpenOptions::new()
            .truncate(should_truncate)
            .write(true)
            .read(true)
            .create(true)
            .open(&path)?;

        lock.lock_exclusive()?;
        Ok(Self { file: lock })
    }

    // drop the package system lock.
    pub fn drop(self) -> anyhow::Result<()> {
        fs4::fs_std::FileExt::unlock(&self.file)?;
        Ok(())
    }
}

fn cache_path_for(name: &str) -> anyhow::Result<PathBuf> {
    let cache_path = get_cache_path();
    let cache_path = Path::new(cache_path);
    // create dir if not exists.
    std::fs::create_dir_all(cache_path)?;

    let project_lock_path = cache_path.join(format!(".{name}.lock"));

    Ok(project_lock_path)
}

fn digest_path(path: &Path) -> String {
    let mut hasher = Sha256::new();
    // Convert the path to a string safely
    hasher.update(path.to_string_lossy().as_bytes());
    let result = hasher.finalize();
    // Return hex representation
    format!("{:x}", result)
}
