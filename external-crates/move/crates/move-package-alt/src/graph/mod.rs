// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod builder;
mod linkage;
mod rename_from;
mod to_lockfile;

pub use linkage::LinkageError;
pub use rename_from::RenameError;

use std::sync::Arc;

use crate::{
    dependency::PinnedDependencyInfo,
    errors::PackageResult,
    flavor::MoveFlavor,
    package::{EnvironmentName, Package, paths::PackagePath},
    schema::{Environment, PackageName},
};
use builder::PackageGraphBuilder;

use derive_where::derive_where;
use petgraph::{
    algo::toposort,
    graph::{DiGraph, EdgeIndex, NodeIndex},
};

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

    /// Return the dependency corresponding to `edge`
    pub fn dep_for_edge(&self, edge: EdgeIndex) -> &PinnedDependencyInfo {
        let (source_index, _) = self
            .inner
            .edge_endpoints(edge)
            .expect("edge is a valid index into the graph");

        self.inner[source_index]
            .package
            .direct_deps()
            .get(&self.inner[edge])
            .expect("edges in graph come from dependencies, so the dependency must exist")
    }

    /// Return a list of package names that are topologically sorted
    pub fn sorted_deps(&self) -> Vec<PackageName> {
        let sorted = toposort(&self.inner, None).expect("non cyclic directed graph");

        sorted
            .into_iter()
            .flat_map(|idx| {
                self.inner
                    .node_weight(idx)
                    .map(|f| f.package.name().clone())
            })
            .collect()
    }

    /// Return the list of dependencies in this package graph
    pub(crate) fn dependencies(&self) -> Vec<Arc<Package<F>>> {
        let mut output = vec![];

        for (idx, n) in self.inner.node_weights().enumerate() {
            if NodeIndex::new(idx) == self.root_idx {
                continue;
            }

            output.push(n.package.clone())
        }

        output
    }
}
