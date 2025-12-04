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

impl NamedAddress {
    pub fn is_defined(&self) -> bool {
        matches!(self, NamedAddress::Defined(_))
    }

    pub fn is_unpublished(&self) -> bool {
        matches!(self, NamedAddress::Unpublished { .. })
    }

    pub fn is_undefined_root_package(&self) -> bool {
        matches!(self, NamedAddress::RootPackage(None))
    }
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
            // Build up the context from the root, so we can pass addresses down.
            // This is empty if the package is the root package.
            let root_assignments = if let Some(incoming) = self
                .graph
                .inner
                .edges_directed(self.node, Direction::Incoming)
                .next()
            {
                let parent = PackageInfo {
                    graph: self.graph,
                    node: incoming.source(),
                };
                parent.named_addresses()?
            } else {
                BTreeMap::new()
            };

            return self.legacy_named_addresses(&root_assignments);
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
    fn legacy_named_addresses(
        &self,
        root_assignments: &BTreeMap<PackageName, NamedAddress>,
    ) -> PackageResult<BTreeMap<PackageName, NamedAddress>> {
        let mut result: BTreeMap<PackageName, NamedAddress> = BTreeMap::new();

        // Step 1: Handle the "name" of the package.
        // For legacy cases that the name was `_`, there are these options:
        // 1. The package has an original id defined for the env, in which case thats the correct address
        // 2. The parent has defined an ID for the address, so we can use that
        // 3. If none of the above, we fail.
        if self.package().name().as_str() != NO_NAME_LEGACY_PACKAGE_NAME {
            // If the package has an `_` as its address, we need special handling
            if let Some(legacy_data) = &self.package().legacy_data
                && legacy_data.modern_name_address_is_underscore
            {
                // if we have an original id, we do the typical insertion
                if self.package().original_id().is_some() {
                    result.insert(self.package().name().clone(), self.node_to_addr(self.node));
                // If the address is defined by a parent, we use that definition.
                } else if let Some(parent_addr) = root_assignments.get(self.package().name()) {
                    result.insert(self.package().name().clone(), parent_addr.clone());
                // Last resort is failing.
                } else {
                    return Err(PackageError::NameNotDefined {
                        name: self.package().name().clone(),
                        package: self.package().display_name().to_string(),
                    });
                }
            } else {
                result.insert(self.package().name().clone(), self.node_to_addr(self.node));
            }
        }

        // Step 2: Process the legacy addresses.
        if let Some(legacy_data) = &self.package().legacy_data {
            let addresses = legacy_data.named_addresses.clone();

            // Handle other addresses
            for (name, addr) in addresses {
                if let Some(addr) = addr {
                    // Address is defined in this package
                    let new_addr = NamedAddress::Defined(OriginalID(addr));
                    let existing = result.insert(name.clone(), new_addr.clone());

                    if existing.is_some_and(|existing| {
                        existing.is_defined() && new_addr.is_defined() && existing != new_addr
                    }) {
                        return Err(PackageError::DuplicateNamedAddress {
                            address: name,
                            package: self.package().display_name().to_string(),
                        });
                    }
                } else {
                    // Address is None, must be filled in from the parent, or we need to error.
                    if let Some(parent_addr) = root_assignments.get(&name) {
                        result.insert(name.clone(), parent_addr.clone());
                    } else {
                        return Err(PackageError::NameNotDefined {
                            name,
                            package: self.package().display_name().to_string(),
                        });
                    }
                }
            }
        }

        // Step 2: Build context for children (parent assignments + this package's resolved addresses)
        let mut child_assignments = root_assignments.clone();
        for (name, addr) in result.iter() {
            if let Some(existing) = child_assignments.get(name) {
                // Check for conflicts when merging into child context
                if existing.is_defined() && addr.is_defined() && existing != addr {
                    return Err(PackageError::DuplicateNamedAddress {
                        address: name.clone(),
                        package: self.package().display_name().to_string(),
                    });
                }
            }
            child_assignments.insert(name.clone(), addr.clone());
        }

        // Step 3: Recursively process dependencies
        for (_, dep) in self.direct_deps() {
            // eprintln!("child_assignments for package {:?}: {:?}", self.package().name(), child_assignments);
            let transitive_result = dep.legacy_named_addresses(&child_assignments)?;

            // Step 4: Merge child's results into our result
            for (name, addr) in transitive_result {
                if let Some(existing) = result.get(&name) {
                    // Check for conflicts during merge
                    if existing.is_defined() && addr.is_defined() && existing != &addr {
                        return Err(PackageError::DuplicateNamedAddress {
                            address: name,
                            package: self.package().display_name().to_string(),
                        });
                    }
                }
                result.insert(name, addr);
            }
        }

        // eprintln!("result for package {:?}: {:?}", self.package().name(), result);

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
    use move_core_types::account_address::AccountAddress;
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
    async fn test_root_legacy_package_underscore_named_addresses() {
        let node_names: Vec<&str> = vec![];
        let scenario = TestPackageGraph::new(node_names)
            .add_package("a", |pkg| pkg.set_legacy().add_address("a", None::<String>))
            .build();

        let graph = scenario.graph_for("a").await;
        let addresses = graph.root_package_info().named_addresses();

        assert!(addresses.is_err());

        let err = addresses.unwrap_err();
        assert_snapshot!(err, @"Named address `a` on package `A` has an underscore `(_)` assignment, but no address was found for it.");
    }

    #[test(tokio::test)]
    /// In this test, root defines a as 0x1, while b defines a as 0x456, giving us an invalid definition error.
    async fn test_underscore_address_defined_with_different_addresses() {
        let node_names: Vec<&str> = vec![];
        let scenario = TestPackageGraph::new(node_names)
            .add_package("root", |pkg| {
                pkg.set_legacy()
                    .add_address("root", Some("0x0"))
                    .add_address("a", Some("0x1"))
            })
            .add_package("b", |pkg| {
                pkg.set_legacy()
                    .add_address("b", Some("0x123"))
                    .add_address("a", Some("0x456"))
            })
            .add_package("a", |pkg| pkg.set_legacy().add_address("a", None::<String>))
            .add_deps([("root", "b"), ("b", "a")])
            .build();

        let graph = scenario.graph_for("root").await;
        let err = graph.root_package_info().named_addresses().unwrap_err();

        assert_snapshot!(err, @"Address `a` is defined more than once in package `B` (or its dependencies)");
    }

    #[test(tokio::test)]
    /// When we're (usually) on a root package, we are allowed to have `_` addresses,
    /// as long as there's an original ID set by the publication info of the environment we're building on.
    async fn test_underscore_can_be_defined_by_publication_info() {
        let node_names: Vec<&str> = vec![];
        let scenario = TestPackageGraph::new(node_names)
            .add_package("root", |pkg| {
                pkg.set_legacy()
                    .add_address("root", None::<String>)
                    // Add a file to make sure `root` is the derived modern name for the pkg.
                    .add_file("sources/root.move", "module root::root;")
                    .publish(
                        OriginalID(AccountAddress::from_hex_literal("0x123").unwrap()),
                        PublishedID(AccountAddress::from_hex_literal("0x1").unwrap()),
                        Some(1),
                    )
            })
            .build();

        let graph = scenario.graph_for("root").await;
        let addresses = graph.root_package_info().named_addresses().unwrap();

        assert_eq!(addresses.len(), 1);

        assert_eq!(
            addresses.get("root").unwrap(),
            &NamedAddress::RootPackage(Some(OriginalID(
                AccountAddress::from_hex_literal("0x123").unwrap()
            )))
        );
    }

    #[test(tokio::test)]
    /// Tests that parent can define an underscore address for a child, and also verify that
    /// extra legacy addresses remain visible to the parent.
    async fn test_underscore_address_is_defined_by_parent() {
        let node_names: Vec<&str> = vec![];
        let scenario = TestPackageGraph::new(node_names)
            .add_package("b", |pkg| {
                pkg.set_legacy()
                    .add_address("b", Some("0x123"))
                    .add_address("a", Some("0x456"))
            })
            .add_package("a", |pkg| {
                pkg.set_legacy()
                    .add_address("a", None::<String>)
                    // add a random address to make sure it's visible to the dependend
                    .add_address("random", Some("0x789"))
                    // Add a file to make sure `a` is the derived modern name for the pkg.
                    .add_file("sources/a.move", "module a::a;")
            })
            .add_deps([("b", "a")])
            .build();

        let graph = scenario.graph_for("b").await;

        let a_address_table = graph.root_package_info().named_addresses().unwrap();

        assert_eq!(a_address_table.len(), 3);
        assert_eq!(
            a_address_table.get("a").unwrap(),
            &NamedAddress::Defined(OriginalID(
                AccountAddress::from_hex_literal("0x456").unwrap()
            ))
        );
        assert_eq!(
            a_address_table.get("b").unwrap(),
            &NamedAddress::Defined(OriginalID(
                AccountAddress::from_hex_literal("0x123").unwrap()
            ))
        );
        assert_eq!(
            a_address_table.get("random").unwrap(),
            &NamedAddress::Defined(OriginalID(
                AccountAddress::from_hex_literal("0x789").unwrap()
            ))
        );

        // Get the address table for `a`. It should work, but ONLY have access to `a = 0x456` and `random = 0x789`
        let b_address_table = graph
            .packages()
            .iter()
            .find(|pkg| pkg.name().as_str() == "a")
            .unwrap()
            .named_addresses();

        let b_address_table = b_address_table.unwrap();

        assert_eq!(b_address_table.len(), 2);

        assert_eq!(
            b_address_table.get("a").unwrap(),
            &NamedAddress::Defined(OriginalID(
                AccountAddress::from_hex_literal("0x456").unwrap()
            ))
        );
        assert_eq!(
            b_address_table.get("random").unwrap(),
            &NamedAddress::Defined(OriginalID(
                AccountAddress::from_hex_literal("0x789").unwrap()
            ))
        );
    }

    #[test(tokio::test)]
    // Success scenario: Address `a` is defined twice in the tree, but they match.
    // We also verify that:
    // 1. Root package is properly detected and classified
    // 2. Dependencies can still only see what is visible ot them
    async fn test_underscore_defined_twice() {
        let node_names: Vec<&str> = vec![];
        let success_scenario = TestPackageGraph::new(node_names)
            .add_package("root", |pkg| {
                pkg.set_legacy()
                    .add_address("root", Some("0x0"))
                    .add_address("a", Some("0x1"))
            })
            .add_package("b", |pkg| {
                pkg.set_legacy()
                    .add_address("b", Some("0x0"))
                    .add_address("a", Some("0x1"))
            })
            .add_package("a", |pkg| pkg.set_legacy().add_address("a", None::<String>))
            .add_deps([("root", "b"), ("b", "a")])
            .build();

        let graph = success_scenario.graph_for("root").await;
        let root_named_addresses = graph.root_package_info().named_addresses().unwrap();
        assert_eq!(root_named_addresses.len(), 3);
        assert!(
            root_named_addresses
                .get("root")
                .unwrap()
                .is_undefined_root_package()
        );
        assert_eq!(
            root_named_addresses.get("a").unwrap(),
            &NamedAddress::Defined(OriginalID(AccountAddress::from_hex_literal("0x1").unwrap()))
        );
        assert!(root_named_addresses.get("b").unwrap().is_unpublished());
    }

    #[test(tokio::test)]
    /// Testing that an underscore address can only be defined by a parent, not a child in the graph.
    async fn test_underscore_address_can_only_be_defined_by_parent() {
        let node_names: Vec<&str> = vec![];
        let scenario = TestPackageGraph::new(node_names.clone())
            .add_package("root", |pkg| {
                pkg.set_legacy()
                    .add_address("root", Some("0x0"))
                    .add_address("random", Some("0x789"))
            })
            .add_package("b", |pkg| {
                pkg.set_legacy()
                    .add_address("b", Some("0x123"))
                    .add_address("random", None::<String>)
            })
            .add_package("a", |pkg| {
                pkg.set_legacy()
                    .add_address("a", Some("0x0"))
                    .add_address("random", Some("0x789"))
            })
            .add_deps([("root", "b"), ("b", "a")])
            .build();

        // We start from `b`, and in this case, we do not have any parent defining `random`, only a child.
        // So this fails.
        let graph = scenario.graph_for("b").await;
        let err = graph.root_package_info().named_addresses().unwrap_err();
        assert_snapshot!(err, @"Named address `random` on package `B` has an underscore `(_)` assignment, but no address was found for it.");

        // But, if we start from `root`, it defines `random` and it also matches the definition that `a` gives (as a lower level dep), so it's ok!
        let graph = scenario.graph_for("root").await;
        let addresses = graph.root_package_info().named_addresses();

        assert!(addresses.is_ok());

        // Now, let's also verify that SIBLINGS should not work either
        let scenario = TestPackageGraph::new(node_names)
            .add_package("root", |pkg| {
                pkg.set_legacy().add_address("root", Some("0x0"))
            })
            .add_package("b", |pkg| {
                pkg.set_legacy()
                    .add_address("b", Some("0x123"))
                    .add_address("random", None::<String>)
            })
            .add_package("a", |pkg| {
                pkg.set_legacy()
                    .add_address("a", Some("0x0"))
                    .add_address("random", Some("0x789"))
            })
            .add_deps([("root", "b"), ("root", "a")])
            .build();

        let graph = scenario.graph_for("root").await;
        let err = graph.root_package_info().named_addresses().unwrap_err();
        assert_snapshot!(err, @"Named address `random` on package `B` has an underscore `(_)` assignment, but no address was found for it.");
    }

    #[test(tokio::test)]
    async fn test_really_deep_underscore_assignment() {
        let scenario = TestPackageGraph::new(vec!["root", "a"])
            .add_package("b", |pkg| pkg.set_legacy())
            .add_package("c", |pkg| {
                pkg.set_legacy()
                    .add_address("c", Some("0x0"))
                    .add_address("random", Some("0x5"))
            })
            .add_package("d", |pkg| pkg.set_legacy())
            .add_package("e", |pkg| pkg.set_legacy())
            .add_package("f", |pkg| {
                pkg.set_legacy()
                    .add_address("f", Some("0x0"))
                    .add_address("random", None::<String>)
            })
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("b", "c"),
                ("c", "d"),
                ("d", "e"),
                ("e", "f"),
            ])
            .build();

        let graph = scenario.graph_for("root").await;

        let f_address_table = graph
            .packages()
            .iter()
            .find(|pkg| pkg.name().as_str() == "f")
            .unwrap()
            .named_addresses()
            .unwrap();

        assert_eq!(f_address_table.len(), 2);
        assert!(f_address_table.get("f").unwrap().is_unpublished());
        assert_eq!(
            f_address_table.get("random").unwrap(),
            &NamedAddress::Defined(OriginalID(AccountAddress::from_hex_literal("0x5").unwrap()))
        );
    }
}
