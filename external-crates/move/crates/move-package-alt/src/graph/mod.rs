// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod builder;
mod linkage;
mod package_info;
mod rename_from;
mod to_lockfile;

pub use linkage::LinkageError;
pub use package_info::{NamedAddress, PackageInfo};
pub use rename_from::RenameError;

use tracing::{debug, warn};

use std::{collections::BTreeMap, sync::Arc};

use crate::schema::Publication;
use crate::{
    dependency::PinnedDependencyInfo,
    errors::PackageResult,
    flavor::MoveFlavor,
    package::{Package, paths::PackagePath},
    schema::{Environment, PackageID, PackageName},
};
use bimap::BiBTreeMap;
use builder::PackageGraphBuilder;

use petgraph::{
    algo::toposort,
    graph::{DiGraph, NodeIndex},
};

#[derive(Debug, Clone)]
pub struct PackageGraphEdge {
    name: PackageName,
    dep: PinnedDependencyInfo,
}

/// The graph of all packages. May include multiple versions of "the same" package. Guaranteed to
/// be a rooted dag
#[derive(Debug)]
pub struct PackageGraph<F: MoveFlavor> {
    /// The root of the dag
    root_index: NodeIndex,

    /// The mapping between package ids and nodes
    /// Invariant: the indices in `package_ids` are the same as those in `inner`
    package_ids: BiBTreeMap<PackageID, NodeIndex>,

    /// The actual nodes and edges of the graph
    inner: DiGraph<Arc<Package<F>>, PackageGraphEdge>,
}

impl<F: MoveFlavor> PackageGraph<F> {
    /// Loads the package graph for each environment defined in the manifest. It checks whether the
    /// resolution graph in the lockfile inside `path` is up-to-date (i.e., whether any of the
    /// manifests digests are out of date). If the resolution graph is up-to-date, it is returned.
    /// Otherwise a new resolution graph is constructed by traversing (only) the manifest files.
    pub async fn load(path: &PackagePath, env: &Environment) -> PackageResult<Self> {
        let builder = PackageGraphBuilder::<F>::new();

        if let Some(graph) = builder.load_from_lockfile(path, env).await? {
            debug!("successfully loaded lockfile");
            Ok(graph)
        } else {
            debug!("lockfile was missing or out of date; loading from manifests");
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
        &self.inner[self.root_index]
    }

    /// Return all packages in the graph, indexed by their package ID
    pub(crate) fn all_packages(&self) -> BTreeMap<&PackageID, PackageInfo<F>> {
        self.package_ids
            .iter()
            .map(|(id, node)| (id, self.package_info(*node)))
            .collect()
    }

    /// Return the list of packages that are in the linkage table, as well as
    /// the unpublished ones in the package graph.
    // TODO: Do we want a way to access ALL packages and not the "de-duplicated" ones?
    // TODO: We probably want a deduplication function, and then we can just use `all_packages` for
    // this
    pub(crate) fn packages(&self) -> PackageResult<Vec<PackageInfo<F>>> {
        let linkage = self.linkage()?;

        // Populate ALL the linkage elements
        let mut result: Vec<PackageInfo<F>> = linkage.values().cloned().collect();

        // Add all nodes that exist but are not in the linkage table.
        // This is done to support unpublished packages, as linkage only includes
        // published packages.
        for node in self.inner.node_indices() {
            let package = &self.inner[node];

            if package
                .original_id()
                .is_some_and(|oid| linkage.contains_key(oid))
            {
                continue;
            }

            result.push(self.package_info(node));
        }

        Ok(result)
    }

    /// Return the sorted list of dependencies' name
    pub(crate) fn sorted_deps(&self) -> Vec<&PackageName> {
        let sorted = toposort(&self.inner, None).expect("to sort the graph");
        sorted
            .iter()
            .flat_map(|x| self.inner.node_weight(*x))
            .map(|x| x.name())
            .collect()
    }

    /// For each entry in `overrides`, override the package publication in `self` for the
    /// corresponding package ID. Warns if the package ID is unrecognized.
    pub(crate) fn add_publish_overrides(&mut self, overrides: BTreeMap<PackageID, Publication<F>>) {
        for (id, publish) in overrides {
            let Some(index) = self.package_ids.get_by_left(&id) else {
                warn!("Ignoring unrecognized package identifier `{id}`");
                continue;
            };
            self.inner[*index] = Arc::new(self.inner[*index].override_publish(publish));
        }
    }
}

#[cfg(test)]
mod tests {
    // TODO: example with a --[local]--> a/b --[local]--> a/c
    use std::collections::BTreeMap;

    use test_log::test;

    use crate::{
        flavor::Vanilla,
        graph::{PackageGraph, PackageInfo},
        schema::PackageName,
        test_utils::graph_builder::TestPackageGraph,
    };

    /// Return the packages in the graph, grouped by their name
    fn packages_by_name(
        graph: &PackageGraph<Vanilla>,
    ) -> BTreeMap<PackageName, PackageInfo<Vanilla>> {
        graph
            .packages()
            .expect("failed to get packages from graph")
            .into_iter()
            .map(|node| (node.name().clone(), node))
            .collect()
    }

    /// Root package `root` depends on `a` which depends on `b` which depends on `c`, which depends
    /// on `d`; `a`, `b`,
    /// `c`, and `d` are all legacy packages.
    ///
    /// Named addresses for 'a' should contain `c` and `d`
    #[test(tokio::test)]
    async fn modern_legacy_legacy_legacy_legacy() {
        let scenario = TestPackageGraph::new(["root"])
            .add_legacy_packages(["a", "b", "c", "d"])
            .add_deps([("root", "a"), ("a", "b"), ("b", "c"), ("c", "d")])
            .build();

        let graph = scenario.graph_for("root").await;

        let packages = packages_by_name(&graph);

        assert!(packages["a"].named_addresses().unwrap().contains_key("c"));
        assert!(packages["a"].named_addresses().unwrap().contains_key("d"));
        assert!(packages["a"].named_addresses().unwrap().contains_key("b"));
        assert!(packages["a"].named_addresses().unwrap().contains_key("a"));
        assert!(
            !packages["root"]
                .named_addresses()
                .unwrap()
                .contains_key("c")
        );
    }

    /// Root package `root` depends on `a` which depends on `b` which depends on `c` which depends
    /// on `d`; `a` and `c` are legacy packages.
    ///
    /// After adding legacy transitive deps, `a` should have direct dependencies on `c` and `d`
    /// (even though they "pass through" a modern package)
    #[test(tokio::test)]
    async fn modern_legacy_modern_legacy() {
        let scenario = TestPackageGraph::new(["root", "b", "d"])
            .add_legacy_packages(["legacy_a", "legacy_c"])
            .add_deps([
                ("root", "legacy_a"),
                ("legacy_a", "b"),
                ("b", "legacy_c"),
                ("legacy_c", "d"),
            ])
            .build();

        let graph = scenario.graph_for("root").await;

        let packages = packages_by_name(&graph);

        assert!(
            packages["legacy_a"]
                .named_addresses()
                .unwrap()
                .contains_key("legacy_c")
        );
        assert!(
            packages["legacy_a"]
                .named_addresses()
                .unwrap()
                .contains_key("d")
        );
        assert!(!packages["b"].named_addresses().unwrap().contains_key("d"));
    }

    // TODO: tests around name conflicts?
}
