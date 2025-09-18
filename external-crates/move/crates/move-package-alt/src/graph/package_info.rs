use std::collections::BTreeMap;

use derive_where::derive_where;
use petgraph::{Direction, graph::NodeIndex, visit::EdgeRef};

use crate::{
    compatibility::legacy_parser::NO_NAME_LEGACY_PACKAGE_NAME,
    dependency::PinnedDependencyInfo,
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::{Package, paths::PackagePath},
    schema::{OriginalID, PackageID, PackageName, PublishAddresses},
};

use super::PackageGraph;

/// A narrow interface for representing packages outside of `move-package-alt`
#[derive(Copy)]
#[derive_where(Clone)]
pub struct PackageInfo<'graph, F: MoveFlavor> {
    // TODO: this code really needs a little reorganization (pub(super)?)
    pub(super) graph: &'graph PackageGraph<F>,
    pub(super) node: NodeIndex,
}

// TODO: `NamedAddress` is a terrible name for this
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum NamedAddress {
    RootPackage(Option<OriginalID>),
    Unpublished { dummy_addr: OriginalID },
    Defined(OriginalID),
}

impl<F: MoveFlavor> PackageGraph<F> {
    /// Return the `PackageInfo` for the root package
    pub fn root_package_info(&self) -> PackageInfo<F> {
        self.package_info(self.root_index)
    }

    /// Return a `PackageInfo` for `node`
    pub(crate) fn package_info(&self, node: NodeIndex) -> PackageInfo<F> {
        PackageInfo { graph: self, node }
    }

    /// Return the `PackageInfo` for id `id`, if one exists
    pub fn package_info_by_id(&self, id: &PackageID) -> Option<PackageInfo<F>> {
        self.package_ids
            .get_by_left(id)
            .map(|node| self.package_info(*node))
    }
}

impl<'graph, F: MoveFlavor> PackageInfo<'graph, F> {
    /// The name that the package has declared for itself
    pub fn name(&self) -> &PackageName {
        self.package().name()
    }

    /// Returns the `display_name` for a given package.
    /// Invariant: For modern packages, this is always equal to `name().as_str()`
    pub fn display_name(&self) -> &str {
        self.package().display_name()
    }

    /// Produce a string of identifiers from the root to this package for identifying the package
    /// in error messages
    pub fn display_path(&self) -> String {
        if let Some(incoming) = self
            .graph
            .inner
            .edges_directed(self.node, Direction::Incoming)
            .next()
        {
            let parent = PackageInfo {
                graph: self.graph,
                node: incoming.source(),
            };
            let mut result = parent.display_path();
            result.push_str("::");
            result.push_str(incoming.weight().name.as_str());
            result
        } else {
            self.package().name().to_string()
        }
    }

    /// The unique ID for this package in the package graph
    pub fn id(&self) -> &'graph PackageID {
        self.graph
            .package_ids
            .get_by_right(&self.node)
            .expect("all nodes are in the graph")
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
    ///
    /// Note that if the graph has been updated (using [PackageGraph::add_publish_overrides]), the
    /// updated address is returned.
    pub fn published(&self) -> Option<&PublishAddresses> {
        self.package()
            .publication()
            .map(|publication| &publication.addresses)
    }

    /// Returns true if the node is the root of the package graph
    pub fn is_root(&self) -> bool {
        self.graph
            .inner
            .edges_directed(self.node, Direction::Incoming)
            .next()
            .is_none()
    }

    /// Return an original id for this node; using the dummy address if needed
    pub(crate) fn original_id(&self) -> OriginalID {
        match self.node_to_addr(self.node) {
            NamedAddress::RootPackage(original_id) => original_id.unwrap_or(0.into()),
            NamedAddress::Unpublished { dummy_addr } => dummy_addr,
            NamedAddress::Defined(original_id) => original_id,
        }
    }

    /// Return the package information for the direct dependencies of this package
    pub(crate) fn direct_deps(&self) -> BTreeMap<PackageName, PackageInfo<'graph, F>> {
        self.graph
            .inner
            .edges(self.node)
            .map(|edge| {
                (
                    edge.weight().name.clone(),
                    Self {
                        graph: self.graph,
                        node: edge.target(),
                    },
                )
            })
            .collect()
    }

    /// Return a dependency that resolves to this package
    pub(crate) fn dep_for_self(&self) -> &PinnedDependencyInfo {
        // TODO: maybe pull this from the graph instead of storing it in the `Package`?
        self.package().dep_for_self()
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

        // We only add the package name if it is not the special "unnamed" package name
        if self.package().name().as_str() != NO_NAME_LEGACY_PACKAGE_NAME {
            result.insert(self.package().name().clone(), self.node_to_addr(self.node));
        }

        for (_, dep) in self.direct_deps() {
            let transitive_result = dep.legacy_named_addresses()?;

            for (name, addr) in transitive_result {
                let existing = result.insert(name.clone(), addr.clone());

                if existing.is_some_and(|existing| existing != addr) {
                    return Err(PackageError::DuplicateNamedAddress {
                        address: name,
                        package: self.package().display_name().to_string(),
                    });
                }
            }
        }

        if let Some(legacy_data) = &self.package().legacy_data {
            let addresses = legacy_data.named_addresses.clone();

            for (name, addr) in addresses {
                let new_addr = NamedAddress::Defined(OriginalID(addr));
                let existing = result.insert(name.clone(), new_addr.clone());

                if existing.is_some_and(|existing| existing != new_addr) {
                    return Err(PackageError::DuplicateNamedAddress {
                        address: name,
                        package: self.package().display_name().to_string(),
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
            return NamedAddress::RootPackage(package.original_id().cloned());
        }
        if let Some(oid) = package.original_id() {
            NamedAddress::Defined(oid.clone())
        } else {
            NamedAddress::Unpublished {
                dummy_addr: package.dummy_addr.clone(),
            }
        }
    }

    /// The package corresponding to this node
    pub(crate) fn package(&self) -> &Package<F> {
        &self.graph.inner[self.node]
    }
}

impl<F: MoveFlavor> std::fmt::Debug for PackageInfo<'_, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.package().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    // TODO: example with a --[local]--> a/b --[local]--> a/c
    use std::collections::BTreeMap;

    use insta::assert_snapshot;
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

    /// ```mermaid
    /// graph LR
    ///     root --> |"a (legacy)"| --> |"b (legacy)"| --> |"c (legacy)"| --> |"d (legacy)"|
    /// ```
    ///
    /// Named addresses for `a` should contain `b`, `c`, and `d`
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

    /// ```mermaid
    /// graph LR
    ///     root --> |"a (legacy)"| --> b --> |"c (legacy)"| --> d
    /// ```
    ///
    /// After adding legacy transitive deps, `a` should have direct dependencies on `c` and `d`
    /// (even though they "pass through" a modern package)
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
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

    /// In the following package graph for `a`, calling `d.display_path` should return `a::x::y::d`:
    ///
    /// ```mermaid
    /// graph LR
    ///     a -->|"x = {..., rename-from=b}"| b -->|"y = {..., rename-from=c}"| c --> d
    /// ```
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn display_path() {
        let scenario = TestPackageGraph::new(["a", "b", "c", "d"])
            .add_dep("a", "b", |dep| dep.name("x").rename_from("b"))
            .add_dep("b", "c", |dep| dep.name("y").rename_from("c"))
            .add_deps([("c", "d")])
            .build();

        let graph = scenario.graph_for("a").await;
        let packages = packages_by_name(&graph);

        assert_snapshot!(packages["d"].display_path(), @"a::x::y::d");
    }
}
