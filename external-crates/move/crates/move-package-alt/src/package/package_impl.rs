// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::Path};

use super::manifest::Manifest;
use super::paths::PackagePath;
use super::published_info::PublishInformation;
use crate::{
    dependency::{DependencySet, PinnedDependencyInfo, pin},
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::lockfile::Lockfiles,
    schema::{LocalDepInfo, LockfileDependencyInfo, Publication},
};
use move_core_types::identifier::Identifier;
use tracing::debug;

pub type EnvironmentName = String;
pub type EnvironmentID = String;

pub type PackageName = Identifier;
pub type AddressInfo = String;

#[derive(Debug)]
pub struct Package<F: MoveFlavor> {
    // TODO: maybe hold a lock on the lock file? Maybe not if move-analyzer wants to hold on to a
    // Package long term?
    manifest: Manifest<F>,
    /// A [`PackagePath`] representing the canonical path to the package directory.
    path: PackagePath,
    /// The on-chain publish information per environment
    publish_data: BTreeMap<EnvironmentName, PublishInformation<F>>,
    /// The way this package should be serialized to the lockfile
    source: LockfileDependencyInfo,

    /// The pinned direct dependencies for this package
    deps: DependencySet<PinnedDependencyInfo>,
}

impl<F: MoveFlavor> Package<F> {
    /// Load a package from the manifest.
    /// Makes a best effort to translate old-style packages into the current format,
    ///
    /// Fails if [path] does not exist, or if it doesn't contain a manifest
    pub async fn load_root(path: impl AsRef<Path>) -> PackageResult<Self> {
        let source = LockfileDependencyInfo::Local(LocalDepInfo { local: ".".into() });
        Self::_load(path, source).await
    }

    /// Fetch [dep] and load a package from the fetched source
    /// Makes a best effort to translate old-style packages into the current format,
    pub async fn load(dep: PinnedDependencyInfo) -> PackageResult<Self> {
        Self::_load(dep.fetch().await?, dep.into()).await
    }

    async fn _load(path: impl AsRef<Path>, source: LockfileDependencyInfo) -> PackageResult<Self> {
        let path = PackagePath::new(path.as_ref().to_path_buf())?;
        let manifest = Manifest::<F>::read_from_file(path.manifest_path())?;
        let publish_data = Self::load_published_info_from_lockfile(&path)?;

        debug!("pinning deps for {}", manifest.package_name());
        debug!("manifest: {:#?}", manifest);
        let deps = pin::<F>(manifest.dependencies().clone(), &manifest.environments()).await?;

        debug!("pinned deps: {:#?}", deps);

        Ok(Self {
            manifest,
            path,
            publish_data,
            source,
            deps,
        })
    }

    /// Try to load a lockfile and extract the published information for each environment from it
    fn load_published_info_from_lockfile(
        path: &PackagePath,
    ) -> PackageResult<BTreeMap<EnvironmentName, PublishInformation<F>>> {
        let lockfile = Lockfiles::<F>::read_from_dir(path)?;

        let publish_data = lockfile
            .map(|l| l.published().clone())
            .map(|x| {
                x.into_iter()
                    .map(|(env, pub_info)| {
                        (
                            pub_info.chain_id.clone(),
                            PublishInformation {
                                environment: env.clone(),
                                publication: pub_info,
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(publish_data)
    }

    /// The path to the root directory of this package. This path is guaranteed to exist
    /// and contain a manifest file.
    pub fn path(&self) -> &PackagePath {
        &self.path
    }

    pub fn name(&self) -> &PackageName {
        self.manifest().package_name()
    }

    /// The contents of the manifest file for the package
    pub fn manifest(&self) -> &Manifest<F> {
        &self.manifest
    }

    /// A dependency that points to this package.
    pub fn dep_for_self(&self) -> &LockfileDependencyInfo {
        &self.source
    }

    pub fn publication(&self, env: &EnvironmentName) -> Option<Publication<F>> {
        todo!()
    }

    /// The resolved and pinned dependencies from the manifest for environment `env`
    /// Returns an error if `env` is not declared in the manifest (TODO: remove this restriction?)
    pub fn direct_deps(
        &self,
        env: &EnvironmentName,
    ) -> PackageResult<BTreeMap<PackageName, PinnedDependencyInfo>> {
        debug!(
            "requested deps for {} in env {}",
            self.manifest.package_name(),
            env
        );

        if !self.manifest.environments().contains_key(env) {
            return Err(PackageError::Generic(format!(
                "Package {} does not have `{env}` defined as an environment in its manifest",
                self.name()
            )));
        }

        Ok(self.deps.deps_for(env).cloned().unwrap_or_default())
    }
}
