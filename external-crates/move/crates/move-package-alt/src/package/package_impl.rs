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
    errors::{ManifestError, PackageError, PackageResult},
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
/// This is a special package that contains the project manifest and lockfile, and associated
/// functions to operate on the package and its dependencies.
pub struct RootPackage<F: MoveFlavor + fmt::Debug> {
    root: Package<F>,
    /// A possible empty lockfile (if there's no lockfile in the root directory).
    lockfile: Lockfile<F>,
    /// The dependency graphs for this root package, keyed by environment name.
    dep_graph: BTreeMap<EnvironmentName, PackageGraph<F>>,
}

impl<F: MoveFlavor + fmt::Debug> RootPackage<F> {
    /// Loads the root package from path and builds a dependency graph from the manifest.
    /// The lockfile is loaded from the same directory.
    pub async fn load(path: impl AsRef<Path>) -> PackageResult<Self> {
        let package_path = PackagePath::new(path.as_ref().to_path_buf())?;
        let package = Package::<F>::load_root(package_path.path()).await?;
        let mut dep_graph = BTreeMap::new();
        for e in package.manifest().environments().keys() {
            dep_graph.insert(
                e.to_string(),
                PackageGraph::<F>::load_from_manifests(&package_path, e).await?,
            );
        }

        let lockfile = Lockfile::<F>::read_from_dir(package_path.path())?;

        Ok(Self {
            root: package,
            lockfile,
            dep_graph,
        })
    }

    /// Load the root package and check if the lockfile is up-to-date. If it is not, then
    /// transitive dependencies will be re-pinned.
    pub async fn load_and_repin(path: impl AsRef<Path>) -> PackageResult<Self> {
        let package = Package::<F>::load_root(path).await?;
        let dep_graph = PackageGraph::<F>::load(&package.path()).await?;
        let lockfile = Lockfile::<F>::read_from_dir(&package.path().path())?;
        Ok(Self {
            root: package,
            lockfile,
            dep_graph,
        })
    }

    /// The package's manifest
    pub fn manifest(&self) -> &Manifest<F> {
        self.root.manifest()
    }

    /// The package's defined environments
    pub fn environments(&self) -> &BTreeMap<EnvironmentName, F::EnvironmentID> {
        self.manifest().environments()
    }

    /// The package's lockfile(s). If the loaded package has no lockfile, this will return an
    /// `empty` lockfile.
    pub fn lockfile(&self) -> &Lockfile<F> {
        &self.lockfile
    }

    /// Return a mutable reference to the lockfile for this package.
    pub fn lockfile_mut(&mut self) -> &mut Lockfile<F> {
        &mut self.lockfile
    }

    /// Return the defined package name in the manifest
    pub fn package_name(&self) -> &PackageName {
        self.manifest().package_name()
    }

    // *** GRAPH RELATED FUNCTIONS ***

    /// Build a dependency graph based on the manifest dependencies
    pub fn build_dep_graph(&self) {
        todo!()
    }

    pub fn load_dep_graph_from_lockfile(&self) {
        todo!()
    }

    // *** DEPS RELATED FUNCTIONS ***

    pub fn direct_deps() {
        todo!()
    }

    pub fn transitive_deps() {}

    /// Return the set of dependencies for the given package
    pub fn package_deps(
        &self,
        package: PackageName,
    ) -> PackageResult<DependencySet<PinnedDependencyInfo<F>>> {
        todo!()
    }

    /// Repin dependencies, update the [`dep_graph`] and the [`lockfile]`.
    pub async fn repin(
        &mut self,
        envs: Option<BTreeMap<EnvironmentName, F::EnvironmentID>>,
    ) -> PackageResult<()> {
        let mut dependencies_graph = BTreeMap::new();
        let envs = envs.unwrap_or(self.manifest().environments().clone());

        for e in envs.keys() {
            dependencies_graph.insert(
                e.to_string(),
                PackageGraph::<F>::load_from_manifests(self.package_path(), e).await?,
            );
        }

        self.dep_graph = dependencies_graph;

        for e in envs.keys() {
            let pinned_deps = self
                .dep_graph
                .get(e)
                .ok_or_else(|| {
                    PackageError::Generic(format!(
                        "Dependency graph for environment '{e}' not found"
                    ))
                })?
                .to_pinned_deps(self.package_path(), e)
                .await?;
            self.lockfile.update_pinned_dep_env(pinned_deps);
        }

        Ok(())
    }

    /// Serialize the lockfile(s) to disk. This will overwrite any existing lockfile(s) in the
    /// package directory, and create `Move.<env>.lock` files for any non-default environments.
    // TODO: I think we don't have defaults anymore, so we might need to fix write_to
    pub async fn serialize_lockfile(
        &self,
        envs: Option<BTreeMap<EnvironmentName, F::EnvironmentID>>,
    ) -> PackageResult<()> {
        self.lockfile.write_to(
            &self.root_path(),
            envs.unwrap_or(self.environments().clone()),
        )?;
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
