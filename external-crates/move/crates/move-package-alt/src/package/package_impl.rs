// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::Debug,
    marker::PhantomData,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};

use super::lockfile::{Lockfile, Publication};
use super::manifest::Manifest;
use crate::{
    dependency::{DependencySet, PinnedDependencyInfo},
    errors::{ManifestError, PackageResult},
    flavor::MoveFlavor,
};
use move_core_types::identifier::Identifier;
use tracing::debug;

// TODO: we might want to use [move_core_types::Identifier] here, particularly for `PackageName`.
// This will force us to maintain invariants.
pub type EnvironmentName = String;
pub type PackageName = Identifier;

pub struct Package<F: MoveFlavor> {
    // TODO: maybe hold a lock on the lock file? Maybe not if move-analyzer wants to hold on to a
    // Package long term?
    manifest: Manifest<F>,
    lockfiles: Lockfile<F>,
    path: PackagePath,
}

/// An absolute path to a directory containing a loaded Move package (in particular, the directory
/// must have a Move.toml)
pub struct PackagePath(PathBuf);

impl<F: MoveFlavor> Package<F> {
    /// Load a package from the manifest and lock files in directory [path].
    /// Makes a best effort to translate old-style packages into the current format,
    ///
    /// Fails if [path] does not exist, or if it doesn't contain a manifest
    pub async fn load_root(path: impl AsRef<Path>) -> PackageResult<Self> {
        todo!()
        /*
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
        */
    }

    /// Fetch [dep] and load a package from the fetched source
    /// Makes a best effort to translate old-style packages into the current format,
    pub async fn load(dep: PinnedDependencyInfo<F>) -> PackageResult<Self> {
        todo!()
    }

    /// The path to the root directory of this package. This path is guaranteed to exist
    /// and contain a manifest file.
    pub fn path(&self) -> &PackagePath {
        &self.path
    }

    /// TODO: comment
    pub fn manifest(&self) -> &Manifest<F> {
        &self.manifest
    }

    /// The resolved and pinned dependencies from the manifest
    pub fn pinned_direct_dependencies(&self) -> &DependencySet<PinnedDependencyInfo<F>> {
        todo!()
    }
}
