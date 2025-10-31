// Copyrightc) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    sync::{LazyLock, Mutex},
};

use derive_where::derive_where;
use tracing::debug;

use super::manifest::Manifest;
use super::paths::PackagePath;
use super::{compute_digest, package_lock::PackageSystemLock};
use crate::compatibility::legacy::LegacyData;
use crate::dependency::FetchedDependency;
use crate::errors::FileHandle;
use crate::{
    dependency::Pinned,
    schema::{ParsedManifest, Publication},
};
use crate::{
    dependency::{CombinedDependency, PinnedDependencyInfo},
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::manifest::Digest,
    schema::{Environment, OriginalID, PackageMetadata, PackageName, PublishedID},
};

// TODO: is this the right way to handle this?
static DUMMY_ADDRESSES: LazyLock<Mutex<u16>> = LazyLock::new(|| Mutex::new(0x1000));

pub type EnvironmentName = String;
pub type EnvironmentID = String;

// pub type PackageName = Identifier;
pub type AddressInfo = String;

#[derive(Debug)]
#[derive_where(Clone)]
pub struct Package<F: MoveFlavor> {
    /// The environment of the loaded package.
    env: EnvironmentName,

    /// The digest of the package.
    digest: Digest,

    /// The metadata of the package.
    metadata: PackageMetadata,

    /// A [`PackagePath`] representing the canonical path to the package directory.
    path: PackagePath,

    /// The `Publication` information for the specified network
    publication: Option<Publication<F>>,

    /// The way this package should be serialized to the lockfile.
    dep_for_self: Pinned,

    /// Optional legacy information for a supplied package.
    pub legacy_data: Option<LegacyData>,

    /// The pinned direct dependencies for this package
    /// Note: for legacy packages, this information will be stored in `legacy_data`.
    deps: Vec<PinnedDependencyInfo>,

    /// Dummy address that is set during package graph initialization for unpublished addresses
    // TODO: probably we want to refactor this and have it in published
    pub dummy_addr: OriginalID,
}

impl<F: MoveFlavor> Package<F> {
    /// Load a package from the directory given by `path`.
    /// Makes a best effort to translate old-style packages into the current format
    pub async fn load_root(
        path: PackagePath,
        env: &Environment,
        mtx: &PackageSystemLock,
    ) -> PackageResult<Self> {
        let source = Pinned::Root(path);

        Self::load(source, env, mtx).await
    }

    /// Fetch [dep] (relative to [self]) and load a package from the fetched source
    /// Makes a best effort to translate old-style packages into the current format,
    pub async fn load(
        dep: Pinned,
        env: &Environment,
        mtx: &PackageSystemLock,
    ) -> PackageResult<Self> {
        debug!("loading package {:?}", dep);
        let path = FetchedDependency::fetch(&dep).await?;

        // try to load a legacy manifest (with an `[addresses]` section)
        //   - if it fails, load a modern manifest (and return any errors)
        let legacy_manifest = path.read_legacy_manifest(env, mtx)?;
        let (file_handle, manifest) = if let Some(result) = legacy_manifest {
            result
        } else {
            let manifest = Manifest::read_from_file(&path, mtx)?;
            check_for_environment::<F>(&manifest, &env.name)?;

            (*manifest.file_handle(), manifest.into_parsed())
        };

        // try to load the address from the modern lockfile
        //   - if it fails, look in the legacy data
        //   - if that fails, use a dummy address
        let publication = Self::load_publication(&path, env.name(), mtx)?.or_else(|| {
            manifest
                .legacy_data
                .as_ref()
                .and_then(|legacy| legacy.publication::<F>(env))
        });
        let dummy_addr = create_dummy_addr();

        // TODO: try to gather dependencies from the modern lockfile
        //   - if it fails (no lockfile / out of date lockfile), compute them from the manifest
        //     (adding system deps)

        let deps = Self::deps_from_manifest(&dep, &file_handle, &manifest, env).await?;

        // compute the digest (TODO: this should only compute over the environment specific data)
        let digest = compute_digest(file_handle.source());

        let result = Self {
            env: env.name().clone(),
            digest,
            metadata: manifest.package,
            path,
            publication,
            dep_for_self: dep,
            legacy_data: manifest.legacy_data,
            deps,
            dummy_addr,
        };

        debug!(
            "successfully loaded {:?}",
            result.dep_for_self.unfetched_path()
        );
        Ok(result)
    }

    /// Create a copy of this package with the publication information replaced by `publish`
    pub(crate) fn override_publish(&self, publish: Publication<F>) -> Self {
        let mut result = self.clone();
        debug!("updating address to {publish:?}");
        result.publication = Some(publish);
        result
    }

    /// The path to the root directory of this package. This path is guaranteed to exist
    /// and contain a manifest file.
    pub fn path(&self) -> &PackagePath {
        &self.path
    }

    pub fn name(&self) -> &PackageName {
        self.metadata.name.as_ref()
    }

    pub fn display_name(&self) -> &str {
        if let Some(legacy_data) = self.legacy_data.as_ref() {
            &legacy_data.legacy_name
        } else {
            self.metadata.name.as_ref().as_str()
        }
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
    pub fn dep_for_self(&self) -> &Pinned {
        &self.dep_for_self
    }

    pub fn is_legacy(&self) -> bool {
        self.legacy_data.is_some()
    }

    /// This returns true if the `source` for the package is `{ local = "." }`. This is guaranteed
    /// to hold for exactly one package for a valid package graph (see [Self::dep_for_self] for
    /// more information)
    pub fn is_root(&self) -> bool {
        matches!(self.dep_for_self(), Pinned::Root(_))
    }

    /// The resolved and pinned dependencies from the manifest for environment `env`
    /// Returns an error if `env` is not declared in the manifest (TODO: remove this restriction?)
    pub fn direct_deps(&self) -> &Vec<PinnedDependencyInfo> {
        &self.deps
    }

    /// Additional flavor-specific information that was recorded when this package was published
    /// (in the `Move.published` file or the ephemeral publication file if this was created with
    /// [Self::override_publish]).
    pub fn publication(&self) -> Option<&Publication<F>> {
        self.publication.as_ref()
    }

    /// Tries to get the `published-at` entry for the given package,
    /// including support for backwards compatibility (legacy packages)
    pub fn published_at(&self) -> Option<&PublishedID> {
        self.publication()
            .map(|publication| &publication.addresses.published_at)
    }

    /// Tries to get the `original-id` entry for the given package,
    /// including support for backwards compatibility (legacy packages)
    pub fn original_id(&self) -> Option<&OriginalID> {
        self.publication()
            .map(|publication| &publication.addresses.original_id)
    }

    pub fn metadata(&self) -> &PackageMetadata {
        &self.metadata
    }

    /// Read the publication for the given environment from the package pubfile.
    fn load_publication(
        path: &PackagePath,
        env: &EnvironmentName,
        mtx: &PackageSystemLock,
    ) -> PackageResult<Option<Publication<F>>> {
        let Some((file, parsed)) = path.read_pubfile(mtx)? else {
            return Ok(None);
        };

        let Some(publish) = parsed.published.get(env) else {
            debug!("no entry for {env:?} in {file:?}");
            return Ok(None);
        };

        Ok(Some(publish.clone()))
    }

    /// Compute the direct dependencies for the given environment by combining the default
    /// dependencies, system dependencies, and dep-replacements from the manifest and then pinning
    /// the results
    async fn deps_from_manifest(
        parent: &Pinned,
        file_handle: &FileHandle,
        manifest: &ParsedManifest,
        env: &Environment,
    ) -> PackageResult<Vec<PinnedDependencyInfo>> {
        let implicits = F::implicit_dependencies(env.id());
        let is_implicit = implicits.contains_key(manifest.package.name.as_ref());

        let system_dependencies = if manifest.package.implicit_dependencies && !is_implicit {
            debug!("adding implicit dependencies");
            F::implicit_dependencies(env.id())
        } else {
            debug!("no implicit dependencies");
            BTreeMap::new()
        };

        debug!("combining [dependencies] with [dep-replacements] for {env:?}");
        let combined_deps = CombinedDependency::combine_deps(
            file_handle,
            env,
            manifest
                .dep_replacements
                .get(env.name())
                .unwrap_or(&BTreeMap::new()),
            &manifest
                .dependencies
                .iter()
                .map(|(k, v)| (k.as_ref().clone(), v.clone()))
                .collect(),
            &system_dependencies,
        )?;

        debug!("pinning dependencies");
        PinnedDependencyInfo::pin::<F>(parent, combined_deps, env.id()).await
    }
}

/// Return a fresh OriginalID
fn create_dummy_addr() -> OriginalID {
    let lock = DUMMY_ADDRESSES.lock();
    let mut dummy_addr = lock.unwrap();
    *dummy_addr += 1;
    (*dummy_addr).into()
}

/// Check that `env` is defined in `manifest`, returning an error if it isn't
fn check_for_environment<F: MoveFlavor>(
    manifest: &Manifest,
    env: &EnvironmentName,
) -> PackageResult<()> {
    let mut known_environments = manifest.environments();
    known_environments.extend(F::default_environments());
    let known_environments: Vec<EnvironmentName> = known_environments
        .into_iter()
        .map(|(name, _)| name)
        .collect();

    if known_environments.contains(env) {
        Ok(())
    } else {
        let message = format!(
            "Package `{}` does not declare a `{}` environment. The available environments are {:?}. Consider running with `--build-env {}`",
            manifest.package_name(),
            env,
            known_environments,
            known_environments
                .first()
                .expect("there is at least one environment")
        );
        Err(PackageError::UnknownBuildEnv(message))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        flavor::vanilla::{DEFAULT_ENV_ID, DEFAULT_ENV_NAME, default_environment},
        package::RootPackage,
        schema::{LocalDepInfo, LockfileDependencyInfo, ReplacementDependency, SystemDepName},
        test_utils::graph_builder::TestPackageGraph,
    };

    use super::*;

    use indexmap::IndexMap;
    use insta::assert_snapshot;
    use test_log::test;

    #[derive(Debug)]
    struct TestFlavor;

    impl MoveFlavor for TestFlavor {
        type PublishedMetadata = ();
        type PackageMetadata = ();
        type AddressInfo = String;

        fn name() -> String {
            "test".to_string()
        }

        fn default_environments() -> IndexMap<EnvironmentName, EnvironmentID> {
            IndexMap::from([(DEFAULT_ENV_NAME.into(), DEFAULT_ENV_ID.into())])
        }

        // Our test flavor has `[foo, bar, baz]` system dependencies.
        fn system_deps(_env: &EnvironmentID) -> BTreeMap<SystemDepName, LockfileDependencyInfo> {
            let mut deps = BTreeMap::new();
            deps.insert(
                "FOO".into(),
                LockfileDependencyInfo::Local(LocalDepInfo {
                    local: "../foo".into(),
                }),
            );
            deps.insert(
                "BAR".into(),
                LockfileDependencyInfo::Local(LocalDepInfo {
                    local: "../bar".into(),
                }),
            );
            deps.insert(
                "BAZ".into(),
                LockfileDependencyInfo::Local(LocalDepInfo {
                    local: "../baz".into(),
                }),
            );
            deps
        }

        // In this flavor, only `[foo, bar]` are enabled by default.
        fn implicit_dependencies(
            _env: &EnvironmentID,
        ) -> BTreeMap<PackageName, ReplacementDependency> {
            let mut result = BTreeMap::new();

            result.insert(
                new_package_name("foo"),
                ReplacementDependency::override_system_dep("FOO"),
            );

            result.insert(
                new_package_name("bar"),
                ReplacementDependency::override_system_dep("BAR"),
            );

            result
        }
    }

    /// Loading a package includes the implicit dependencies, and the system dependencies are
    /// resolved to the right packages
    #[test(tokio::test)]
    async fn test_default_implicit_deps() {
        let scenario = TestPackageGraph::new(["root", "foo", "bar", "baz"]).build();

        let root = RootPackage::<TestFlavor>::load(
            scenario.path_for("root"),
            default_environment(),
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(
            root.package_info()
                .direct_deps()
                .keys()
                .map(|k| k.as_str())
                .collect::<Vec<_>>(),
            vec!["bar", "foo"]
        );

        assert_eq!(
            root.package_info()
                .direct_deps()
                .get("foo")
                .unwrap()
                .name()
                .as_str(),
            "foo"
        );
    }

    /// Loading a package includes the implicit dependencies, and the system dependencies are
    /// resolved to the right packages
    #[test(tokio::test)]
    async fn test_disabled_implicit_deps() {
        let scenario = TestPackageGraph::new(["root"])
            .add_package("a", |a| a.implicit_deps(false))
            .build();

        let root =
            RootPackage::<TestFlavor>::load(scenario.path_for("a"), default_environment(), vec![])
                .await
                .unwrap();

        assert!(root.package_info().direct_deps().is_empty());
    }

    /// Loading a package with an explicit dep with the same name as an implicit dep fails
    #[test(tokio::test)]
    async fn test_explicit_implicit() {
        let scenario = TestPackageGraph::new(["a", "b"])
            .add_dep("a", "b", |dep| dep.name("foo").rename_from("b"))
            .build();

        let err =
            RootPackage::<TestFlavor>::load(scenario.path_for("a"), default_environment(), vec![])
                .await
                .unwrap_err();

        assert_snapshot!(err.to_string(), @"The `foo` dependency is implicitly provided and should not be defined in your manifest.");
    }

    /// Loading a package with an explicit dep with the same name as an implicit succeeds if
    /// implicit deps are disabled
    #[test(tokio::test)]
    async fn test_explicit_with_implicits_disabled() {
        let scenario = TestPackageGraph::new(["dummy"])
            .add_package("a", |pkg| pkg.implicit_deps(false))
            .add_package("b", |pkg| pkg.implicit_deps(false))
            .add_dep("a", "b", |dep| dep.name("foo").rename_from("b"))
            .build();

        RootPackage::<TestFlavor>::load(scenario.path_for("a"), default_environment(), vec![])
            .await
            .unwrap();
    }

    fn new_package_name(name: &str) -> PackageName {
        PackageName::new(name.to_string()).unwrap()
    }
}
