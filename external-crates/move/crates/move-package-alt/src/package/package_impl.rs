// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fmt::{self, Debug},
    marker::PhantomData,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};

use super::lockfile::{Lockfile, Publication};
use super::manifest::Manifest;
use crate::{
    dependency::{DependencySet, PinnedDependencyInfo, fetch},
    errors::{ManifestError, PackageResult},
    flavor::MoveFlavor,
    git::GitRepo,
};
use move_core_types::identifier::Identifier;
use tracing::debug;

pub type EnvironmentName = String;
pub type PackageName = Identifier;

#[derive(Debug)]
pub struct Package<F: MoveFlavor + fmt::Debug> {
    // TODO: maybe hold a lock on the lock file? Maybe not if move-analyzer wants to hold on to a
    // Package long term?
    manifest: Manifest<F>,
    path: PackagePath,
}

/// An absolute path to a directory containing a loaded Move package (in particular, the directory
/// must have a Move.toml)
#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub struct PackagePath(PathBuf);

impl PackagePath {
    /// Create a new package path from a string
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self(path.as_ref().to_path_buf())
    }

    /// Get the underlying path
    pub fn as_path(&self) -> &Path {
        self.0.as_path()
    }
}

impl<F: MoveFlavor> Package<F> {
    /// Load a package from the manifest and lock files in directory [path].
    /// Makes a best effort to translate old-style packages into the current format,
    ///
    /// Fails if [path] does not exist, or if it doesn't contain a manifest
    pub async fn load_root(path: impl AsRef<Path>) -> PackageResult<Self> {
        let manifest = Manifest::<F>::read_from_file(path.as_ref())?;
        let path = PackagePath(path.as_ref().to_path_buf());
        Ok(Self { manifest, path })
    }

    /// Fetch [dep] and load a package from the fetched source
    /// Makes a best effort to translate old-style packages into the current format,
    pub async fn load(dep: PinnedDependencyInfo<F>) -> PackageResult<Self> {
        // TODO: most of this should live in [dependency]
        use PinnedDependencyInfo as P;

        let package = match dep {
            P::Git(d) => {
                let git = GitRepo::from(&d);
                let path = git.fetch().await?;
                let manifest = Manifest::<F>::read_from_file(&path)?;
                Self {
                    manifest,
                    path: PackagePath(path),
                }
            }
            P::Local(d) => {
                let local = d.path()?;
                let manifest = Manifest::<F>::read_from_file(&local.join("Move.toml"))?;
                Self {
                    manifest,
                    path: PackagePath(local),
                }
            }
            P::FlavorSpecific(dep) => todo!(),
        };

        Ok(package)
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

    /// The resolved and pinned dependencies from the manifest for environment `env`
    pub fn direct_deps(
        &self,
        env: &EnvironmentName,
    ) -> BTreeMap<PackageName, PinnedDependencyInfo<F>> {
        todo!()
    }
}
