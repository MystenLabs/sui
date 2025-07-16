// Copyrightc) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::Path};

use super::manifest::Manifest;
use super::paths::PackagePath;
use crate::{
    compatibility::{
        legacy::LegacyData,
        legacy_parser::{is_legacy_like, parse_legacy_manifest_from_file},
    },
    dependency::{CombinedDependency, PinnedDependencyInfo, pin},
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::{lockfile::Lockfiles, manifest::Digest},
    schema::{
        Environment, LocalDepInfo, LockfileDependencyInfo, OriginalID, PackageMetadata,
        PackageName, Publication, PublishAddresses, PublishedID,
    },
};

pub type EnvironmentName = String;
pub type EnvironmentID = String;

// pub type PackageName = Identifier;
pub type AddressInfo = String;

#[derive(Debug)]
pub struct Package<F: MoveFlavor> {
    /// The environment of the loaded package.
    env: EnvironmentName,
    /// The digest of the package.
    digest: Digest,
    /// The metadata of the package.
    metadata: PackageMetadata,
    /// A [`PackagePath`] representing the canonical path to the package directory.
    path: PackagePath,
    /// (Optional) Publish information for the loaded environment (original-id, published-at and more).
    publish_data: Option<Publication<F>>,

    /// The way this package should be serialized to the lockfile. Note that this is a dependency
    /// relative to the root package (in particular, the root package is the only package with
    /// `source = {local = "."}`
    source: LockfileDependencyInfo,

    /// Optional legacy information for a supplied package.
    /// TODO(manos): Make `LegacyData` single environment too, or use multiple types for this.
    pub legacy_data: Option<LegacyData>,

    /// The pinned direct dependencies for this package
    /// Note: for legacy packages, this information will be stored in `legacy_data`.
    deps: BTreeMap<PackageName, PinnedDependencyInfo>,
}

impl<F: MoveFlavor> Package<F> {
    /// Load a package from the manifest.
    /// Makes a best effort to translate old-style packages into the current format,
    ///
    /// Fails if [path] does not exist, or if it doesn't contain a manifest
    pub async fn load_root(path: impl AsRef<Path>, env: &Environment) -> PackageResult<Self> {
        let path = PackagePath::new(path.as_ref().to_path_buf())?;
        let source = LockfileDependencyInfo::Local(LocalDepInfo { local: ".".into() });

        Self::load_internal(path, source, env).await
    }

    /// Fetch [dep] and load a package from the fetched source
    /// Makes a best effort to translate old-style packages into the current format,
    pub async fn load(dep: PinnedDependencyInfo, env: &Environment) -> PackageResult<Self> {
        let path = PackagePath::new(dep.fetch().await?)?;

        Self::load_internal(path, dep.into(), env).await
    }

    /// Loads a package internally, doing a "best" effort to translate an old-style package into the new one.
    async fn load_internal(
        path: PackagePath,
        source: LockfileDependencyInfo,
        env: &Environment,
    ) -> PackageResult<Self> {
        let manifest = Manifest::<F>::read_from_file(path.manifest_path());

        // If our "modern" manifest is OK, we load the modern lockfile and return early.
        if let Ok(manifest) = manifest {
            let publish_data = Self::load_published_info_from_lockfile(&path)?;

            // TODO: We should error if there environment is not supported!
            let manifest_deps = manifest
                .dependencies()
                .deps_for(env.name())
                .cloned()
                .unwrap_or_default();

            let deps = pin::<F>(manifest_deps.clone(), env.id()).await?;

            return Ok(Self {
                env: env.name().clone(),
                digest: manifest.digest().to_string(),
                metadata: manifest.metadata(),
                path,
                publish_data: publish_data.get(env.name()).cloned(),
                source,
                legacy_data: None,
                deps,
            });
        }

        // If the manifest does not look like a legacy one, we again return early by erroring on the modern errors.
        if !is_legacy_like(&path) {
            return Err(PackageError::Manifest(manifest.unwrap_err()));
        }

        // Here, that means that we're working on legacy package, so we can throw its errors.
        let legacy_manifest = parse_legacy_manifest_from_file(&path)?;

        let handle = legacy_manifest.file_handle;

        let deps = pin::<F>(
            legacy_manifest
                .deps
                .into_iter()
                .map(|(name, dep)| {
                    (
                        name,
                        CombinedDependency::from_default(handle, env.name().clone(), dep),
                    )
                })
                .collect(),
            env.id(),
        )
        .await?;

        Ok(Self {
            env: env.name().clone(),
            // TODO: Should we compute this at this point?
            digest: "".to_string(),
            metadata: legacy_manifest.metadata,
            path,
            publish_data: Default::default(),
            source,
            legacy_data: Some(legacy_manifest.legacy_data),
            deps,
        })
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
                    .map(|(env, pub_info)| (env.clone(), pub_info))
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
        self.metadata.name.as_ref()
    }

    pub fn digest(&self) -> &Digest {
        &self.digest
    }

    /// The way this package should be serialized to the root package's lockfile. Note that this is
    /// a dependency relative to the root package (in particular, the root package is the only
    /// package where `dep_for_self()` returns `{local = "."}`
    pub fn dep_for_self(&self) -> &LockfileDependencyInfo {
        &self.source
    }

    pub fn is_legacy(&self) -> bool {
        self.legacy_data.is_some()
    }

    /// This returns true if the `source` for the package is `{ local = "." }`. This is guaranteed
    /// to hold for exactly one package for a valid package graph (see [Self::dep_for_self] for
    /// more information)
    pub fn is_root(&self) -> bool {
        let result = (self.dep_for_self()
            == &LockfileDependencyInfo::Local(LocalDepInfo { local: ".".into() }));
        result
    }

    /// The resolved and pinned dependencies from the manifest for environment `env`
    /// Returns an error if `env` is not declared in the manifest (TODO: remove this restriction?)
    pub fn direct_deps(&self) -> &BTreeMap<PackageName, PinnedDependencyInfo> {
        &self.deps
    }

    fn publication(&self) -> Option<&PublishAddresses> {
        self.publish_data.as_ref().map(|data| &data.addresses)
    }

    /// Tries to get the `published-at` entry for the given package,
    /// including support for backwards compatibility (legacy packages)
    pub fn published_at(&self) -> Option<PublishedID> {
        self.publication().map(|data| data.published_at.clone())
    }

    /// Tries to get the `original-id` entry for the given package,
    /// including support for backwards compatibility (legacy packages)
    pub fn original_id(&self) -> Option<OriginalID> {
        self.publication().map(|data| data.original_id.clone())
    }
}
