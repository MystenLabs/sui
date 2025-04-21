// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod lockfile;
pub mod manifest;

use std::{
    fmt::Debug,
    marker::PhantomData,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::flavor::MoveFlavor;
use lockfile::{Lockfile, Publication};

pub type EnvironmentName = String;
pub type PackageName = String;

pub struct Package<F: MoveFlavor> {
    // TODO: maybe hold a lock on the lock file? Maybe not if move-analyzer wants to hold on to a
    // Package long term?
    // TODO: manifest: manifest::Manifest,
    lockfiles: Lockfile<F>,
    path: PathBuf,
}

impl<F: MoveFlavor> Package<F> {
    /// Load a package from the manifest and lock files in directory [path].
    /// Makes a best effort to translate old-style packages into the current format,
    ///
    /// Fails if [path] does not exist, or if it doesn't contain a manifest
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        todo!()
    }

    /// The path to the root directory of this package. This path is guaranteed to exist
    /// and contain a manifest file.
    pub fn path(&self) -> &Path {
        todo!()
    }

    /// Return the metadata for the most recent published version in the given environemtn
    pub fn publication_for(&self, env: EnvironmentName) -> Option<Publication<F>> {
        todo!()
    }

    /// Register a published package on the given chain in the saved lockfiles
    pub fn add_publication_for(
        &mut self,
        env: EnvironmentName,
        metadata: Publication<F>,
    ) -> anyhow::Result<()> {
        // TODO: we'll have to update the local dependencies here
        todo!()
    }

    /// Load and return all the transitive dependencies of this package
    pub fn transitive_dependencies(&self) -> Vec<Package<F>> {
        todo!()
    }
}
