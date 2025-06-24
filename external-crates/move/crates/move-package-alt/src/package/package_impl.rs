// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fmt::{self, Debug},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::manifest::Manifest;
use super::{
    lockfile::{Lockfile, Publication},
    paths::PackagePath,
};
use crate::{
    compatibility::{
        legacy::LegacyPackageInformation, legacy_parser::parse_legacy_manifest_from_file,
    },
    dependency::{DependencySet, PinnedDependencyInfo, pin},
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
};
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use tracing::{debug, info};

pub type EnvironmentName = String;
pub type EnvironmentID = String;

pub type PackageName = Identifier;
pub type AddressInfo = String;

// TODO: We should probably move this to `lockfile.rs` as this is lockfile data.
// Keeping it here to scaffold the data we want to gather during publishing / upgrades.
#[derive(Debug, Serialize, Deserialize)]
pub struct PublishInformation {
    /// This is usually the `chain_id`. We need to decide if we really want to abstract the concept of "environments".
    pub environment: EnvironmentID,
    /// The IDs (original, published_at) for the package.
    pub published_ids: PublishedIds,
    /// The current version of the package -- this info is not needed in the package graphs, maybe
    /// helps with conflict errors handling.
    pub version: String,
}

/// TODO(manos): Move this to a better place!
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PublishedIds {
    /// The "original" address (v1 of the published package)
    pub original_id: AccountAddress,
    /// The `latest` address (latest address of the published package)
    pub latest_id: AccountAddress,
}

/// TODO(manos): Move this to the lockfile data once we align on structure.
pub type PublishInformationMap = BTreeMap<EnvironmentName, PublishInformation>;

#[derive(Debug)]
pub struct Package<F: MoveFlavor> {
    // TODO: maybe hold a lock on the lock file? Maybe not if move-analyzer wants to hold on to a
    // Package long term?
    manifest: Manifest<F>,
    path: PackagePath,
    /// The on-chain publish information per environment
    publish_data: Option<PublishInformationMap>,
    /// The legacy data we might need (e.g. for deprecated "addresses" section).
    legacy_info: Option<LegacyPackageInformation>,
}

impl<F: MoveFlavor> Package<F> {
    /// Load a package from the manifest.
    /// Makes a best effort to translate old-style packages into the current format,
    ///
    /// Fails if [path] does not exist, or if it doesn't contain a manifest
    pub async fn load_root(path: impl AsRef<Path>) -> PackageResult<Self> {
        let path = PackagePath::new(path.as_ref().to_path_buf())?;

        Self::internal_load(path).await
    }

    /// Fetch [dep] and load a package from the fetched source
    /// Makes a best effort to translate old-style packages into the current format,
    pub async fn load(dep: PinnedDependencyInfo) -> PackageResult<Self> {
        let path = PackagePath::new(dep.fetch().await?)?;

        Self::internal_load(path).await
    }

    /// TODO: The ugly thing here is that we pretty much emit "legacy" errors here, which is not what we want.
    /// We should probably indicate if the manifest is intended to be modern but has invalid syntax (by peeking into the "edition"),
    /// when we're parsing the modern manifests, or instead fallback to trying to read as legacy!
    async fn internal_load(path: PackagePath) -> PackageResult<Self> {
        if let Ok(manifest) = Manifest::<F>::read_from_file(path.manifest_path()) {
            Ok(Self {
                manifest,
                path,
                legacy_info: None,
                publish_data: None,
            })
        } else {
            let (manifest, legacy_info) =
                Manifest::<F>::read_legacy_from_file(path.manifest_path())?;

            Ok(Self {
                manifest,
                path,
                legacy_info: Some(legacy_info),
                publish_data: None,
            })
        }
    }

    /// TODO(manos): Finish this with the non-legacy approach too!
    pub fn get_package_ids(&self, _env: &EnvironmentName) -> PackageResult<PublishedIds> {
        // TODO: This should be "last" resort. IF we find the IDs in the regular envs, we do not need this.
        // [legacy]: Handle the case where the IDs are part of `published-at` and `[addresses]` on the legacy manifest files.
        if let Some(legacy_info) = &self.legacy_info {
            if let Some(manifest_address_info) = &legacy_info.manifest_address_info {
                return Ok(manifest_address_info.clone());
            }
        }
        // TODO: implement normal flow!
        Err(PackageError::Generic("No package ids found".to_string()))
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

    /// The resolved and pinned dependencies from the manifest for environment `env`
    pub async fn direct_deps(
        &self,
        env: &EnvironmentName,
    ) -> PackageResult<BTreeMap<PackageName, PinnedDependencyInfo>> {
        let mut deps = self.manifest.dependencies();

        if self.manifest().environments().get(env).is_none() {
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
}
