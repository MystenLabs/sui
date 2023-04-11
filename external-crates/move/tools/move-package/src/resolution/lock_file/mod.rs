// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs::{self, File},
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use tempfile::NamedTempFile;

pub mod schema;

use crate::compilation::package_layout::CompiledPackageLayout;

/// Representation of a machine-generated, human-readable text file that is generated as part of the
/// build, with the following properties:
///
///  - It is only updated by tooling, during the build.
///  - It will be re-generated on each build.
///  - Its content is stable (running the same build multiple times should result in a lock file
///    with the same content).
///  - It will only be updated if the operation touching it succeeds.
///
/// To support this model, the contents of the lock file is stored in a temporary file in the
/// compiled package output directory, and it must be explicitly committed to its place in the
/// package root on success, consuming the lock file.
///
/// Lock files wrap a `File` which can be accessed by dereferencing it.
#[derive(Debug)]
pub struct LockFile {
    file: NamedTempFile,
}

impl LockFile {
    /// Creates a new lock file in a sub-directory of `install_dir` (the compiled output directory
    /// of a move package).
    pub fn new(install_dir: PathBuf) -> Result<LockFile> {
        let mut locks_dir = install_dir;
        locks_dir.extend([
            CompiledPackageLayout::Root.path(),
            CompiledPackageLayout::LockFiles.path(),
        ]);
        fs::create_dir_all(&locks_dir).context("Creating output directory")?;

        let mut lock = tempfile::Builder::new()
            .prefix("Move.lock")
            .tempfile_in(locks_dir)
            .context("Creating lock file")?;

        schema::write_prologue(&mut lock).context("Initializing lock file")?;

        Ok(LockFile { file: lock })
    }

    /// Consume the lock file, moving it to its final position at `lock_path`.  NOTE: If this
    /// function is not called, the contents of the lock file will be discarded.
    pub fn commit(self, lock_path: impl AsRef<Path>) -> Result<()> {
        self.file
            .persist(lock_path)
            .context("Committing lock file")?;
        Ok(())
    }
}

impl Deref for LockFile {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        self.file.as_file()
    }
}

impl DerefMut for LockFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.file.as_file_mut()
    }
}
