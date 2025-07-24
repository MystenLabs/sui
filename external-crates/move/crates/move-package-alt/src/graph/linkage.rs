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

use crate::{flavor::MoveFlavor, schema::OriginalID};

use super::PackageGraph;

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

/// Mapping from original ID to the package node to use for that address
type LinkageTable = BTreeMap<OriginalID, NodeIndex>;

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
    pub fn linkage(&self) -> LinkageResult<LinkageTable> {
        // we compute the linkage in reverse topological order, so that the linkage for a package's
        // dependencies have been computed before we compute its linkage
        let sorted = toposort(&self.inner, None).map_err(LinkageError::CyclicDependencies)?;

        let mut linkages: BTreeMap<NodeIndex, LinkageTable> = BTreeMap::new();
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
            let mut linkage = LinkageTable::new();
            let overrides = self.override_nodes(*node)?;

            // TODO: `select_dep(node, ...)` produces an error if there's a missing override in `node`,
            // which means we will produce an error if any package in the package graph is missing
            // an override. However, that means if a package doesn't have its override set
            // properly, you can't depend on it. Should we relax this check to only produce an error
            // for the root node?
            for (original_id, nodes) in transitive_deps.into_iter() {
                linkage.insert(
                    original_id.clone(),
                    self.select_dep(node, original_id, nodes, &overrides)?,
                );
            }

            // if this node is published, add it to its linkage
            if let Some(oid) = package_node.original_id() {
                let old_entry = linkage.insert(oid, *node);
                if old_entry.is_some() {
                    // this means a package depends on another package that has the same original
                    // id (but it's a different package since we already checked for cycles)
                    return Err(LinkageError::DependsOnSelf);
                }
            }

            linkages.insert(*node, linkage);
        }

        let root = sorted[0];
        Ok(linkages
            .remove(&sorted[0]) // root package is first in topological order
            .expect("all linkages have been computed"))
    }

    /// Returns the the packages that are overridden in `node`, keyed by their original IDs (only
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

            let old = result.insert(oid, edge.target());
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

#[cfg(test)]
mod tests {
    use crate::{
        schema::{OriginalID, PublishedID},
        test_utils::graph_builder::TestPackageGraph,
    };
    use test_log::test;

    // TODO: add error message snapshots for the tests that produce errors

    /// `root` depends on `a` depends on `b` and `c`, both of which depend on `d`
    /// Computing linkage for both `root` and `a` should succeed
    #[test(tokio::test)]
    async fn test_diamond() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c", "d"])
            .add_deps([
                ("root", "a"),
                ("a", "b"),
                ("a", "c"),
                ("b", "d"),
                ("c", "d"),
            ])
            .build();

        scenario.graph_for("root").await.linkage().unwrap();
        scenario.graph_for("a").await.linkage().unwrap();
    }

    /// `root` depends on `a` which depends on `b` and `c`, which depend on `d1` and `d2` respectively
    /// Computing linkage for both `root` and `a` should fail due to inconsistent versions
    #[test(tokio::test)]
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

    /// `root` depends on `a` which depends on `b` and `c`, which depend on `d1` and `d2`
    /// respectively, BUT `d1` and `d2` have the same published-at address.
    ///
    /// In the current iteration this should fail, but in the future we may want to enable it
    #[test(tokio::test)]
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

    /// `root` depends on `a` depends on `b` and `c` which depend on `d1`  and `d2`, but `a` has an override
    /// dependency on `d3`.
    /// Computing linkage for both `a` and `root` should succeed
    #[test(tokio::test)]
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

    /// `root` depends on `a` which depends on `b`, `c`, and `d3` (non-override), while `b` depends on `d2` and `c` depends
    /// on `d3`
    /// Computing linkage for both `a` and `root` should fail because of the inconsistent linkage
    #[test(tokio::test)]
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

    /// `root` depends on `a` which depends on `b` and `d1`, `b` depends on `d2`
    /// Computing linkage for both `a` and `root` should fail because of linkage to `d1` and `d2`
    #[test(tokio::test)]
    async fn test_direct_and_transitive_nooverride() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_published("d1", OriginalID::from(1), PublishedID::from(1))
            .add_published("d2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([("root", "a"), ("a", "b"), ("a", "d1"), ("b", "d2")])
            .build();

        assert!(scenario.graph_for("root").await.linkage().is_err());
        assert!(scenario.graph_for("a").await.linkage().is_err());
    }

    /// `root` depends on `a` which depends on `b1` and `b2`.
    /// Computing linkage for both `root` and `a` should fail because of conflicting
    /// implementations of `b`
    #[test(tokio::test)]
    async fn test_direct_no_override() {
        let scenario = TestPackageGraph::new(["root", "a"])
            .add_published("b1", OriginalID::from(1), PublishedID::from(1))
            .add_published("b2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([("root", "a"), ("a", "b1"), ("a", "b2")])
            .build();

        assert!(scenario.graph_for("root").await.linkage().is_err());
        assert!(scenario.graph_for("a").await.linkage().is_err());
    }

    /// Same as [test_direct_no_override] except one of the deps is an override:
    /// `root` depends on `a` which depends on `b1` and `b2`; the dependency on `b2` is an
    /// override.
    ///
    /// It's unclear what we should do in this case. On the one hand, the user has probably made a
    /// mistake to end up in such a weird situation. On the other hand, the semantics are clear:
    /// for compilation `b1` refers to `b1` and `b2` refers to `b2`, while at runtime `b2` is used
    /// for both. In a sense, `a` is overriding its own dependencies.
    ///
    /// Currently we allow this since it is simpler and doesn't break anything. It should really be
    /// a lint (ha!)
    #[test(tokio::test)]
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

    /// Same as [test_direct_no_override] except both of the deps are overrides
    #[test(tokio::test)]
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

    /// `root` depends on `a` which depends on `b` which depends on `c` which depends on `a`
    /// Computing linkage for both `a` and `root` should fail because of cyclic dependency
    #[test(tokio::test)]
    async fn test_cyclic() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_deps([("root", "a"), ("a", "b"), ("b", "c"), ("c", "a")])
            .build();

        assert!(scenario.graph_for("root").await.linkage().is_err());
        assert!(scenario.graph_for("a").await.linkage().is_err());
    }

    /// `root` depends on `a1` which depends on `b` which depends on `a2`.
    /// Computing linkgage for both `a1` and `root` should fail because `a1` depends transitively
    /// on a different version of itself
    #[test(tokio::test)]
    async fn test_dep_on_different_version_of_self() {
        let scenario = TestPackageGraph::new(["root", "b"])
            .add_published("a1", OriginalID::from(1), PublishedID::from(1))
            .add_published("a2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([("root", "a1"), ("a1", "b"), ("b", "a2")])
            .build();

        assert!(scenario.graph_for("root").await.linkage().is_err());
        assert!(scenario.graph_for("a1").await.linkage().is_err());
    }

    /// `root` depends on `a` which depends on `b` twice (as `b` and `b2`)
    /// Computing linkage for both `root` and `a` should succeed (although this is arguably a
    /// corner case)
    #[test(tokio::test)]
    async fn test_double_dep() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_deps([("root", "a"), ("a", "b")])
            .add_dep("a", "b", |dep| dep.name("b2"))
            .build();

        scenario.graph_for("root").await.linkage().unwrap();
        scenario.graph_for("a").await.linkage().unwrap();
    }

    /// Same as [test_double_dep] except that the dependencies are overrides
    #[test(tokio::test)]
    async fn test_double_dep_override() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_deps([("root", "a")])
            .add_dep("a", "b", |dep| dep.set_override())
            .add_dep("a", "b", |dep| dep.set_override().name("b2"))
            .build();

        scenario.graph_for("root").await.linkage().unwrap();
        scenario.graph_for("a").await.linkage().unwrap();
    }
}
