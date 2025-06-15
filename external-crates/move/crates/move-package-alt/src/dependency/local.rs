// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods related to local dependencies (of the form `{ local = "<path>" }`)

use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use crate::{
    errors::{FileHandle, PackageResult, TheFile},
    flavor::MoveFlavor,
    package::paths::PackagePath,
};

use derive_where::derive_where;
use path_clean::PathClean;
use serde::{Deserialize, Serialize};
use serde_spanned::Spanned;

use super::PinnedDependencyInfo;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LocalDependency {
    /// The path on the filesystem, relative to the location of the containing file (which is
    /// stored in the `Located` wrapper)
    local: PathBuf,

    /// This is the directory to which this local dependency is relative to. As this is local
    /// dependency, the directory should be the parent directory that contains this dependency.
    #[serde(skip, default = "TheFile::parent_dir")]
    relative_to_parent_dir: PathBuf,
}

impl LocalDependency {
    /// The path on the filesystem, relative to the location of the containing file
    pub fn relative_path(&self) -> &PathBuf {
        &self.local
    }

    /// Return a local dependency whose local variable is set to '.' (the current directory).
    pub fn root_dependency(path: &PackagePath) -> Self {
        Self {
            local: PathBuf::from("."),
            relative_to_parent_dir: path.path().to_path_buf(),
        }
    }

    /// Retrieve the absolute path to [`LocalDependency`] without actually fetching it.
    pub fn unfetched_path(&self) -> PathBuf {
        // TODO: handle panic with a proper error.
        self.relative_to_parent_dir
            .join(&self.local)
            .canonicalize()
            .unwrap_or_else(|_| {
                panic!(
                    "Failed to canonicalize local dependency path: {}",
                    self.relative_to_parent_dir.join(&self.local).display()
                )
            })
    }
}
