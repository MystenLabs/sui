use fs4::fs_std::FileExt;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, error};

use crate::git::get_cache_path;
use crate::logging::user_error;

#[derive(Debug, Error)]
pub enum LockError {
    #[error(
        "Unexpected error acquiring lock for package at {package} (lock file: `{lock}`): {source}"
    )]
    PackageLockError {
        package: PathBuf,
        lock: PathBuf,
        source: std::io::Error,
    },

    #[error("Unexpected error acquiring lock for {name} cache (path: `{path}`): {source}")]
    CacheLockError {
        name: String,
        path: PathBuf,
        source: std::io::Error,
    },
}

pub type LockResult<T> = Result<T, LockError>;

#[derive(Debug)]
pub struct PackageSystemLock {
    file: File,
}

impl PackageSystemLock {
    /// Acquire a lock for doing git operations sequentially
    pub fn new_for_git(repo_id: &str) -> LockResult<Self> {
        let path = cache_path_for(repo_id)?;
        Self::new_for_path(&path, true).map_err(|source| LockError::CacheLockError {
            name: repo_id.to_string(),
            path,
            source,
        })
    }

    /// Acquire a lock corresponding to the package contained in the directory `path`
    /// We do sequential operations per package (we acquire lock per package path).
    pub fn new_for_project(path: &Path) -> LockResult<Self> {
        let project_lock_path = cache_path_for(digest_path(path).as_str())
            .expect("failed to get git cache folder lock");
        Self::new_for_path(&project_lock_path, true).map_err(|source| LockError::PackageLockError {
            package: path.to_path_buf(),
            lock: project_lock_path,
            source,
        })
    }

    pub fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    fn new_for_path(path: &Path, should_truncate: bool) -> std::io::Result<Self> {
        debug!("acquiring lock for {path:?}");
        let lock = OpenOptions::new()
            .truncate(should_truncate)
            .write(true)
            .read(true)
            .create(true)
            .open(&path)?;

        lock.lock_exclusive()?;
        Ok(Self { file: lock })
    }
}

impl Drop for PackageSystemLock {
    fn drop(&mut self) {
        if let Err(err) = fs4::fs_std::FileExt::unlock(&self.file) {
            user_error!(
                "Failed to release filesystem lock at {:?}: {err:?}",
                self.file
            );
        }
    }
}

fn cache_path_for(name: &str) -> LockResult<PathBuf> {
    let cache_path = PathBuf::from(get_cache_path());
    let project_lock_path = cache_path.join(format!(".{name}.lock"));

    // create dir if not exists.
    std::fs::create_dir_all(&cache_path).map_err(|source| LockError::CacheLockError {
        name: name.to_string(),
        path: project_lock_path.clone(),
        source,
    })?;

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
