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
#[serde(untagged)]
pub enum LocalDependency {
    /// A local dependency that is not pinned
    Unpinned(UnpinnedLocalDependency),
    /// A local dependency that is pinned, containing additional metadata about the dependency's
    /// parent.
    Pinned(PinnedLocalDependency),
}

// TODO: PinnedLocalDependencies should be different from UnpinnedLocalDependency - the former also
// needs an absolute filesystem path (which doesn't get serialized to the lockfile)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnpinnedLocalDependency {
    /// The path on the filesystem, relative to the location of the containing file (which is
    /// stored in the `Located` wrapper)
    local: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PinnedLocalDependency {
    /// The path on the filesystem, relative to the location of the containing file (which is
    /// stored in the `Located` wrapper)
    local: PathBuf,
}

impl LocalDependency {
    /// The path on the filesystem, relative to the location of the containing file
    pub fn path(&self) -> PackageResult<PathBuf> {
        match self {
            LocalDependency::Unpinned(dep) => dep.path(),
            LocalDependency::Pinned(dep) => dep.path(),
        }
    }
}

impl UnpinnedLocalDependency {
    /// The path on the filesystem, relative to the location of the containing file
    pub fn path(&self) -> PackageResult<PathBuf> {
        let path = fs::canonicalize(&self.local)?;
        Ok(path)
    }

    // TODO
    // /// Given a local dependency inside a manifest living at [source], return a pinned dependency
    // pub fn pin(&self, source: &PinnedDependencyInfo<F>) -> PackageResult<PinnedDependencyInfo<F>> {
    //     todo!()
    // }
}

impl PinnedLocalDependency {
    pub fn path(&self) -> PackageResult<PathBuf> {
        let path = fs::canonicalize(&self.local)?;

        Ok(path)
    }
}

// TODO: dead code
impl TryFrom<(&Path, toml_edit::Value)> for UnpinnedLocalDependency {
    type Error = anyhow::Error; // TODO

    fn try_from(value: (&Path, toml_edit::Value)) -> Result<Self, Self::Error> {
        // TODO: just deserialize
        todo!()
    }
}
