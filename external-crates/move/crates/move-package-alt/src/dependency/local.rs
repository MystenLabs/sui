// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods related to local dependencies (of the form `{ local = "<path>" }`)

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::errors::PackageResult;
use serde::{Deserialize, Serialize};
use serde_spanned::Spanned;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct LocalDependency {
    /// The path on the filesystem, relative to the location of the containing file (which is
    /// stored in the `Located` wrapper)
    local: PathBuf,
}

impl LocalDependency {
    /// Returns the path to the local dependency
    pub fn path(&self) -> PackageResult<PathBuf> {
        let path = fs::canonicalize(&self.local)?;
        Ok(path)
    }
}

// TODO: dead code
impl TryFrom<(&Path, toml_edit::Value)> for LocalDependency {
    type Error = anyhow::Error; // TODO

    fn try_from(value: (&Path, toml_edit::Value)) -> Result<Self, Self::Error> {
        // TODO: just deserialize
        todo!()
    }
}
