// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod lockfile;
pub mod manifest;

use std::{
    fmt::Debug,
    marker::PhantomData,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};

use crate::{
    errors::{ManifestError, PackageResult},
    flavor::MoveFlavor,
};
use lockfile::{Lockfile, Publication};
use manifest::Manifest;
use move_core_types::identifier::Identifier;
use tracing::debug;

pub type EnvironmentName = String;
pub type PackageName = Identifier;

pub struct Package<F: MoveFlavor> {
    // TODO: maybe hold a lock on the lock file? Maybe not if move-analyzer wants to hold on to a
    // Package long term?
    manifest: Manifest<F>,
    lockfiles: Lockfile<F>,
    path: PathBuf,
}

impl<F: MoveFlavor> Package<F> {
    /// Load a package from the manifest and lock files in directory [path].
    /// Makes a best effort to translate old-style packages into the current format,
    ///
    /// Fails if [path] does not exist, or if it doesn't contain a manifest
    pub async fn load(path: impl AsRef<Path>, flavor: &F) -> PackageResult<Self> {
        let move_toml_path = path.as_ref().join("Move.toml");
        debug!(
            "Checking if there's a move toml file in path: {:?}",
            move_toml_path.display()
        );

        let manifest = Manifest::<F>::read_from(&move_toml_path)?;

        // check if there's a lockfile, and if it is not, we add one
        let mut lockfiles = Lockfile::read_from(&path)?;

        lockfiles.update_lockfile(flavor, &manifest).await?;

        Ok(Self {
            manifest,
            lockfiles,
            path: path.as_ref().to_path_buf(),
        })
    }

    /// The path to the root directory of this package. This path is guaranteed to exist
    /// and contain a manifest file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return the metadata for the most recent published version in the given environment.
    pub fn publication_for(&self, env: &EnvironmentName) -> Option<Publication<F>> {
        self.lockfiles.published_for_env(env)
    }

    /// Register a published package on the given chain in the saved lockfiles.
    pub fn add_publication_for(
        &mut self,
        env: EnvironmentName,
        metadata: Publication<F>,
    ) -> PackageResult<()> {
        // TODO: we'll have to update the local dependencies here
        todo!()
    }

    /// Load and return all the transitive dependencies of this package
    pub fn transitive_dependencies(&self) -> Vec<Package<F>> {
        todo!()
    }
}
