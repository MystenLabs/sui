// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod builder;
mod linkage;
mod rename_from;
mod to_lockfile;

pub use linkage::LinkageError;
pub use rename_from::RenameError;

use std::{collections::BTreeMap, sync::Arc};

use crate::{
    dependency::PinnedDependencyInfo,
    errors::PackageResult,
    flavor::MoveFlavor,
    package::{EnvironmentName, Package, paths::PackagePath},
    schema::{Environment, PackageName, PublishAddresses},
};
use builder::PackageGraphBuilder;

use derive_where::derive_where;
use petgraph::{
    graph::{DiGraph, EdgeIndex, NodeIndex},
    visit::EdgeRef,
};

#[derive(Debug)]
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

/// A narrow interface for representing packages outside of `move-package-alt`
#[derive(Copy)]
#[derive_where(Clone)]
pub struct PackageInfo<'a, F: MoveFlavor> {
    graph: &'a PackageGraph<F>,
    node: NodeIndex,
}

impl<'graph, F: MoveFlavor> PackageInfo<'graph, F> {
    /// The name that the package has declared for itself
    pub fn name(&self) -> &PackageName {
        self.package().name()
    }

    /// The compiler edition for the package
    pub fn edition(&self) -> Option<&str> {
        // TODO: pull this from manifest
        Some("2024")
    }

    /// The flavor for the package
    pub fn flavor(&self) -> Option<&str> {
        // TODO: pull this from manifest
        Some("sui")
    }

    /// The path to the package's files on disk
    pub fn path(&self) -> &PackagePath {
        self.package().path()
    }

    /// Returns the published address of this package, if it is published
    pub fn published(&self) -> Option<&PublishAddresses> {
        self.package().publication()
    }

    /// Returns true if the node is the root of the package graph
    pub fn is_root(&self) -> bool {
        self.package().is_root()
    }

    /// The addresses for the names that are available to this package. For modern packages, this
    /// contains only the package and its dependencies, but legacy packages may define additional
    /// addresses as well
    pub fn named_addresses(&self) -> BTreeMap<PackageName, PackageInfo<'graph, F>> {
        let mut result: BTreeMap<PackageName, PackageInfo<F>> = self
            .graph
            .inner
            .edges(self.node)
            .map(|edge| {
                (
                    edge.weight().clone(),
                    Self {
                        graph: self.graph,
                        node: edge.target(),
                    },
                )
            })
            .collect();
        result.insert(self.package().name().clone(), self.clone());

        result
    }

    /// The package corresponding to this node
    fn package(&self) -> &Package<F> {
        &self.graph.inner[self.node].package
    }
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
    fn dep_for_edge(&self, edge: EdgeIndex) -> &PinnedDependencyInfo {
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

    /// Return the list of dependencies in this package graph
    pub(crate) fn dependencies(&self) -> Vec<PackageInfo<F>> {
        self.inner
            .node_indices()
            .map(|node| PackageInfo { graph: self, node })
            .collect()
    }
}
