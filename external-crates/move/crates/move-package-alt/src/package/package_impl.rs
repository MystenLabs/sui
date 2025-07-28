// Copyrightc) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::Path};

use tracing::debug;

use super::manifest::Manifest;
use super::paths::PackagePath;
use crate::compatibility::legacy_parser::ParsedLegacyPackage;
use crate::dependency::FetchedDependency;
use crate::errors::FileHandle;
use crate::schema::{ImplicitDepMode, ReplacementDependency};
use crate::{
    compatibility::{
        legacy::LegacyData,
        legacy_parser::{is_legacy_like, parse_legacy_manifest_from_file},
    },
    dependency::{CombinedDependency, PinnedDependencyInfo},
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::{lockfile::Lockfiles, manifest::Digest},
    schema::{
        Environment, OriginalID, PackageMetadata, PackageName, Publication, PublishAddresses,
        PublishedID,
    },
};

const SYSTEM_DEPS_NAMES: [&str; 5] = ["Sui", "MoveStdlib", "Bridge", "DeepBook", "SuiSystem"];

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
    dep_for_self: PinnedDependencyInfo,

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
        let root_manifest = FileHandle::new(path.manifest_path())?;
        let source = PinnedDependencyInfo::root_dependency(root_manifest, env.name().clone());

        Self::load_internal(path, source, env).await
    }

    /// Fetch [dep] and load a package from the fetched source
    /// Makes a best effort to translate old-style packages into the current format,
    pub async fn load(dep: PinnedDependencyInfo, env: &Environment) -> PackageResult<Self> {
        let path = FetchedDependency::fetch(&dep).await?.into();

        Self::load_internal(path, dep, env).await
    }

    /// Loads a package internally, doing a "best" effort to translate an old-style package into the new one.
    async fn load_internal(
        path: PackagePath,
        source: PinnedDependencyInfo,
        env: &Environment,
    ) -> PackageResult<Self> {
        let manifest = Manifest::read_from_file(path.manifest_path());

        // If our "modern" manifest is OK, we load the modern lockfile and return early.
        if let Ok(manifest) = manifest {
            // TODO check if the environment IDs match
            // - if there's multiple keys for the same environment ID, we error
            // - if there is one key for the environment ID, we use that
            // - if there is no value with the same environment ID, we error

            let publish_data = Self::load_published_info_from_lockfile(&path)?;

            debug!("adding implicit dependencies");
            let implicit_deps =
                Self::implicit_deps(env, manifest.parsed().package.implicit_deps, None);

            // TODO: We should error if there environment is not supported!
            debug!("combining [dependencies] with [dep-replacements] for {env:?}");
            let combined_deps = CombinedDependency::combine_deps(
                manifest.file_handle(),
                env,
                manifest
                    .dep_replacements()
                    .get(env.name())
                    .unwrap_or(&BTreeMap::new()),
                &manifest.dependencies(),
                &implicit_deps,
            )?;

            debug!("pinning dependencies");
            let deps = PinnedDependencyInfo::pin::<F>(&source, combined_deps, env.id()).await?;

            debug!("package loaded from {:?}", path.as_ref());
            return Ok(Self {
                env: env.name().clone(),
                digest: manifest.digest().to_string(),
                metadata: manifest.metadata(),
                path,
                publish_data: publish_data.get(env.name()).cloned(),
                dep_for_self: source,
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
        let implicit_deps =
            Self::implicit_deps(env, ImplicitDepMode::Legacy, Some(&legacy_manifest));

        let combined_deps = CombinedDependency::combine_deps(
            &legacy_manifest.file_handle,
            env,
            &BTreeMap::new(),
            &legacy_manifest.deps,
            &implicit_deps,
        )?;

        let deps = PinnedDependencyInfo::pin::<F>(&source, combined_deps, env.id()).await?;

        Ok(Self {
            env: env.name().clone(),
            // TODO: Should we compute this at this point?
            digest: "".to_string(),
            metadata: legacy_manifest.metadata,
            path,
            publish_data: Default::default(),
            dep_for_self: source,
            legacy_data: Some(legacy_manifest.legacy_data),
            deps,
        })
    }

    /// Try to load a lockfile and extract the published information for each environment from it
    fn load_published_info_from_lockfile(
        path: &PackagePath,
    ) -> PackageResult<BTreeMap<EnvironmentName, Publication<F>>> {
        let lockfile = Lockfiles::<F>::read_from_dir(path)?;

        debug!("lockfiles loaded");
        let publish_data = lockfile
            .map(|l| l.published().clone())
            .map(|x| {
                x.into_iter()
                    .map(|(env, pub_info)| (env.clone(), pub_info))
                    .collect()
            })
            .unwrap_or_default();

        debug!("extracted publication data");

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

    pub fn environment_name(&self) -> &EnvironmentName {
        &self.env
    }

    /// The way this package should be serialized to the root package's lockfile. Note that this is
    /// a dependency relative to the root package (in particular, the root package is the only
    /// package where `dep_for_self()` returns `{local = "."}`
    pub fn dep_for_self(&self) -> &PinnedDependencyInfo {
        &self.dep_for_self
    }

    pub fn is_legacy(&self) -> bool {
        self.legacy_data.is_some()
    }

    /// This returns true if the `source` for the package is `{ local = "." }`. This is guaranteed
    /// to hold for exactly one package for a valid package graph (see [Self::dep_for_self] for
    /// more information)
    pub fn is_root(&self) -> bool {
        self.dep_for_self().is_root()
    }

    /// The resolved and pinned dependencies from the manifest for environment `env`
    /// Returns an error if `env` is not declared in the manifest (TODO: remove this restriction?)
    pub fn direct_deps(&self) -> &BTreeMap<PackageName, PinnedDependencyInfo> {
        &self.deps
    }

    /// Tries to get the `published addresses` information for the given package,
    pub fn publication(&self) -> Option<&PublishAddresses> {
        self.legacy_data
            .as_ref()
            .and_then(|data| data.publication(self.environment_name()))
            .or_else(|| self.publish_data.as_ref().map(|data| &data.addresses))
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

    /// Return the implicit deps depending on the implicit dep mode. Note that if `implicit_dep_mode`
    /// is ImplicitDepMode::Legacy, a `legacy_manifest` is required otherwise it will panic.
    // TODO this needs to be moved into a ImplicitDeps trait
    fn implicit_deps(
        env: &Environment,
        implicit_dep_mode: ImplicitDepMode,
        legacy_manifest: Option<&ParsedLegacyPackage>,
    ) -> BTreeMap<PackageName, ReplacementDependency> {
        // TODO - rethink how this implict dep mode works
        // let system_pacakages = F::implicit_deps;
        // if let Some(legacy) = package.legacy_deta {
        //   check if package is a system package (i.e. if package.name() is in system_packages)
        //   check if package has any deps with same names as system packages
        //   if either is true: set implicit_dep_mode to Disabled
        // }

        match implicit_dep_mode {
            // for modern packages, the manifest should have the implicit-deps field set to true
            // (by default), or to false.
            ImplicitDepMode::Enabled => F::implicit_deps(env.id().to_string()),
            ImplicitDepMode::Disabled => BTreeMap::new(),
            ImplicitDepMode::Legacy => {
                let legacy_manifest = legacy_manifest.expect("Legacy manifest should be present");
                let system_deps_in_legacy_manifest = legacy_manifest
                    .deps
                    .iter()
                    .any(|(name, _)| SYSTEM_DEPS_NAMES.contains(&name.as_str()));

                // if the legacy manifest has system dependencies explicitly defined, return an empty
                // map
                if system_deps_in_legacy_manifest {
                    BTreeMap::new()
                } else {
                    let deps = F::implicit_deps(env.id().to_string());

                    // for legacy system packages, we don't need to add implicit deps for them as their
                    // dependencies are explicitly set in their manifest

                    if deps.contains_key(legacy_manifest.metadata.name.get_ref()) {
                        BTreeMap::new()
                    } else {
                        deps
                    }
                }
            }
            ImplicitDepMode::Testing => todo!(),
        }
    }
}
