// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
//
// Copied and adapted from
// <https://github.com/rust-lang/cargo/tree/master/crates/cargo-test-support/src> at SHA
// 4ac865d3d7b62281ad4dcb92406c816b6f1aeceb

//! Access common paths and manipulate the filesystem

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Path to the test's filesystem scratchpad
pub fn root() -> PathBuf {
    let tempdir = tempfile::tempdir().unwrap();
    tempdir.path().to_path_buf()
}

/// Common path and file operations
pub trait PathExt {
    fn rm_rf(&self);
    fn mkdir_p(&self);

    /// Returns a list of all files and directories underneath the given
    /// directory, recursively, including the starting path.
    fn ls_r(&self) -> Vec<PathBuf>;
}

impl PathExt for Path {
    fn rm_rf(&self) {
        let meta = match self.symlink_metadata() {
            Ok(meta) => meta,
            Err(e) => {
                if e.kind() == ErrorKind::NotFound {
                    return;
                }
                panic!("failed to remove {:?}, could not read: {:?}", self, e);
            }
        };
        // There is a race condition between fetching the metadata and
        // actually performing the removal, but we don't care all that much
        // for our tests.
        if meta.is_dir() {
            if let Err(e) = fs::remove_dir_all(self) {
                panic!("failed to remove {:?}: {:?}", self, e)
            }
        } else if let Err(e) = fs::remove_file(self) {
            panic!("failed to remove {:?}: {:?}", self, e)
        }
    }

    fn mkdir_p(&self) {
        fs::create_dir_all(self)
            .unwrap_or_else(|e| panic!("failed to mkdir_p {}: {}", self.display(), e))
    }

    fn ls_r(&self) -> Vec<PathBuf> {
        walkdir::WalkDir::new(self)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.map(|e| e.path().to_owned()).ok())
            .collect()
    }
}

impl PathExt for PathBuf {
    fn rm_rf(&self) {
        self.as_path().rm_rf()
    }
    fn mkdir_p(&self) {
        self.as_path().mkdir_p()
    }

    fn ls_r(&self) -> Vec<PathBuf> {
        self.as_path().ls_r()
    }
}
