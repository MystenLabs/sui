// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashMap};

use itertools::Itertools;
use petgraph::{
    algo::{Cycle, toposort},
    graph::NodeIndex,
    visit::EdgeRef,
};
use thiserror::Error;

use crate::{flavor::MoveFlavor, package::PackageName, schema::OriginalID};

use super::PackageGraph;

#[derive(Debug, Error)]
pub enum LinkageError {
    #[error("{0}")]
    InconsistentLinkage(String),

    #[error("
        Package <TODO: root> has depends on different source packages for version <TODO> of <TODO> (published at <TODO: abbrev published-at>):

          <TODO: p1> -> <TODO: p1'> -> <TODO: p2'> is <TODO: dep 1 as manifest dep>
          <TODO: p2> -> <TODO: p2'> is <TODO: dep 2 as manifest dep>

        To resolve this, you must explicitly add an override in <TODO: root>'s Move.toml:

           <TODO: conflict> = {{ <TODO: manifest dep for dep 1>, override = true }}

           or

           <TODO: conflict> = {{ <TODO: manifest dep for dep 2>, override = true }}
        "
    )]
    MultipleImplementations {
        root: NodeIndex,
        node1: NodeIndex,
        node2: NodeIndex,
    },

    #[error(
        "Package <TODO: p1> depends on itself (<TODO: p1> -> <TODO: p2> -> <TODO: p3> -> <TODO: p1>)"
    )]
    CyclicDependencies(Cycle<NodeIndex>),
}

pub type LinkageResult<T> = Result<T, LinkageError>;

/// Mapping from original ID to the package node to use for that address
type LinkageTable = BTreeMap<OriginalID, NodeIndex>;

impl<F: MoveFlavor> PackageGraph<F> {
    /// Construct and return a linkage table for the root package of `self`
    pub fn linkage(&self) -> LinkageResult<LinkageTable> {
        let sorted = toposort(&self.inner, None).map_err(LinkageError::CyclicDependencies)?;
        let root = sorted[0];

        let mut linkages: BTreeMap<NodeIndex, LinkageTable> = BTreeMap::new();
        for node in sorted.iter().rev() {
            // note: since we are iterating in reverse topological order, the linkages for the
            // dependencies have already been computed
            let transitive_deps: HashMap<&OriginalID, Vec<&NodeIndex>> = self
                .inner
                .neighbors(*node)
                .flat_map(|n| linkages[&n].iter())
                .into_group_map();

            // compute the linkage for `node` by iterating all transitive deps and looking for
            // duplicates
            let mut linkage = LinkageTable::new();
            let overrides = self.overrides(*node);
            for (original_id, nodes) in transitive_deps.into_iter() {
                linkage.insert(
                    original_id.clone(),
                    self.select_dep(node, original_id, nodes, &overrides)?,
                );
            }

            // TODO: add self to linkage

            linkages.insert(*node, linkage);
        }

        Ok(linkages
            .remove(&sorted[0]) // root package is first in topological order
            .expect("all linkages have been computed"))
    }

    /// Returns the original IDs of the packages that are overridden in `node`
    fn overrides(&self, node: NodeIndex) -> BTreeMap<OriginalID, NodeIndex> {
        let env = &self.inner[node].use_env;

        let overrides: BTreeSet<PackageName> = self.inner[node]
            .package
            .direct_deps(env)
            .unwrap()
            .into_iter()
            .filter_map(|(name, dep)| if dep.is_override() { Some(name) } else { None })
            .collect();

        self.inner
            .edges(node)
            .filter(|edge| overrides.contains(edge.weight()))
            .map(|edge| {
                (
                    self.inner[edge.target()]
                        .package
                        .original_id(env)
                        .expect("TODO"),
                    edge.target(),
                )
            })
            .collect()
    }

    /// Given a (nonempty) set of transitive dependencies all having `original_id`, choose the correct one (or
    /// produce an error).
    fn select_dep(
        &self,
        root: &NodeIndex,
        original_id: &OriginalID,
        nodes: Vec<&NodeIndex>,
        overrides: &BTreeMap<OriginalID, NodeIndex>,
    ) -> LinkageResult<NodeIndex> {
        if let Some(result) = overrides.get(original_id) {
            return Ok(*result);
        }

        let deduped: BTreeSet<_> = nodes.into_iter().collect();

        if deduped.len() <= 1 {
            Ok(**deduped.first().expect("nodes is nonempty"))
        } else {
            // TODO: construct error message
            todo!()
            /*
                let paths = deduped.map(|target| todo!());
                "Package <TODO: root> depends on <TODO: p1> and <TODO: p2>, but these depend on different versions of <TODO: conflict>:

               <TODO: p1> -> <TODO: p1'> -> <TODO: p1''> refers version <TODO: v1> (published at <TODO: abbrev. addr1>)
               <TODO: p2> -> <TODO: p2'> -> <TODO: p2''> -> <TODO: p2'''> refers to version <TODO: v2> (published at <TODO: abbrev. addr2>)

            To resolve this, you must explicitly add an override in <TODO: root>'s Move.toml:

               <TODO: conflict> = {{ <TODO: manifest dep for later version of conflict>, override = true }}
            "
            */
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{flavor::Vanilla, graph::PackageGraph};

    struct TestPackageGraph;
    struct NodeBuilder;
    struct EdgeBuilder;
    struct Scenario;

    impl TestPackageGraph {
        fn new(nodes: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
            todo!()
        }

        fn add_deps(
            self,
            edges: impl IntoIterator<Item = (impl AsRef<str>, impl AsRef<str>)>,
        ) -> Self {
            todo!();
            self
        }

        fn add_node(
            self,
            node: impl AsRef<str>,
            build: impl FnOnce(NodeBuilder) -> NodeBuilder,
        ) -> Self {
            todo!();
            self
        }

        fn add_dep(
            self,
            source: impl AsRef<str>,
            target: impl AsRef<str>,
            build: impl FnOnce(EdgeBuilder) -> EdgeBuilder,
        ) -> Self {
            todo!();
            self
        }

        fn build(self) -> Scenario {
            todo!()
        }
    }

    impl NodeBuilder {
        fn original_id(self, addr: u64) -> Self {
            todo!();
            self
        }

        fn published_at(self, addr: u64) -> Self {
            todo!();
            self
        }
    }

    impl EdgeBuilder {
        fn set_override(self) -> Self {
            todo!();
            self
        }
    }

    impl Scenario {
        fn graph_for(&self, root: impl AsRef<str>) -> PackageGraph<Vanilla> {
            todo!()
        }
    }

    /// `root` depends on `a` depends on `b` and `c`, both of which depend on `d`
    /// Computing linkage for both `root` and `a` should succeed
    #[test]
    fn test_diamond() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c", "d"])
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("a", "c"),
                ("b", "d"),
                ("c", "d"),
            ])
            .build();

        scenario.graph_for("root").linkage().unwrap();
        scenario.graph_for("a").linkage().unwrap();
    }

    /// `root` depends on `a` which depends on `b` and `c`, which depend on `d1` and `d2` respectively
    /// Computing linkage for both `root` and `a` should fail due to inconsistent versions
    #[test]
    fn test_incompatible() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_node("d1", |d1| d1.original_id(1).published_at(1))
            .add_node("d2", |d2| d2.original_id(1).published_at(2))
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("a", "c"),
                ("b", "d1"),
                ("c", "d2"),
            ])
            .build();

        assert!(scenario.graph_for("root").linkage().is_err());
        assert!(scenario.graph_for("a").linkage().is_err());
    }

    /// `root` depends on `a` which depends on `b` and `c`, which depend on `d1` and `d2`
    /// respectively, BUT `d1` and `d2` have the same published-at address.
    ///
    /// In the current iteration this should fail, but in the future we may want to enable it
    #[test]
    fn test_compatible() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_node("d1", |d1| d1.original_id(1).published_at(1))
            .add_node("d2", |d2| d2.original_id(1).published_at(1))
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("a", "c"),
                ("b", "d1"),
                ("c", "d2"),
            ])
            .build();

        assert!(scenario.graph_for("root").linkage().is_err());
        assert!(scenario.graph_for("a").linkage().is_err());
    }

    /// `root` depends on `a` depends on `b` and `c` which depend on `d1`  and `d2`, but `a` has an override
    /// dependency on `d3`.
    /// Computing linkage for both `a` and `root` should succeed
    #[test]
    fn test_override() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_node("d1", |d1| d1.original_id(1).published_at(1))
            .add_node("d2", |d2| d2.original_id(1).published_at(2))
            .add_node("d3", |d3| d3.original_id(1).published_at(3))
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("a", "c"),
                ("b", "d1"),
                ("c", "d2"),
            ])
            .add_dep("a", "d3", |dep| dep.set_override())
            .build();

        scenario.graph_for("root").linkage().unwrap();
        scenario.graph_for("a").linkage().unwrap();
    }

    /// `root` depends on `a` which depends on `b`, `c`, and `d3` (non-override), while `b` depends on `d2` and `c` depends
    /// on `d3`
    /// Computing linkage for both `a` and `root` should fail because of the inconsistent linkage
    #[test]
    fn test_nooverride() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_node("d1", |d1| d1.original_id(1).published_at(1))
            .add_node("d2", |d2| d2.original_id(1).published_at(2))
            .add_node("d3", |d3| d3.original_id(1).published_at(3))
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("a", "c"),
                ("a", "d3"),
                ("b", "d1"),
                ("c", "d2"),
            ])
            .build();

        assert!(scenario.graph_for("root").linkage().is_err());
        assert!(scenario.graph_for("a").linkage().is_err());
    }

    /// `root` depends on `a` which depends on `b` and `d1`, `b` depends on `d2`
    /// Computing linkage for both `a` and `root` should fail because of linkage to `d1` and `d2`
    #[test]
    fn test_direct_nooverride() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_node("d1", |d1| d1.original_id(1).published_at(1))
            .add_node("d2", |d2| d2.original_id(1).published_at(2))
            .add_deps([("root", "a"), ("a", "b"), ("a", "d1"), ("b", "d2")])
            .build();

        assert!(scenario.graph_for("root").linkage().is_err());
        assert!(scenario.graph_for("a").linkage().is_err());
    }

    /// `root` depends on `a` which depends on `b` which depends on `c` which depends on `a`
    /// Computing linkage for both `a` and `root` should fail because of cyclic dependency
    #[test]
    fn test_cyclic() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_deps([("root", "a"), ("a", "b"), ("b", "c"), ("c", "a")])
            .build();

        assert!(scenario.graph_for("root").linkage().is_err());
        assert!(scenario.graph_for("a").linkage().is_err());
    }
}
