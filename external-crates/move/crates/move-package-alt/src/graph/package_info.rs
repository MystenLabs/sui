use std::collections::BTreeMap;

use derive_where::derive_where;
use petgraph::{Direction, graph::NodeIndex, visit::EdgeRef};

use crate::{
    compatibility::legacy_parser::NO_NAME_LEGACY_PACKAGE_NAME,
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::{Package, paths::PackagePath},
    schema::{OriginalID, PackageID, PackageName, PublishAddresses},
};

use super::PackageGraph;
use move_compiler::editions::Edition;

/// A narrow interface for representing packages outside of `move-package-alt`. Note that
/// at different points in the package system we use graphs that have been filtered in different
/// ways; the package info has the same invariants as its underlying graph.
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
    pub fn root_package_info(&self) -> PackageInfo<'_, F> {
        self.package_info(self.root_index)
    }

    /// Return a `PackageInfo` for `node`
    pub(crate) fn package_info(&self, node: NodeIndex) -> PackageInfo<'_, F> {
        PackageInfo { graph: self, node }
    }

    /// Return the `PackageInfo` for id `id`, if one exists
    #[cfg(test)]
    pub fn package_info_by_id(&self, id: &PackageID) -> Option<PackageInfo<'_, F>> {
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
            result.push_str(incoming.weight().name().as_str());
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
    pub fn edition(&self) -> Option<Edition> {
        self.package().metadata().edition
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
                    edge.weight().name().clone(),
                    Self {
                        graph: self.graph,
                        node: edge.target(),
                    },
                )
            })
            .collect()
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
            .map(|edge| {
                (
                    edge.weight().name().clone(),
                    self.node_to_addr(edge.target()),
                )
            })
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
            self.legacy_insert_unique_or_error(
                &mut result,
                self.package().name().clone(),
                self.node_to_addr(self.node),
            )?;
        }

        for (_, dep) in self.direct_deps() {
            let transitive_result = dep.legacy_named_addresses()?;

            for (name, addr) in transitive_result {
                self.legacy_insert_unique_or_error(&mut result, name.clone(), addr.clone())?;
            }
        }

        if let Some(legacy_data) = &self.package().legacy_data {
            let addresses = legacy_data.named_addresses.clone();

            for (name, addr) in addresses {
                self.legacy_insert_unique_or_error(
                    &mut result,
                    name.clone(),
                    NamedAddress::Defined(OriginalID(addr)),
                )?;
            }
        }

        Ok(result)
    }

    // Tries to add an address in the result, and fails if it already exists and dos not match
    // the existing address.
    fn legacy_insert_unique_or_error(
        &self,
        result: &mut BTreeMap<PackageName, NamedAddress>,
        name: PackageName,
        addr: NamedAddress,
    ) -> PackageResult<()> {
        let existing = result.insert(name.clone(), addr.clone());
        if existing.is_some_and(|existing| existing != addr) {
            return Err(PackageError::DuplicateNamedAddress {
                address: name,
                package: self.package().display_name().to_string(),
            });
        }
        Ok(())
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

    /// Return the named address for this package
    pub fn named_address(&self) -> NamedAddress {
        self.node_to_addr(self.node)
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
        graph::{NamedAddress, PackageGraph, PackageInfo},
        schema::{OriginalID, PackageName, PublishedID},
        test_utils::graph_builder::TestPackageGraph,
    };

    /// Return the packages in the graph, grouped by their name
    fn packages_by_name(
        graph: &PackageGraph<Vanilla>,
    ) -> BTreeMap<PackageName, PackageInfo<'_, Vanilla>> {
        graph
            .packages()
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

    #[test(tokio::test)]
    /// We are testing that if we have `_` in a legacy package's addresses.
    async fn check_legacy_underscore_addresses_cases() {
        let node_names: Vec<&str> = vec![];
        let scenario = TestPackageGraph::new(node_names)
            .add_package("a", |pkg| {
                pkg.set_legacy()
                    .set_legacy_addresses([("a", None::<String>)])
            })
            .add_package("b", |pkg| {
                pkg.set_legacy()
                    .set_legacy_addresses([("b", Some("0x0")), ("foo", None)])
            })
            .add_package("c", |pkg| {
                pkg.set_legacy()
                    .set_legacy_addresses([("c", None::<String>)])
                    .publish(OriginalID::from(0x1), PublishedID::from(0x2), Some(1))
                    .add_file("sources/c.move", "module c::c;")
            })
            .add_package("d1", |pkg| pkg.set_legacy())
            .add_package("d2", |pkg| {
                pkg.set_legacy()
                    .set_legacy_addresses([("d2", None::<String>)])
                    .add_file("sources/d2.move", "module d2::d2;")
            })
            .add_package("e1", |pkg| pkg.set_legacy())
            .add_package("e2", |pkg| {
                pkg.set_legacy()
                    .set_legacy_addresses([("e2", None::<String>)])
                    .publish(OriginalID::from(0x3), PublishedID::from(0x4), Some(1))
                    .add_file("sources/e2.move", "module e2::e2;")
            })
            .add_deps([("d1", "d2"), ("e1", "e2")])
            .build();

        // Scenario 1: We can load the package just fine, treating `_` as the root.
        let a_graph = scenario.graph_for("a").await;
        let addresses = a_graph.root_package_info().named_addresses().unwrap();
        assert_eq!(addresses.len(), 1);
        assert_eq!(
            addresses.get("a").unwrap(),
            &NamedAddress::RootPackage(None)
        );

        // Scenario 2: We cannot load the package because it defines addresses with `_` in them and this is not supported.
        let b_err = scenario.try_graph_for("b").await.unwrap_err();
        let b_err_string = b_err
            .to_string()
            .replace(scenario.path_for("b").to_string_lossy().as_ref(), "<DIR>");

        assert_snapshot!(b_err_string, @r#"Error while loading dependency <DIR>: error while loading legacy manifest "<DIR>/Move.toml": Found non instantiated named address `foo` (declared as `_`). All addresses in the `addresses` field must be instantiated."#);

        // Scenario 3: We can load the package just fine, and it's a root package with a defined addr.
        let c_graph = scenario.graph_for("c").await;
        let addresses = c_graph.root_package_info().named_addresses().unwrap();
        assert_eq!(addresses.len(), 1);
        assert_eq!(
            addresses.get("c").unwrap(),
            &NamedAddress::RootPackage(Some(OriginalID::from(0x1)))
        );

        // Scenario 4: Package d1 depends on package d2 where d2 should be considered "unpublished"
        let d1_graph = scenario.graph_for("d1").await;

        // d1 sees both itself (as root unpublished), and d2 as unpublished.
        let d1_addresses = d1_graph.root_package_info().named_addresses().unwrap();
        assert_eq!(d1_addresses.len(), 2);
        assert!(matches!(
            d1_addresses.get("d2").unwrap(),
            NamedAddress::Unpublished { dummy_addr: _ }
        ));
        assert!(matches!(
            d1_addresses.get("d1").unwrap(),
            NamedAddress::RootPackage(None)
        ));

        let d2_addresses = d1_graph
            .packages()
            .into_iter()
            .find(|package| package.name().as_str() == "d2")
            .unwrap();

        let d2_addresses = d2_addresses.named_addresses().unwrap();
        assert_eq!(d2_addresses.len(), 1);
        assert!(matches!(
            d2_addresses.get("d2").unwrap(),
            NamedAddress::Unpublished { dummy_addr: _ }
        ));

        // Scenario 5: Package e1 depends on package e2 where e2 should be considered published.
        let e1_graph = scenario.graph_for("e1").await;
        let e1 = e1_graph.root_package_info().named_addresses().unwrap();

        let e2_publish_named_addr = NamedAddress::Defined(OriginalID::from(0x3));
        assert_eq!(e1.len(), 2);
        assert_eq!(e1.get("e2").unwrap(), &e2_publish_named_addr);
        assert!(matches!(
            e1.get("e1").unwrap(),
            NamedAddress::RootPackage(None)
        ));

        let e2_addresses = e1_graph
            .packages()
            .into_iter()
            .find(|package| package.name().as_str() == "e2")
            .unwrap();

        let e2_addresses = e2_addresses.named_addresses().unwrap();
        assert_eq!(e2_addresses.len(), 1);
        assert_eq!(e2_addresses.get("e2").unwrap(), &e2_publish_named_addr);
    }

    #[test(tokio::test)]
    async fn test_address_can_appear_many_times() {
        let node_names: Vec<&str> = vec![];
        let scenario = TestPackageGraph::new(node_names)
            .add_package("matches_foo", |pkg| {
                pkg.set_legacy().set_legacy_addresses([
                    ("matches_foo", Some("0x0")),
                    ("foo", Some("0x1")),
                    ("bar", Some("0x2")),
                ])
            })
            .add_package("does_not_match_foo", |pkg| {
                pkg.set_legacy().set_legacy_addresses([
                    ("does_not_match_foo", Some("0x0")),
                    ("foo", Some("0x2")),
                ])
            })
            .add_package("foo", |pkg| {
                pkg.set_legacy()
                    .set_legacy_addresses([("foo", Some("0x1")), ("bar", Some("0x2"))])
                    .add_file("sources/foo.move", "module foo::foo;")
            })
            .add_deps([("matches_foo", "foo"), ("does_not_match_foo", "foo")])
            .build();

        let graph = scenario.graph_for("matches_foo").await;
        let addresses = graph.root_package_info().named_addresses().unwrap();
        assert_eq!(addresses.len(), 3);
        assert_eq!(
            addresses.get("foo").unwrap(),
            &NamedAddress::Defined(OriginalID::from(0x1))
        );
        assert_eq!(
            addresses.get("bar").unwrap(),
            &NamedAddress::Defined(OriginalID::from(0x2))
        );

        let graph = scenario.graph_for("does_not_match_foo").await;
        let addresses = graph.root_package_info().named_addresses().unwrap_err();

        assert_snapshot!(addresses, @"Address `foo` is defined more than once in package `DoesNotMatchFoo` (or its dependencies)");
    }
}
