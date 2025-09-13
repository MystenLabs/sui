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
use tracing::debug;

use crate::{flavor::MoveFlavor, schema::OriginalID};

use super::{PackageGraph, PackageInfo};

#[derive(Debug, Error)]
pub enum LinkageError {
    #[error("{0}")]
    InconsistentLinkage(String),

    // Note: see [super::rename_from] for how I think this error message should be constructed
    #[error("
        Package <TODO: root> depends on different source packages for version <TODO> of <TODO> (published at <TODO: abbrev published-at>):

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

    #[error("
        Package <TODO> has override dependencies on both <TODO> and <TODO>, but these are different versions of the same package.
        ")]
    ConflictingOverrides,

    #[error(
        "Package <TODO: p1> depends on itself (<TODO: p1> -> <TODO: p2> -> <TODO: p3> -> <TODO: p1>)"
    )]
    CyclicDependencies(Cycle<NodeIndex>),

    #[error(
        "Package <TODO: p1> depends on a different version of itself (<TODO: p1> → <TODO: p1> → ...); both <TODO: p1> and <TODO: pn> have the original id <TODO>"
    )]
    DependsOnSelf,
}

pub type LinkageResult<T> = Result<T, LinkageError>;

/// Mapping from original ID to the package info to use for that address
pub type LinkageTable<'a, F> = BTreeMap<OriginalID, PackageInfo<'a, F>>;

impl<F: MoveFlavor> PackageGraph<F> {
    /// Construct and return a linkage table for the root package of `self`. Only published
    /// packages are included in the linkage table.
    ///
    /// The linkage table for a given package indicates which package nodes it should use for its
    /// transitive dependencies. The linkage must be consistent - if any node depends
    /// (transitively) multiple versions of the same package (as determined by their original IDs),
    /// then that node must specify `override = true` in its manifest.
    ///
    /// This method checks that the entire graph has consistent linkage, but only returns the
    /// linkage for the root node.
    pub fn linkage(&self) -> LinkageResult<LinkageTable<F>> {
        // we compute the linkage in reverse topological order, so that the linkage for a package's
        // dependencies have been computed before we compute its linkage
        let sorted = toposort(&self.inner, None).map_err(LinkageError::CyclicDependencies)?;

        let mut linkages: BTreeMap<NodeIndex, BTreeMap<OriginalID, NodeIndex>> = BTreeMap::new();
        for node in sorted.iter().rev() {
            let package_node = &self.inner[*node];

            // note: since we are iterating in reverse topological order, the linkages for the
            // dependencies have already been computed
            let transitive_deps: HashMap<&OriginalID, Vec<&NodeIndex>> = self
                .inner
                .neighbors(*node)
                .flat_map(|n| linkages[&n].iter())
                .into_group_map();

            // compute the linkage for `node` by iterating all transitive deps and looking for
            // duplicates
            let mut linkage: BTreeMap<OriginalID, NodeIndex> = BTreeMap::new();
            let overrides = self.override_nodes(*node)?;

            // TODO: `select_dep(node, ...)` produces an error if there's a missing override in `node`,
            // which means we will produce an error if any package in the package graph is missing
            // an override. However, that means if a package doesn't have its override set
            // properly, you can't depend on it. Should we relax this check to only produce an error
            // for the root node?
            for (original_id, nodes) in transitive_deps.into_iter() {
                linkage.insert(
                    original_id.clone(),
                    self.select_dep(original_id, nodes, &overrides)?,
                );
            }

            // if this node is published, add it to its linkage
            if let Some(oid) = package_node.original_id() {
                let old_entry = linkage.insert(oid.clone(), *node);
                if old_entry.is_some() {
                    // this means a package depends on another package that has the same original
                    // id (but it's a different package since we already checked for cycles)
                    return Err(LinkageError::DependsOnSelf);
                }
            }

            linkages.insert(*node, linkage);
        }

        let root = sorted[0];
        let root_linkage = linkages
            .remove(&root) // root package is first in topological order
            .expect("all linkages have been computed");

        debug!("computed linkage: {root_linkage:?}");
        // Convert NodeIndex to PackageInfo
        Ok(root_linkage
            .into_iter()
            .map(|(oid, node)| (oid, self.package_info(node)))
            .collect())
    }

    /// Returns the packages that are overridden in `node`, keyed by their original IDs (only
    /// published packages are returned).
    fn override_nodes(&self, node_id: NodeIndex) -> LinkageResult<BTreeMap<OriginalID, NodeIndex>> {
        let mut result: BTreeMap<OriginalID, NodeIndex> = BTreeMap::new();

        for edge in self.inner.edges(node_id) {
            let dep = &edge.weight().dep;

            if !dep.is_override() {
                continue;
            }

            let target = &self.inner[edge.target()];
            let Some(oid) = target.original_id() else {
                continue;
            };

            let old = result.insert(oid.clone(), edge.target());
            if old.is_some() {
                return Err(LinkageError::ConflictingOverrides);
            }
        }

        Ok(result)
    }

    /// Given a (nonempty) set of transitive dependencies all having `original_id`, choose the correct one or
    /// produce an error as follows:
    ///  - If there is an override, return that
    ///  - Otherwise if there is only one such package, return it
    ///  - Otherwise return an error message
    fn select_dep(
        &self,
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
            Err(LinkageError::InconsistentLinkage(
                "TODO: inconsistent linkage".to_string(),
            ))
            /* TODO: construct error message
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

#[cfg(any(doc, test))]
mod tests {
    // Note: to generate diagrams for these tests, you need to first make the `dep-dependencies`
    // into real dependencies (this seems to be a fundamental limitation to rust)
    // ```sh
    // sed -I "" 's/\[dev-dep/# \[dev-dep/g' Cargo.toml
    // cargo rustdoc --lib -- --cfg test --document-private-items
    // sed -I "" 's/# \[dev-dep/\[dev-dep/g' Cargo.toml
    // cargo docs --dir ../../target/doc
    // ```
    //
    // and navigate to http://127.0.0.1:8080/move_package_alt/graph/linkage/tests/index.html
    //

    use crate::{
        schema::{OriginalID, PublishedID},
        test_utils::graph_builder::TestPackageGraph,
    };

    use insta::assert_snapshot;
    use test_log::test;

    // TODO: add error message snapshots for the tests that produce errors

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b --> d
    ///     a --> c --> d
    ///     d --> e
    /// ```
    /// Computing linkage for both `root` and `a` should succeed
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_basic() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c", "d", "e"])
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("a", "c"),
                ("b", "d"),
                ("c", "d"),
                ("d", "e"),
            ])
            .build();

        scenario.graph_for("root").await.linkage().unwrap();
        scenario.graph_for("a").await.linkage().unwrap();
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b --> d1
    ///     a --> c --> d2
    /// ```
    ///
    /// Computing linkage for both `root` and `a` should fail due to inconsistent versions
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_incompatible() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_published("d1", OriginalID::from(1), PublishedID::from(1))
            .add_published("d2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("a", "c"),
                ("b", "d1"),
                ("c", "d2"),
            ])
            .build();

        assert!(scenario.graph_for("root").await.linkage().is_err());
        assert!(scenario.graph_for("a").await.linkage().is_err());
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b --> d1[d1; published-at = 1]
    ///     a --> c --> d2[d2; published-at = 1]
    /// ```
    ///
    /// In the current iteration this should fail, but in the future we may want to enable it. For
    /// example, `d1` may be a source package and `d2` an on-chain package; we should support
    /// having both in the package graph
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_compatible() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_published("d1", OriginalID::from(1), PublishedID::from(1))
            .add_published("d2", OriginalID::from(1), PublishedID::from(1))
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("a", "c"),
                ("b", "d1"),
                ("c", "d2"),
            ])
            .build();

        assert!(scenario.graph_for("a").await.linkage().is_err());
        assert!(scenario.graph_for("root").await.linkage().is_err());
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b --> d1
    ///     a --> c --> d2
    ///     a -->|override| d3
    /// ```
    ///
    /// Computing linkage for both `a` and `root` should succeed
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_override() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_published("d1", OriginalID::from(1), PublishedID::from(1))
            .add_published("d2", OriginalID::from(1), PublishedID::from(2))
            .add_published("d3", OriginalID::from(1), PublishedID::from(3))
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("a", "c"),
                ("b", "d1"),
                ("c", "d2"),
            ])
            .add_dep("a", "d3", |dep| dep.set_override())
            .build();

        scenario.graph_for("root").await.linkage().unwrap();
        scenario.graph_for("a").await.linkage().unwrap();
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b --> d1
    ///     a --> c --> d2
    ///     a --> d3
    /// ```
    ///
    /// Computing linkage for both `a` and `root` should fail because of the inconsistent linkage
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_nooverride() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_published("d1", OriginalID::from(1), PublishedID::from(1))
            .add_published("d2", OriginalID::from(1), PublishedID::from(2))
            .add_published("d3", OriginalID::from(1), PublishedID::from(3))
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("a", "c"),
                ("a", "d3"),
                ("b", "d1"),
                ("c", "d2"),
            ])
            .build();

        assert!(scenario.graph_for("root").await.linkage().is_err());
        assert!(scenario.graph_for("a").await.linkage().is_err());
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b --> d2
    ///     a --> d1
    /// ```
    ///
    /// Computing linkage for both `a` and `root` should fail because of linkage to `d1` and `d2`
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_direct_and_transitive_nooverride() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_published("d1", OriginalID::from(1), PublishedID::from(1))
            .add_published("d2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([("root", "a"), ("a", "b"), ("a", "d1"), ("b", "d2")])
            .build();

        assert!(scenario.graph_for("root").await.linkage().is_err());
        assert!(scenario.graph_for("a").await.linkage().is_err());
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b1
    ///     a --> b2
    /// ```
    /// Computing linkage for both `root` and `a` should fail because of conflicting
    /// implementations of `b`
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_direct_no_override() {
        let scenario = TestPackageGraph::new(["root", "a"])
            .add_published("b1", OriginalID::from(1), PublishedID::from(1))
            .add_published("b2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([("root", "a"), ("a", "b1"), ("a", "b2")])
            .build();

        assert!(scenario.graph_for("root").await.linkage().is_err());
        assert!(scenario.graph_for("a").await.linkage().is_err());
    }

    /// (Same as [test_direct_no_override] except that `b2` dependency is an override)
    ///
    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b1
    ///     a -->|override| b2
    /// ```
    ///
    /// It's unclear what we should do in this case. On the one hand, the user has probably made a
    /// mistake to end up in such a weird situation. On the other hand, the semantics are clear:
    /// for compilation `b1` refers to `b1` and `b2` refers to `b2`, while at runtime `b2` is used
    /// for both. In a sense, `a` is overriding its own dependencies.
    ///
    /// Currently we allow this since it is simpler and doesn't break anything. It should really be
    /// a lint (ha!)
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_direct_one_override() {
        let scenario = TestPackageGraph::new(["root", "a"])
            .add_published("b1", OriginalID::from(1), PublishedID::from(1))
            .add_published("b2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([("root", "a"), ("a", "b1")])
            .add_dep("a", "b2", |dep| dep.set_override())
            .build();

        scenario.graph_for("root").await.linkage().unwrap();
        scenario.graph_for("a").await.linkage().unwrap();
    }

    /// (Same as [test_direct_no_override] except that `b2` dependency is an override)
    ///
    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a -->|override| b1
    ///     a -->|override| b2
    /// ```
    ///
    /// Same as [test_direct_no_override] except both of the deps are overrides. This should
    /// fail because it's not clear which override to take
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_direct_both_override() {
        let scenario = TestPackageGraph::new(["root", "a"])
            .add_published("b1", OriginalID::from(1), PublishedID::from(1))
            .add_published("b2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([("root", "a")])
            .add_dep("a", "b1", |dep| dep.set_override())
            .add_dep("a", "b2", |dep| dep.set_override())
            .build();

        assert!(scenario.graph_for("root").await.linkage().is_err());
        assert!(scenario.graph_for("a").await.linkage().is_err());
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b --> c --> a
    /// ```
    ///
    /// Computing linkage for both `a` and `root` should fail because of cyclic dependency
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_cyclic() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_deps([("root", "a"), ("a", "b"), ("b", "c"), ("c", "a")])
            .build();

        assert!(scenario.graph_for("root").await.linkage().is_err());
        assert!(scenario.graph_for("a").await.linkage().is_err());
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a1 --> b --> a2
    /// ```
    ///
    /// Computing linkgage for both `a1` and `root` should fail because `a1` depends transitively
    /// on a different version of itself
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_dep_on_different_version_of_self() {
        let scenario = TestPackageGraph::new(["root", "b"])
            .add_published("a1", OriginalID::from(1), PublishedID::from(1))
            .add_published("a2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([("root", "a1"), ("a1", "b"), ("b", "a2")])
            .build();

        assert!(scenario.graph_for("root").await.linkage().is_err());
        assert!(scenario.graph_for("a1").await.linkage().is_err());
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a -->|"b1 = { ... }"| b
    ///     a -->|"b2 = { ... }"| b
    /// ```
    ///
    /// Computing linkage for both `root` and `a` should succeed (although this is arguably a
    /// corner case)
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_double_dep() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_deps([("root", "a"), ("a", "b")])
            .add_dep("a", "b", |dep| dep.name("b2"))
            .build();

        scenario.graph_for("root").await.linkage().unwrap();
        scenario.graph_for("a").await.linkage().unwrap();
    }

    /// Same as [test_double_dep] except that the dependencies are overrides
    ///
    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a -->|"b1 = { ..., override = true }"| b
    ///     a -->|"b2 = { ..., override = true }"| b
    /// ```
    ///
    /// Computing linkage for both `root` and `a` should succeed (although this is arguably a
    /// corner case)
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_double_dep_override() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_deps([("root", "a")])
            .add_dep("a", "b", |dep| dep.set_override())
            .add_dep("a", "b", |dep| dep.set_override().name("b2"))
            .build();

        scenario.graph_for("root").await.linkage().unwrap();
        scenario.graph_for("a").await.linkage().unwrap();
    }

    /// This is a situation where an overridden dependency introduces a transitive dependency which
    /// then causes a conflict. This is a regression test.
    ///
    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b --> c1 --> d1
    ///     a -->|override| c2 --> d2
    /// ```
    ///
    /// The conflict between `d1` and `d2` should not require an override in `a` because the
    /// dependency on `d1` only comes through `c1` which is already overriddent to `c2`. In other
    /// words no linked package has a dependency on `d1`
    ///
    /// Therefore linkage for both `a` and `root` should succeed
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    #[ignore] // TODO: current implementation is incorrect
    async fn test_overridden_override() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_published("c1", OriginalID::from(1), PublishedID::from(1))
            .add_published("c2", OriginalID::from(1), PublishedID::from(2))
            .add_published("d1", OriginalID::from(2), PublishedID::from(1))
            .add_published("d2", OriginalID::from(2), PublishedID::from(2))
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("b", "c1"),
                ("c1", "d1"),
                ("c2", "d2"),
            ])
            .add_dep("a", "c2", |dep| dep.set_override())
            .build();

        scenario.graph_for("root").await.linkage().unwrap();
        scenario.graph_for("a").await.linkage().unwrap();
    }

    /// This test shows why we require overrides on _all_ paths from the root to a package if that
    /// package's direct dependencies are changed.
    ///
    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b --> d
    ///     a --> c --> d
    ///     d --> e1
    ///     c -->|override| e2
    /// ```
    ///
    /// In this case, `d` has a direct dependency on `e1` which is overridden to `e2` by `c`.
    /// However, `c`'s override shouldn't matter to `b`: `a` has altered the behavior of `b` by
    /// relinking, but has not indicated that by providing any overrides. Therefore, this example
    /// should be rejected.
    ///
    /// This can be solved in `a` by adding an override dependency to `e2` in its manifest (see
    /// [test_diamond_with_side_override_fixed])
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_diamond_with_side_override() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c", "d"])
            .add_published("e1", OriginalID::from(1), PublishedID::from(1))
            .add_published("e2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("b", "d"),
                ("a", "c"),
                ("c", "d"),
                ("d", "e1"),
            ])
            .add_dep("c", "e2", |dep| dep.set_override())
            .build();

        assert_snapshot!(scenario.graph_for("root").await.linkage().unwrap_err().to_string(), @"TODO: inconsistent linkage");
        assert_snapshot!(scenario.graph_for("a").await.linkage().unwrap_err().to_string(), @"TODO: inconsistent linkage");
    }

    /// This test shows the fix for [test_diamond_with_side_override]
    ///
    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b --> d
    ///     a --> c --> d
    ///     d --> e1
    ///     c -->|override| e2
    ///     a -->|override| e2
    /// ```
    ///
    /// Here the fact that `b` has a transitive dependency on `e1` that is overridden to `e2` is
    /// not a problem because its ancestor (`a`) declared an override to `e2`.
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_diamond_with_side_override_fixed() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c", "d"])
            .add_published("e1", OriginalID::from(1), PublishedID::from(1))
            .add_published("e2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("b", "d"),
                ("a", "c"),
                ("c", "d"),
                ("d", "e1"),
            ])
            .add_dep("c", "e2", |dep| dep.set_override())
            .add_dep("a", "e2", |dep| dep.set_override())
            .build();

        scenario.graph_for("root").await.linkage().unwrap();
        scenario.graph_for("a").await.linkage().unwrap();
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b --> c --> d1
    ///     b -->|override| d2
    ///     a -->|override| d3
    /// ```
    ///
    /// This computed linkage for both `a` and `root` should have `d3` because that's the highest
    /// override in the tree
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_nested_overrides() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_published("d1", OriginalID::from(1), PublishedID::from(1))
            .add_published("d2", OriginalID::from(1), PublishedID::from(2))
            .add_published("d3", OriginalID::from(1), PublishedID::from(3))
            .add_deps([("root", "a"), ("a", "b"), ("b", "c"), ("c", "d1")])
            .add_dep("b", "d2", |dep| dep.set_override())
            .add_dep("a", "d3", |dep| dep.set_override())
            .build();

        let graph = scenario.graph_for("root").await;
        let linkage = graph.linkage().unwrap();
        assert_eq!(
            linkage
                .get(&OriginalID::from(1))
                .unwrap()
                .package()
                .published_at(),
            Some(&PublishedID::from(3))
        );
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b --> c --> d1
    ///     a -->|override| d2
    ///     b -->|override| d3
    /// ```
    ///
    /// Linkage should fail because a should override to `d2` but that would force `b` to downgrade
    /// from `d3` to `d2`
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    #[ignore] // TODO: fix this bug
    async fn test_nested_overrides_bad_version() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_published("d1", OriginalID::from(1), PublishedID::from(1))
            .add_published("d2", OriginalID::from(1), PublishedID::from(2))
            .add_published("d3", OriginalID::from(1), PublishedID::from(3))
            .add_deps([("root", "a"), ("a", "b"), ("b", "c"), ("c", "d1")])
            .add_dep("b", "d3", |dep| dep.set_override())
            .add_dep("a", "d2", |dep| dep.set_override())
            .build();

        assert_snapshot!(scenario.graph_for("root").await.linkage().unwrap_err().to_string(), @"");
        assert_snapshot!(scenario.graph_for("a").await.linkage().unwrap_err().to_string(), @"");
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a
    ///     a --> b --> c1 --> d1
    ///     a -->|override| c2
    ///     a -.-> d1
    /// ```
    ///
    /// In this example, `a` overrides `b`'s `c1` dependency to `c2` which doesn't include a
    /// dependency on `d` at all. In a legacy package, can `a` refer to `d` at all? If so, which
    /// version of `d` should it use?
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    #[ignore] // TODO: what should happen here?
    async fn test_dropped_dep() {
        todo!()
    }
}
