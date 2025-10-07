use fs4::fs_std::FileExt;
use std::fs::{File, OpenOptions};
use std::path::Path;

use crate::git::get_cache_path;

pub struct PackageSystemLock {
    _file: File,
}

impl PackageSystemLock {
    pub fn new() -> anyhow::Result<Self> {
        // TODO: This should be fixed to always return a file, even for `cfg(test)` cases.
        let file = global_git_cache_folder_lock().expect("failed to get git cache folder lock");
        file.lock_exclusive()?;
        Ok(Self { _file: file })
    }

    // drop the package system lokc.
    pub fn drop(self) -> anyhow::Result<()> {
        fs4::fs_std::FileExt::unlock(&self._file)?;
        Ok(())
    }
}

fn global_git_cache_folder_lock() -> anyhow::Result<File> {
    let cache_path = get_cache_path();
    let cache_path = Path::new(cache_path);
    // create dir if not exsits.
    std::fs::create_dir_all(cache_path)?;
    let project_lock = cache_path.join("lock");

    let git_cache_folder_lock_file = OpenOptions::new()
        .truncate(true)
        .write(true)
        .read(true)
        .create(true)
        .open(&project_lock)?;

    Ok(git_cache_folder_lock_file)
}
