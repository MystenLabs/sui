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
    schema::LocalDepInfo,
};

use derive_where::derive_where;
use path_clean::PathClean;
use serde::{Deserialize, Serialize};
use serde_spanned::Spanned;

use super::Dependency;

impl LocalDepInfo {
    /// The path on the filesystem, relative to the location of the containing file
    pub fn relative_path(&self) -> &PathBuf {
        &self.local
    }

    /// Retrieve the absolute path to [`LocalDependency`] without actually fetching it.
    pub fn absolute_path(&self, containing_file: impl AsRef<Path>) -> PathBuf {
        // TODO: handle panic with a proper error.
        containing_file
            .as_ref()
            .parent()
            .expect("non-directory files have parents")
            .join(&self.local)
            .clean()
    }
}
