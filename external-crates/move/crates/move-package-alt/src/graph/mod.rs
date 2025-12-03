// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod builder;
mod linkage;
mod package_info;
mod rename_from;
mod to_lockfile;

pub use builder::LockfileError;
use derive_where::derive_where;
pub use linkage::{LinkageError, LinkageTable};
pub use package_info::{NamedAddress, PackageInfo};
use petgraph::visit::EdgeRef;
pub use rename_from::RenameError;

use tracing::debug;

use std::{collections::BTreeMap, sync::Arc};

use crate::package::package_lock::PackageSystemLock;
use crate::schema::{LockfileDependencyInfo, ModeName, Publication};
use crate::{
    dependency::PinnedDependencyInfo,
    errors::PackageResult,
    flavor::MoveFlavor,
    package::{Package, paths::PackagePath},
    schema::{Environment, PackageID},
};
use bimap::BiBTreeMap;
use builder::PackageGraphBuilder;

use petgraph::{
    algo::toposort,
    graph::{DiGraph, NodeIndex},
};

/// The graph of all packages. May include multiple versions of "the same" package. Guaranteed to
/// be a rooted dag
#[derive(Debug)]
#[derive_where(Clone)]
pub(crate) struct PackageGraph<F: MoveFlavor> {
    /// The root of the dag
    root_index: NodeIndex,

    /// The mapping between package ids and nodes
    /// Invariant: the indices in `package_ids` are the same as those in `inner`
    package_ids: BiBTreeMap<PackageID, NodeIndex>,

    /// The actual nodes and edges of the graph
    inner: DiGraph<Arc<Package<F>>, PinnedDependencyInfo>,
}

impl<F: MoveFlavor> PackageGraph<F> {
    /// Loads the package graph for each environment defined in the manifest. It checks whether the
    /// resolution graph in the lockfile inside `path` is up-to-date (i.e., whether any of the
    /// manifests digests are out of date). If the resolution graph is up-to-date, it is returned.
    /// Otherwise a new resolution graph is constructed by traversing (only) the manifest files.
    pub async fn load(
        path: &PackagePath,
        env: &Environment,
        mtx: &PackageSystemLock,
    ) -> PackageResult<Self> {
        let builder = PackageGraphBuilder::<F>::new();

        if let Some(graph) = builder.load_from_lockfile(path, env, mtx).await? {
            debug!("successfully loaded lockfile");
            Ok(graph)
        } else {
            debug!("lockfile was missing or out of date; loading from manifests");
            builder.load_from_manifests(path, env, mtx).await
        }
    }

    /// Construct a [PackageGraph] by pinning and fetching all transitive dependencies from the
    /// manifests rooted at `path` (no lockfiles are read) for the passed environment.
    pub async fn load_from_manifests(
        path: &PackagePath,
        env: &Environment,
        mtx: &PackageSystemLock,
    ) -> PackageResult<Self> {
        PackageGraphBuilder::new()
            .load_from_manifests(path, env, mtx)
            .await
    }

    /// Read a [PackageGraph] from a lockfile, ignoring manifest digests. Primarily useful for
    /// testing - you will usually want [Self::load].
    /// TODO: probably want to take a path to the lockfile
    pub async fn load_from_lockfile_ignore_digests(
        path: &PackagePath,
        env: &Environment,
        mtx: &PackageSystemLock,
    ) -> PackageResult<Option<Self>> {
        PackageGraphBuilder::new()
            .load_from_lockfile_ignore_digests(path, env, mtx)
            .await
    }

    /// Returns the root package of the graph.
    pub fn root_package(&self) -> &Package<F> {
        &self.inner[self.root_index]
    }

    /// Return the list of all packages that are in the package graph. Note that depending on whether the
    /// graph has been filtered or not, this may contain multiple packages with the same original
    /// ID
    pub fn packages(&self) -> Vec<PackageInfo<F>> {
        self.inner
            .node_indices()
            .map(|node| self.package_info(node))
            .collect()
    }

    /// Return the list of all packages that are in the package graph, sorted in topological order.
    pub fn sorted_packages(&self) -> Vec<PackageInfo<F>> {
        let sorted = toposort(&self.inner, None).expect("to sort the graph");
        sorted.iter().map(|x| self.package_info(*x)).collect()
    }

    /// For each entry in `overrides`, override the package publication in `self` for the
    /// corresponding dependency. Warns if the package ID is unrecognized.
    pub fn add_publish_overrides(
        &mut self,
        overrides: BTreeMap<LockfileDependencyInfo, Publication<F>>,
    ) {
        for (_, index) in &self.package_ids {
            let dep = self.inner[*index].dep_for_self().clone().into();
            if let Some(publish) = overrides.get(&dep) {
                self.inner[*index] = Arc::new(self.inner[*index].override_publish(publish.clone()));
            }
        }
    }

    /// Return a copy of `self` with all moded dependencies that don't match `mode` filtered out
    pub fn filter_for_mode(&self, modes: &Vec<ModeName>) -> Self {
        let mut result = Self {
            root_index: NodeIndex::from(0),
            package_ids: BiBTreeMap::new(),
            inner: DiGraph::new(),
        };

        result.root_index = self.copy_moded(&mut result, self.root_index, modes);

        result
    }

    /// Copy subgraph rooted at `node` into `dest`, filtering out dependencies that don't match
    /// `modes`. Returns the index for `node` in the new graph
    fn copy_moded(&self, dest: &mut Self, node: NodeIndex, modes: &Vec<ModeName>) -> NodeIndex {
        let package_id = self
            .package_ids
            .get_by_right(&node)
            .expect("node is in the graph");

        if let Some(index) = dest.package_ids.get_by_left(package_id) {
            return *index;
        }

        let index = dest.inner.add_node(self.inner[node].clone());
        dest.package_ids.insert(package_id.clone(), index);

        for edge in self.inner.edges(node) {
            if let Some(dep_modes) = edge.weight().modes()
                && !modes.iter().any(|mode| dep_modes.contains(mode))
            {
                // dependency is moded but doesn't contain the modes we're allowing;
                // skip adding the dep to the new graph
                continue;
            }

            let dst_index = self.copy_moded(dest, edge.target(), modes);
            dest.inner.add_edge(index, dst_index, edge.weight().clone());
        }

        index
    }

    /// Return a `PackageInfo` for `id`. Panics if the ID is not present
    fn get_package(&self, id: &PackageID) -> PackageInfo<F> {
        let node = self
            .package_ids
            .get_by_left(id)
            .expect("all IDs have nodes");
        self.package_info(*node)
    }
}

#[cfg(test)]
mod tests {
    use test_log::test;

    use crate::{schema::PackageID, test_utils::graph_builder::TestPackageGraph};

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a -->|test| b --> c
    ///     a --> d --> |spec| e
    /// ```
    ///
    /// If an edge has a mode, it should be dropped if there are no modes passed, so after calling
    /// `filter_modes([])`, the graph should look like
    /// ```mermaid
    ///     root --> a --> d
    /// ```
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_mode_filter_no_modes() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c", "d", "e"])
            .add_deps([("root", "a"), ("b", "c"), ("a", "d")])
            .add_dep("a", "b", |dep| dep.modes(["test"]))
            .add_dep("d", "e", |dep| dep.modes(["spec"]))
            .build();

        let graph = scenario.graph_for("root").await;
        let filtered = graph.filter_for_mode(&vec![]);

        let ids: Vec<PackageID> = filtered
            .package_ids
            .into_iter()
            .map(|(pkg, _)| pkg)
            .collect();

        assert_eq!(ids, ["a", "d", "root"]);
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a -->|test| b --> c
    ///     a --> d --> |spec| e
    /// ```
    ///
    /// If an edge has a mode but some other mode is passed, we should drop the edge, so after
    /// calling `filter_modes(["test"])`, the graph should look like
    /// ```mermaid
    ///     root --> a --> b --> c
    ///     a --> d
    /// ```
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_mode_filter_one_mode() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c", "d", "e"])
            .add_deps([("root", "a"), ("b", "c"), ("a", "d")])
            .add_dep("a", "b", |dep| dep.modes(["test"]))
            .add_dep("d", "e", |dep| dep.modes(["spec"]))
            .build();

        let graph = scenario.graph_for("root").await;
        let filtered = graph.filter_for_mode(&vec!["test".into()]);

        let ids: Vec<PackageID> = filtered
            .package_ids
            .into_iter()
            .map(|(pkg, _)| pkg)
            .collect();

        assert_eq!(ids, ["a", "b", "c", "d", "root"]);
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a -->|test| b --> c
    ///     a --> d --> |spec| e
    /// ```
    ///
    /// If we pass multiple modes, we should include all edges that match any of the passed modes,
    /// so after calling `filter_modes(["test", "spec"])`, the graph should look like
    /// ```mermaid
    ///     root --> a --> b --> c
    ///     a --> d --> e
    /// ```
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_mode_filter_multimodes() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c", "d", "e"])
            .add_deps([("root", "a"), ("b", "c"), ("a", "d")])
            .add_dep("a", "b", |dep| dep.modes(["test"]))
            .add_dep("d", "e", |dep| dep.modes(["spec"]))
            .build();

        let graph = scenario.graph_for("root").await;
        let filtered = graph.filter_for_mode(&vec!["test".into(), "spec".into()]);

        let ids: Vec<PackageID> = filtered
            .package_ids
            .into_iter()
            .map(|(pkg, _)| pkg)
            .collect();

        assert_eq!(ids, ["a", "b", "c", "d", "e", "root"]);
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a -->|test, spec| b --> c
    /// ```
    ///
    /// Here, `b` should be included for both `spec` and `test` modes, so after calling
    /// `filter_modes(["test"])`, the graph should look like
    /// ```mermaid
    ///     root --> a --> b --> c
    /// ```
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_multimode_filter() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_deps([("root", "a"), ("b", "c")])
            .add_dep("a", "b", |dep| dep.modes(["test", "spec"]))
            .build();

        let graph = scenario.graph_for("root").await;
        let filtered = graph.filter_for_mode(&vec!["test".into()]);

        let ids: Vec<PackageID> = filtered
            .package_ids
            .into_iter()
            .map(|(pkg, _)| pkg)
            .collect();

        assert_eq!(ids, ["a", "b", "c", "root"]);
    }
}
