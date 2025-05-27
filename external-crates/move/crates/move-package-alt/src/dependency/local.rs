// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods related to local dependencies (of the form `{ local = "<path>" }`)

use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use crate::{errors::PackageResult, flavor::MoveFlavor};
use derive_where::derive_where;
use serde::{Deserialize, Serialize};
use serde_spanned::Spanned;

use super::PinnedDependencyInfo;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LocalDependency {
    /// The path on the filesystem, relative to the location of the containing file (which is
    /// stored in the `Located` wrapper)
    local: PathBuf,
}

impl LocalDependency {
    /// The path on the filesystem, relative to the location of the containing file
    pub fn path(&self) -> PackageResult<PathBuf> {
        // TODO incorrect, we need a base path
        self.local.canonicalize().map_err(|e| {
            crate::errors::PackageError::Generic(format!(
                "Failed to canonicalize path {}: {}",
                self.local.display(),
                e
            ))
        })
    }

    /// The path on the filesystem, relative to the location of the containing file
    pub fn root_dependency() -> Self {
        Self {
            local: PathBuf::from("."),
        }
    }
}
