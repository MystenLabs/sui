// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod builder;
mod linkage;
mod rename_from;
mod to_lockfile;

pub use linkage::LinkageError;
pub use rename_from::RenameError;
use tracing::debug;

use std::{collections::BTreeMap, sync::Arc};

use crate::{
    dependency::PinnedDependencyInfo,
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::{Package, paths::PackagePath},
    schema::{Environment, OriginalID, PackageName, PublishAddresses},
};
use builder::PackageGraphBuilder;

use derive_where::derive_where;
use petgraph::{
    algo::toposort,
    graph::{DiGraph, NodeIndex},
    visit::EdgeRef,
};

#[derive(Debug, Clone)]
pub struct PackageGraphEdge {
    name: PackageName,
    dep: PinnedDependencyInfo,
}

#[derive(Debug)]
pub struct PackageGraph<F: MoveFlavor> {
    root_index: NodeIndex,
    inner: DiGraph<Arc<Package<F>>, PackageGraphEdge>,
}

/// A narrow interface for representing packages outside of `move-package-alt`
#[derive(Copy)]
#[derive_where(Clone)]
pub struct PackageInfo<'a, F: MoveFlavor> {
    graph: &'a PackageGraph<F>,
    node: NodeIndex,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NamedAddress {
    RootPackage(Option<OriginalID>),
    Unpublished { dummy_addr: OriginalID },
    Defined(OriginalID),
}

impl<F: MoveFlavor> PackageInfo<'_, F> {
    /// The name that the package has declared for itself
    pub fn name(&self) -> &PackageName {
        self.package().name()
    }

    /// The compiler edition for the package
    pub fn edition(&self) -> &str {
        self.package().metadata().edition.as_str()
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
    pub fn named_addresses(&self) -> PackageResult<BTreeMap<PackageName, NamedAddress>> {
        if self.package().is_legacy() {
            return self.legacy_named_addresses();
        }

        let mut result: BTreeMap<PackageName, NamedAddress> = self
            .graph
            .inner
            .edges(self.node)
            .map(|edge| (edge.weight().name.clone(), self.node_to_addr(edge.target())))
            .collect();
        result.insert(self.package().name().clone(), self.node_to_addr(self.node));

        Ok(result)
    }

    /// For legacy packages, our named addresses need to include all transitive deps too.
    /// An example of that is depending on "sui", but also keeping it possible to use "std".
    fn legacy_named_addresses(&self) -> PackageResult<BTreeMap<PackageName, NamedAddress>> {
        let mut result: BTreeMap<PackageName, NamedAddress> = BTreeMap::new();

        result.insert(self.package().name().clone(), self.node_to_addr(self.node));

        for edge in self.graph.inner.edges(self.node) {
            let name = edge.weight().name.clone();
            let dep = Self {
                graph: self.graph,
                node: edge.target(),
            };

            let transitive_result = dep.legacy_named_addresses()?;

            for (name, addr) in transitive_result {
                let existing = result.insert(name.clone(), addr.clone());

                if existing.is_some_and(|existing| existing != addr) {
                    return Err(PackageError::DuplicateNamedAddress {
                        address: name,
                        package: self.package().name().clone(),
                    });
                }
            }
        }

        if let Some(legacy_data) = &self.package().legacy_data {
            let addresses = legacy_data.addresses.clone();

            for (name, addr) in addresses {
                let new_addr = NamedAddress::Defined(OriginalID(addr));
                let existing = result.insert(name.clone(), new_addr.clone());

                if existing.is_some_and(|existing| existing != new_addr) {
                    return Err(PackageError::DuplicateNamedAddress {
                        address: name,
                        package: self.package().name().clone(),
                    });
                }
            }
        }

        Ok(result)
    }

    /// Return the NamedAddress for `node`
    fn node_to_addr(&self, node: NodeIndex) -> NamedAddress {
        let package = self.graph.inner[node].clone();
        if package.is_root() {
            return NamedAddress::RootPackage(package.original_id());
        }
        if let Some(oid) = package.original_id() {
            NamedAddress::Defined(oid)
        } else {
            NamedAddress::Unpublished {
                dummy_addr: package.dummy_addr.clone(),
            }
        }
    }

    /// The package corresponding to this node
    fn package(&self) -> &Package<F> {
        &self.graph.inner[self.node]
    }
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

    pub fn root_package_info(&self) -> PackageInfo<'_, F> {
        PackageInfo {
            graph: self,
            node: self.root_index,
        }
    }

    /// Return the list of packages that are in the linkage table, as well as
    /// the unpublished ones in the package graph.
    // TODO: Do we want a way to access ALL packages and not the "de-duplicated" ones?
    pub(crate) fn packages(&self) -> PackageResult<Vec<PackageInfo<'_, F>>> {
        let mut linkage = self.linkage()?;

        // Populate ALL the linkage elements
        let mut result: Vec<PackageInfo<F>> = linkage
            .values()
            .cloned()
            .map(|node| PackageInfo { graph: self, node })
            .collect();

        // Add all nodes that exist but are not in the linkage table.
        // This is done to support unpublished packages, as linkage only includes
        // published packages.
        for node in self.inner.node_indices() {
            let package = &self.inner[node];

            if package
                .original_id()
                .is_some_and(|oid| linkage.contains_key(&oid))
            {
                continue;
            }

            result.push(PackageInfo { graph: self, node });
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

        let mut graph = scenario.graph_for("root").await;

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

        let mut graph = scenario.graph_for("root").await;

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
