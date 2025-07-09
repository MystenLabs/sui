// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::Path};

use super::manifest::Manifest;
use super::paths::PackagePath;
use crate::{
    compatibility::{
        legacy::LegacyData,
        legacy_parser::{parse_legacy_lockfile_addresses, parse_legacy_manifest_from_file},
    },
    dependency::{PinnedDependencyInfo, pin},
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::lockfile::Lockfiles,
    schema::{LocalDepInfo, LockfileDependencyInfo, OriginalID, Publication, PublishedID},
};
use move_core_types::identifier::Identifier;

pub type EnvironmentName = String;
pub type EnvironmentID = String;

pub type PackageName = Identifier;
pub type AddressInfo = String;

pub type PublishData<F> = BTreeMap<EnvironmentName, Publication<F>>;

#[derive(Debug)]
pub struct Package<F: MoveFlavor> {
    // TODO: maybe hold a lock on the lock file? Maybe not if move-analyzer wants to hold on to a
    // Package long term?
    manifest: Manifest<F>,
    /// A [`PackagePath`] representing the canonical path to the package directory.
    path: PackagePath,
    /// The on-chain publish information per environment
    publish_data: PublishData<F>,
    /// The way this package should be serialized to the lockfile
    source: LockfileDependencyInfo,
    /// Optional legacy information for a supplied package.
    pub legacy_data: Option<LegacyData>,
}

impl<F: MoveFlavor> Package<F> {
    /// Load a package from the manifest.
    /// Makes a best effort to translate old-style packages into the current format,
    ///
    /// Fails if [path] does not exist, or if it doesn't contain a manifest
    pub async fn load_root(path: impl AsRef<Path>) -> PackageResult<Self> {
        let path = PackagePath::new(path.as_ref().to_path_buf())?;

        let (manifest, publish_data, legacy_data) = Self::load_internal(&path).await?;

        Ok(Self {
            manifest,
            path,
            publish_data,
            source: LockfileDependencyInfo::Local(LocalDepInfo { local: ".".into() }),
            legacy_data,
        })
    }

    /// Fetch [dep] and load a package from the fetched source
    /// Makes a best effort to translate old-style packages into the current format,
    pub async fn load(dep: PinnedDependencyInfo) -> PackageResult<Self> {
        let path = PackagePath::new(dep.fetch().await?)?;

        let (manifest, publish_data, legacy_data) = Self::load_internal(&path).await?;

        Ok(Self {
            manifest,
            path,
            publish_data,
            source: dep.into(),
            legacy_data,
        })
    }

    /// Loads a package internally, doing a "best" effort to translate an old-style package into the new one.
    pub async fn load_internal(
        path: &PackagePath,
    ) -> PackageResult<(Manifest<F>, PublishData<F>, Option<LegacyData>)> {
        let manifest = Manifest::<F>::read_from_file(path.manifest_path());

        // If our "modern" manifest is OK, we load the modern lockfile and that's it.
        if let Ok(manifest) = manifest {
            let publish_data = Self::load_published_info_from_lockfile(path)?;
            return Ok((manifest, publish_data, None));
        }

        // If our "modern" manifest is not OK, we try to parse a legacy manifest.
        let legacy_manifest = parse_legacy_manifest_from_file(path);

        if let Ok(legacy_manifest) = legacy_manifest {
            // We might be able to parse a manifest, but the edition is not as expected, which probably means
            // it is either an "incorrectly" designed modern package OR it's totally wrong.
            // In both cases, we wanna emit the "modern" error, rather than the legacy one.
            if legacy_manifest.is_legacy_edition {
                let publish_data = parse_legacy_lockfile_addresses(path).unwrap_or_default();

                return Ok((
                    Manifest::try_from_parsed_manifest(
                        legacy_manifest.parsed_manifest,
                        legacy_manifest.file_handle,
                    )?,
                    publish_data,
                    Some(legacy_manifest.legacy_data),
                ));
            }
        }

        // We default to the modern manifest's error.
        Err(PackageError::Manifest(manifest.unwrap_err()))
    }

    /// Try to load a lockfile and extract the published information for each environment from it
    fn load_published_info_from_lockfile(
        path: &PackagePath,
    ) -> PackageResult<BTreeMap<EnvironmentName, Publication<F>>> {
        let lockfile = Lockfiles::<F>::read_from_dir(path)?;

        let publish_data = lockfile
            .map(|l| l.published().clone())
            .map(|x| {
                x.into_iter()
                    .map(|(env, pub_info)| (pub_info.chain_id.clone(), pub_info))
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

    /// TODO: comment
    pub fn manifest(&self) -> &Manifest<F> {
        &self.manifest
    }

    pub fn dep_for_self(&self) -> &LockfileDependencyInfo {
        &self.source
    }

    /// The resolved and pinned dependencies from the manifest for environment `env`
    pub async fn direct_deps(
        &self,
        env: &EnvironmentName,
    ) -> PackageResult<BTreeMap<PackageName, PinnedDependencyInfo>> {
        let mut deps = self.manifest.dependencies();

        // TODO: This will probably go away after our discussions.
        if self.manifest().environments().get(env).is_none() && self.legacy_data.is_none() {
            return Err(PackageError::Generic(format!(
                "Package {} does not have `{env}` defined as an environment in its manifest",
                self.name()
            )));
        }

        let envs: BTreeMap<_, _> = self
            .manifest()
            .environments()
            .iter()
            .filter(|(e, _)| *e == env)
            .map(|(env, id)| (env.clone(), id.clone()))
            .collect();
        let pinned_deps = pin::<F>(deps.clone(), &envs).await?;

        Ok(pinned_deps
            .into_iter()
            .map(|(_, id, dep)| (id, dep))
            .collect())
    }

    /// Tries to get the `published-at` entry for the given package,
    /// including support for backwards compatibility (legacy packages)
    pub fn try_get_published_id(&self, env: &EnvironmentName) -> PackageResult<PublishedID> {
        if let Some(publish_data) = self.publish_data.get(env) {
            return Ok(publish_data.published_at.clone());
        }

        // Handle legacy packages (the ones with `publised-at` in Move.toml files)
        if let Some(legacy_data) = &self.legacy_data {
            if let Some(manifest_address_info) = &legacy_data.manifest_address_info {
                return Ok(manifest_address_info.published_at.clone());
            }
        }

        // TODO: Create specific errors when published id is not defined.
        Err(PackageError::Generic(format!(
            "Package {} does not have `{env}` published information",
            self.name()
        )))
    }

    /// Tries to get the `published-at` entry for the given package,
    /// including support for backwards compatibility (legacy packages)
    pub fn try_get_original_id(&self, env: &EnvironmentName) -> PackageResult<OriginalID> {
        if let Some(publish_data) = self.publish_data.get(env) {
            return Ok(publish_data.original_id.clone());
        }

        // Handle legacy packages (the ones with original id being on `[addresses]` in Move.toml files)
        if let Some(legacy_data) = &self.legacy_data {
            if let Some(manifest_address_info) = &legacy_data.manifest_address_info {
                return Ok(manifest_address_info.original_id.clone());
            }
        }

        Err(PackageError::Generic(format!(
            "Package {} does not have `{env}` published information",
            self.name()
        )))
    }
}
