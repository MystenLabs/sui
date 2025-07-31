// Copyrightc) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::Path};

use tracing::debug;

use super::compute_digest;
use super::manifest::{Manifest, ManifestError, ManifestErrorKind};
use super::paths::PackagePath;
use crate::dependency::FetchedDependency;
use crate::errors::{FileHandle, Location};
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
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use std::sync::{LazyLock, Mutex};

// TODO: is this the right way to handle this?
static DUMMY_ADDRESSES: LazyLock<Mutex<u16>> = LazyLock::new(|| Mutex::new(0x1000));

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

    /// Dummy address that is set during package graph initialization for unpublished addresses
    // TODO: probably we want to refactor this and have it in published
    pub dummy_addr: OriginalID,
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
        let dummy_addr = {
            let lock = DUMMY_ADDRESSES.lock();
            let mut dummy_addr = lock.unwrap();
            *dummy_addr += 1;
            *dummy_addr
        };

        // If our "modern" manifest is OK, we load the modern lockfile and return early.
        if let Ok(manifest) = manifest {
            // TODO check if the environment IDs match
            // - if there's multiple keys for the same environment ID, we error
            // - if there is one key for the environment ID, we use that
            // - if there is no value with the same environment ID, we error

            let default_envs = F::default_environments();
            Self::validate_manifest(&manifest, *manifest.file_handle(), &default_envs);

            let publish_data = Self::load_published_info_from_lockfile(&path)?;

            debug!("adding implicit dependencies");
            let implicit_deps =
                Self::implicit_deps(env, manifest.parsed().package.implicit_deps.clone())?;

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
                dummy_addr: OriginalID(AccountAddress::from_suffix(dummy_addr)),
            });
        }

        // If the manifest does not look like a legacy one, we again return early by erroring on the modern errors.
        if !is_legacy_like(&path) {
            return Err(PackageError::Manifest(manifest.unwrap_err()));
        }

        // Here, that means that we're working on legacy package, so we can throw its errors.
        let legacy_manifest = parse_legacy_manifest_from_file(&path)?;

        let implicit_deps =
            Self::implicit_deps(env, legacy_manifest.metadata.implicit_deps.clone())?;

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
            digest: compute_digest(legacy_manifest.file_handle.source()),
            metadata: legacy_manifest.metadata,
            path,
            publish_data: None,
            dep_for_self: source,
            legacy_data: Some(legacy_manifest.legacy_data),
            deps,
            dummy_addr: OriginalID(AccountAddress::from_suffix(dummy_addr)),
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

    pub fn metadata(&self) -> &PackageMetadata {
        &self.metadata
    }

    /// Return the implicit deps depending on the implicit dep mode.
    fn implicit_deps(
        env: &Environment,
        implicit_dep_mode: ImplicitDepMode,
    ) -> PackageResult<BTreeMap<PackageName, ReplacementDependency>> {
        match implicit_dep_mode {
            // For enabled state, we need to pick the deps based on whether there is
            // a specfiied
            ImplicitDepMode::Enabled(specified_deps) => {
                let deps = F::implicit_deps(env.id().to_string());

                if let Some(specified_deps) = specified_deps {
                    // If a list of deps is specified, we need to make sure
                    // that all of the deps are valid in the implicit deps list, or warn.
                    for dep in &specified_deps {
                        if !deps.contains_key(&Identifier::new(dep.as_str())?) {
                            return Err(PackageError::Generic(format!(
                                "The implicit dependency `{}` does not exist in the implicit deps list.",
                                dep
                            )));
                        }
                    }

                    // If we have a "specified" list of deps, we need to filter the implicit deps to only support
                    // the ones that are in the specified list.
                    Ok(deps
                        .into_iter()
                        .filter(|(name, _)| specified_deps.contains(&name.to_string()))
                        .collect())
                } else {
                    Ok(deps)
                }
            }
            ImplicitDepMode::Disabled => Ok(BTreeMap::new()),
            ImplicitDepMode::Testing => todo!(),
        }
    }

    /// Validate the manifest contents, after deserialization.
    ///
    // TODO: add more validation
    fn validate_manifest(
        manifest: &Manifest,
        handle: FileHandle,
        default_envs: &BTreeMap<String, String>,
    ) -> PackageResult<()> {
        let mut environments = manifest.environments();
        environments.extend(default_envs.iter().map(|(k, v)| (k.clone(), v.clone())));
        assert!(
            !environments.is_empty(),
            "there should be at least one environment"
        );

        // Do all dep-replacements have valid environments?
        for (env, entries) in manifest.parsed().dep_replacements.iter() {
            if !environments.contains_key(env) {
                let span = entries
                    .first_key_value()
                    .expect("dep-replacements.<env> only exists if it has a dep")
                    .1
                    .span();

                let loc = Location::new(handle, span);

                return Err(ManifestError::with_span(&loc)(
                    ManifestErrorKind::MissingEnvironment { env: env.clone() },
                )
                .into());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestFlavor;

    impl MoveFlavor for TestFlavor {
        type PublishedMetadata = ();
        type PackageMetadata = ();
        type AddressInfo = String;

        fn name() -> String {
            "test".to_string()
        }

        fn default_environments() -> BTreeMap<String, String> {
            BTreeMap::new()
        }

        // Our test flavor has "foo" and "bar" accessible.
        fn implicit_deps(_: String) -> BTreeMap<PackageName, ReplacementDependency> {
            let mut deps = BTreeMap::new();
            deps.insert(
                new_package_name("foo"),
                ReplacementDependency {
                    dependency: None,
                    addresses: None,
                    use_environment: None,
                },
            );
            deps.insert(
                new_package_name("bar"),
                ReplacementDependency {
                    dependency: None,
                    addresses: None,
                    use_environment: None,
                },
            );
            deps
        }
    }

    #[test]
    /// We enable ALL implicit-deps.
    fn test_all_implicit_deps() {
        let env = test_environment();
        let implicit_deps = ImplicitDepMode::Enabled(None);

        let deps = Package::<TestFlavor>::implicit_deps(&env, implicit_deps).unwrap();
        let dep_keys: Vec<_> = deps.keys().cloned().collect();

        assert_eq!(dep_keys.len(), 2);
        assert!(dep_keys.contains(&new_package_name("foo")));
        assert!(dep_keys.contains(&new_package_name("bar")));
    }

    #[test]
    /// We enable implicit-deps, but specifying which ones we want.
    fn test_explicit_implicit_deps() {
        let env = test_environment();
        let implicit_deps = ImplicitDepMode::Enabled(Some(vec!["foo".to_string()]));

        let deps = Package::<TestFlavor>::implicit_deps(&env, implicit_deps).unwrap();
        let dep_keys: Vec<_> = deps.keys().cloned().collect();

        assert_eq!(dep_keys.len(), 1);
        assert!(dep_keys.contains(&new_package_name("foo")));
        assert!(!dep_keys.contains(&new_package_name("bar")));
    }

    #[test]
    fn test_explicit_implicit_deps_with_invalid_names() {
        let env = test_environment();
        let implicit_deps =
            ImplicitDepMode::Enabled(Some(vec!["ignore".to_string(), "foo".to_string()]));

        assert!(Package::<TestFlavor>::implicit_deps(&env, implicit_deps).is_err());
    }

    #[test]
    /// We disable implicit deps.
    fn test_no_implicit_deps() {
        let env = test_environment();
        let implicit_deps = ImplicitDepMode::Disabled;

        let deps = Package::<TestFlavor>::implicit_deps(&env, implicit_deps).unwrap();

        assert_eq!(deps.len(), 0);
    }

    #[test]
    /// We disable implicit deps by providing empty array
    ///
    fn test_empty_implicit_deps() {
        let env = test_environment();
        let implicit_deps = ImplicitDepMode::Enabled(Some(vec![]));

        let deps = Package::<TestFlavor>::implicit_deps(&env, implicit_deps).unwrap();

        assert_eq!(deps.len(), 0);
    }

    fn test_environment() -> Environment {
        Environment::new("test".to_string(), "test".to_string())
    }

    fn new_package_name(name: &str) -> PackageName {
        PackageName::new(name.to_string()).unwrap()
    }
}
