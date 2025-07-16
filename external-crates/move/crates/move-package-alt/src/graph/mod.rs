// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod builder;
mod linkage;
mod to_lockfile;

pub use linkage::LinkageError;

use std::sync::Arc;

use crate::{
    errors::PackageResult,
    flavor::MoveFlavor,
    package::{EnvironmentName, Package, paths::PackagePath},
    schema::{Environment, PackageName},
};
use builder::PackageGraphBuilder;

use derive_where::derive_where;
use petgraph::graph::{DiGraph, NodeIndex};

#[derive(Debug)]
#[derive_where(Clone)]
pub struct PackageGraph<F: MoveFlavor> {
    root_idx: NodeIndex,
    inner: DiGraph<PackageNode<F>, PackageName>,
}

/// A node in the package graph, containing a [Package] in a particular environment
#[derive(Debug)]
#[derive_where(Clone)]
struct PackageNode<F: MoveFlavor> {
    package: Arc<Package<F>>,
    use_env: EnvironmentName,
}

impl<F: MoveFlavor> PackageGraph<F> {
    /// Loads the package graph for each environment defined in the manifest. It checks whether the
    /// resolution graph in the lockfile inside `path` is up-to-date (i.e., whether any of the
    /// manifests digests are out of date). If the resolution graph is up-to-date, it is returned.
    /// Otherwise a new resolution graph is constructed by traversing (only) the manifest files.
    pub async fn load(path: &PackagePath, env: &Environment) -> PackageResult<Self> {
        let package = Package::<F>::load_root(path.path(), env).await?;

        let builder = PackageGraphBuilder::<F>::new();

        if let Some(graph) = builder.load_from_lockfile(path, env).await? {
            Ok(graph)
        } else {
            builder.load_from_manifests(path, env).await
        }
    }

    /// Construct a [PackageGraph] by pinning and fetching all transitive dependencies from the
    /// manifests rooted at `path` (no lockfiles are read) for the passed environment.
    pub async fn load_from_manifests(path: &PackagePath, env: &Environment) -> PackageResult<Self> {
        PackageGraphBuilder::new()
            .load_from_manifests(path, env)
            .await
    }

    /// Read a [PackageGraph] from a lockfile, ignoring manifest digests. Primarily useful for
    /// testing - you will usually want [Self::load].
    /// TODO: probably want to take a path to the lockfile
    pub async fn load_from_lockfile_ignore_digests(
        path: &PackagePath,
        env: &Environment,
    ) -> PackageResult<Option<Self>> {
        PackageGraphBuilder::new()
            .load_from_lockfile_ignore_digests(path, env)
            .await
    }

    /// Returns the root package of the graph.
    pub fn root_package(&self) -> &Package<F> {
        self.inner[self.root_idx].package.as_ref()
    }
}
