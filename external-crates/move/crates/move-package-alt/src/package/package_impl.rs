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
use super::{
    lockfile::{Lockfile, Publication},
    paths::PackagePath,
};
use crate::{
    dependency::{DependencySet, PinnedDependencyInfo, pin},
    errors::{ManifestError, ManifestErrorKind::EnvironmentNotFound, PackageError, PackageResult},
    flavor::MoveFlavor,
    git::GitRepo,
    graph::PackageGraph,
};
use move_core_types::identifier::Identifier;
use tracing::{debug, info};

pub type EnvironmentName = String;
pub type PackageName = Identifier;

#[derive(Debug)]
pub struct Package<F: MoveFlavor + fmt::Debug> {
    // TODO: maybe hold a lock on the lock file? Maybe not if move-analyzer wants to hold on to a
    // Package long term?
    manifest: Manifest<F>,
    path: PackagePath,
}

/// A package that is defined as the root of a Move project.
///
/// This is a special package that contains the project manifest and dependencies' graphs,
/// and associated functions to operate with this data.
pub struct RootPackage<F: MoveFlavor + fmt::Debug> {
    /// The root package itself as a Package
    root: Package<F>,
    /// A map from an environment in the manifest to its dependency graph.
    dependencies: BTreeMap<EnvironmentName, PackageGraph<F>>,
}

impl<F: MoveFlavor + fmt::Debug> RootPackage<F> {
    /// Loads the root package from path and builds a dependency graph from the manifest. If `env`
    /// is passed, it will check that this environment exists in the manifest.
    // TODO: maybe we want to check multiple envs
    pub async fn load(path: impl AsRef<Path>, env: Option<EnvironmentName>) -> PackageResult<Self> {
        let package_path = PackagePath::new(path.as_ref().to_path_buf())?;
        let root = Package::<F>::load_root(package_path.path()).await?;
        let dependencies = if let Some(env) = env {
            if root.manifest().environments().get(&env).is_none() {
                return Err(PackageError::Generic(format!(
                    "Package {} does not have `{env}` defined as an environment in its manifest",
                    root.name(),
                )));
            }
            BTreeMap::from([(
                env.clone(),
                PackageGraph::<F>::load_from_manifest_by_env(&package_path, &env).await?,
            )])
        } else {
            PackageGraph::load_from_manifest(&package_path).await?
        };

        Ok(Self { root, dependencies })
    }

    /// Load the root package and check if the lockfile is up-to-date. If it is not, then
    /// all dependencies will be re-pinned.
    pub async fn load_and_repin(path: impl AsRef<Path>) -> PackageResult<Self> {
        let root = Package::<F>::load_root(path).await?;
        let dependencies = PackageGraph::<F>::load(&root.path()).await?;

        Ok(Self { root, dependencies })
    }

    /// Read the lockfile from the root directory
    pub fn load_lockfile(&self) -> PackageResult<Lockfile<F>> {
        Lockfile::read_from_dir(self.root_path())
    }

    /// The package's manifest
    pub fn manifest(&self) -> &Manifest<F> {
        self.root.manifest()
    }

    /// The package's defined environments
    pub fn environments(&self) -> &BTreeMap<EnvironmentName, F::EnvironmentID> {
        self.manifest().environments()
    }

    /// Return the defined package name in the manifest
    pub fn package_name(&self) -> &PackageName {
        self.manifest().package_name()
    }

    // *** DEPENDENCIES RELATED FUNCTIONS ***

    pub fn dependencies(&self) -> &BTreeMap<EnvironmentName, PackageGraph<F>> {
        &self.dependencies
    }

    /// Create a lockfile with the current package's dependencies. These dependencies will be
    /// rendered in the pinned section of the lockfile. The lockfile will have no published
    /// information.
    pub async fn dependencies_to_lockfile(&self) -> PackageResult<Lockfile<F>> {
        let mut lockfile = Lockfile::<F>::new(BTreeMap::new(), BTreeMap::new());

        for (env, graph) in self.dependencies() {
            lockfile.update_pinned_dep_env(graph.to_pinned_deps(self.package_path(), env).await?);
        }

        Ok(lockfile)
    }

    /// Build a dependency graph based on the manifest dependencies
    pub fn build_dep_graph(&self) {
        todo!()
    }

    pub fn load_dep_graph_from_lockfile(&self) {
        todo!()
    }

    // *** DEPS RELATED FUNCTIONS ***

    /// A map from an environment to the packages' direct dependencies
    pub async fn direct_dependencies(
        &self,
    ) -> PackageResult<BTreeMap<PackageName, PinnedDependencyInfo<F>>> {
        let mut output = BTreeMap::new();
        for (env, _) in self.environments() {
            output.extend(self.root.direct_deps(env).await?);
        }

        Ok(output)
    }

    /// A map from an environment to the packages' transitive dependencies
    // TODO: do we need this?
    pub fn transitive_deps() {}

    /// Return the set of dependencies for the given package
    // TODO: are package names unique? What happens when they're not?
    // TODO: do we need this?
    pub fn package_deps(
        &self,
        package: PackageName,
    ) -> PackageResult<DependencySet<PinnedDependencyInfo<F>>> {
        todo!()
    }

    /// Repin dependencies and update the [`dep_graph`].
    // TODO: do we need this?
    pub async fn repin(
        &mut self,
        envs: Option<BTreeMap<EnvironmentName, F::EnvironmentID>>,
    ) -> PackageResult<()> {
        let mut dependencies = BTreeMap::new();
        let envs = envs.unwrap_or(self.manifest().environments().clone());

        for env in envs.keys() {
            dependencies.insert(
                env.to_string(),
                PackageGraph::<F>::load_from_manifest_by_env(self.package_path(), env).await?,
            );
        }

        self.dependencies = dependencies;

        Ok(())
    }

    /// Repin dependencies for the given environments and write back to lockfile. If `envs` is
    /// None, it will re-pin for all environments defined in the manifest.
    ///
    /// Note that this will not update the [`dependencies`] field itself.
    pub async fn update_deps_and_write_to_lockfile(
        &self,
        envs: Option<BTreeMap<EnvironmentName, F::EnvironmentID>>,
    ) -> PackageResult<()> {
        let envs = envs.unwrap_or(self.environments().clone());
        let mut deps = BTreeMap::new();
        for env in envs.keys() {
            let graph =
                PackageGraph::<F>::load_from_manifest_by_env(self.package_path(), env).await?;
            let pinned_deps = graph.to_pinned_deps(self.package_path(), env).await?;
            deps.extend(pinned_deps);
        }

        let lockfile = self.load_lockfile()?;
        lockfile.updated_deps_to_lockfile(self.root_path(), deps, envs);

        Ok(())
    }

    // *** PATHS RELATED FUNCTIONS ***

    /// Return the package path wrapper
    pub fn package_path(&self) -> &PackagePath {
        self.root.path()
    }

    /// Return the path to this package's manifest
    pub fn manifest_path(&self) -> PathBuf {
        self.package_path().manifest_path()
    }

    /// Return the path to this package's `Move.lock` lockfile
    pub fn lockfile_path(&self) -> PathBuf {
        self.package_path().lockfile_path()
    }

    /// The root path of this package
    pub fn root_path(&self) -> &PathBuf {
        self.package_path().path()
    }
}

impl<F: MoveFlavor> Package<F> {
    /// Load a package from the manifest.
    /// Makes a best effort to translate old-style packages into the current format,
    ///
    /// Fails if [path] does not exist, or if it doesn't contain a manifest
    pub async fn load_root(path: impl AsRef<Path>) -> PackageResult<Self> {
        let path = PackagePath::new(path.as_ref().to_path_buf())?;
        let manifest = Manifest::<F>::read_from_file(path.manifest_path())?;
        Ok(Self { manifest, path })
    }

    /// Fetch [dep] and load a package from the fetched source
    /// Makes a best effort to translate old-style packages into the current format,
    pub async fn load(dep: PinnedDependencyInfo<F>) -> PackageResult<Self> {
        let path = PackagePath::new(dep.fetch().await?)?;
        let manifest = Manifest::<F>::read_from_file(path.manifest_path())?;

        Ok(Self { manifest, path })
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
    ) -> PackageResult<BTreeMap<PackageName, PinnedDependencyInfo<F>>> {
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
        let pinned_deps = pin(&F::new(), deps.clone(), &envs).await?;

        Ok(pinned_deps
            .into_iter()
            .map(|(_, id, dep)| (id, dep))
            .collect())
    }
}
