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

use super::manifest::Manifest;
use crate::{
    dependency::{DependencySet, PinnedDependencyInfo},
    errors::{ManifestError, PackageResult},
    flavor::MoveFlavor,
    git::GitRepo,
};
use crate::{errors::PackageResult, schema::ParsedManifest};
use move_core_types::identifier::Identifier;
use tracing::debug;

#[derive(Debug)]
pub struct Package {
    // TODO: maybe hold a lock on the lock file? Maybe not if move-analyzer wants to hold on to a
    // Package long term?
    manifest: Manifest,
    path: PackagePath,
}

impl Package {
    /// Load a package from the manifest and lock files in directory [path].
    /// Makes a best effort to translate old-style packages into the current format,
    ///
    /// Fails if [path] does not exist, or if it doesn't contain a manifest
    pub async fn load_root(path: impl AsRef<Path>) -> PackageResult<Self> {
        let manifest = Manifest::load(path.as_ref())?;
        let path = PackagePath(path.as_ref().to_path_buf());
        Ok(Self { manifest, path })
    }

    /// Fetch [dep] and load a package from the fetched source
    /// Makes a best effort to translate old-style packages into the current format,
    pub async fn load(dep: PinnedDependencyInfo) -> PackageResult<Self> {
        let path = PackagePath::new(dep.fetch().await?)?;
        let manifest = Manifest::read_from_file(path.manifest_path())?;

        Ok(Self { manifest, path })
    }

    /// The path to the root directory of this package. This path is guaranteed to exist
    /// and contain a manifest file.
    pub fn path(&self) -> &PackagePath {
        &self.path
    }

    /// TODO: comment
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The resolved and pinned dependencies from the manifest for environment `env`
    pub fn direct_deps(
        &self,
        env: &EnvironmentName,
    ) -> BTreeMap<PackageName, PinnedDependencyInfo> {
        todo!()
    }
}
