// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, btree_map::Entry};

use derive_where::derive_where;
use indoc::formatdoc;
use petgraph::visit::EdgeRef;
use thiserror::Error;

use crate::{
    flavor::MoveFlavor,
    schema::{OriginalID, PackageName},
};

use super::{PackageGraph, PackageInfo};

#[derive(Debug, Error)]
pub enum LinkageError {
    #[error("{0}")]
    InconsistentLinkage(String),

    // TODO: this error message could be better - it should include the dependency names for `dep1`
    // and `dep2`
    #[error("{0}")]
    ConflictingOverrides(String),

    // TODO: this error message could be better - it should include the path from `dep1` to `dep2`
    #[error("{0}")]
    CyclicDependencies(String),
}

pub type LinkageResult<T> = Result<T, LinkageError>;

/// Mapping from original ID to the package info to use for that address
pub type LinkageTable<'graph, F> = BTreeMap<OriginalID, PackageInfo<'graph, F>>;

impl<F: MoveFlavor> PackageGraph<F> {
    /// Construct and return a linkage table for the root package of `self`. The linkage table for a
    /// given package indicates which package nodes it should use for its transitive dependencies.
    ///
    /// For each package `p` in the linkage table and each direct dependency `d` of `p`, there must
    /// be a package `d2` in the linkage table with the same original ID as `d`. Moreover, if `d`
    /// and `d2` are different, `d2`'s version must be at least as high as `d`'s.
    ///
    /// The overrides must be declared: for every package `p` in the linkage and every dependency
    /// `p --> d`, if the linkage for `d` is different from `d`, then every path from the root to
    /// `p` must contain a package with an override dependency on `d2`.
    ///
    /// The linkage table is constructed by starting at the root and walking down the dependency
    /// tree while maintaining a current set of overrides along the path; dependencies are first
    /// replaced with their overrides, and then recursively traversed. Once the linkage tables for
    /// the dependencies are constructed, they are merged and returned.
    pub fn linkage(&self) -> LinkageResult<LinkageTable<F>> {
        self.root_package_info().check_cycles(
            self.root_package_info().name().clone(),
            &mut Vec::new(),
            &mut BTreeMap::new(),
        )?;
        let linkage_result = self
            .root_package_info()
            .linkage_ignoring_overrides(&LinkageTable::new(), 0)?;

        if let Some(conflict) = linkage_result.best_conflict {
            return Err(LinkageError::inconsistent_linkage(
                &conflict.node,
                &conflict.conflict,
            ));
        }

        Ok(linkage_result
            .linkage
            .into_iter()
            .map(|(oid, (_, pkg))| (oid, pkg))
            .collect())
    }
}

#[derive_where(Clone)]
struct Conflict<'graph, F: MoveFlavor> {
    depth: u8,
    node: PackageInfo<'graph, F>,
    conflict: PackageInfo<'graph, F>,
}

#[derive_where(Default)]
struct TraversalState<'graph, F: MoveFlavor> {
    linkage: BTreeMap<OriginalID, (u8, PackageInfo<'graph, F>)>,
    best_conflict: Option<Conflict<'graph, F>>,
}

impl<F: MoveFlavor> Conflict<'_, F> {
    fn min(a: Option<Self>, b: Option<Self>) -> Option<Self> {
        match (a, b) {
            (None, b) => b,
            (a, None) => a,
            (Some(a), Some(b)) => Some(if a.depth < b.depth { a } else { b }),
        }
    }
}

impl<'graph, F: MoveFlavor> PackageInfo<'graph, F> {
    /// Return the linkage table for this node and its descendents, ignoring all dependencies with
    /// original IDs in `overrides`. The returned table will include an entry for this node.
    ///
    /// Each entry in the returned table also includes a depth for the node that defines the
    /// original ID; this is used to select the highest conflict in the tree in the case of
    /// multiple errors. See [tests::test_best_error] to understand why this is important.
    ///
    /// If there is a conflict, then `best_conflict` will be `Some(c)`, and the linkage will
    /// contain some node with the corresponding OriginalID
    fn linkage_ignoring_overrides(
        &self,
        overrides: &LinkageTable<'graph, F>,
        depth: u8,
    ) -> LinkageResult<TraversalState<'graph, F>> {
        let mut local_overrides = self.overrides()?;
        for (addr, pkg) in overrides {
            local_overrides.insert(addr.clone(), pkg.clone());
        }

        // combine linkage from direct deps
        let mut result = TraversalState::default();

        for (_, pkg) in self.direct_deps().into_iter() {
            if overrides.contains_key(&pkg.original_id()) {
                continue;
            }

            // see if we got a better error
            let child_result = pkg.linkage_ignoring_overrides(&local_overrides, depth + 1)?;
            result.best_conflict = Conflict::min(result.best_conflict, child_result.best_conflict);

            // merge child linkage in
            for (addr, (new_depth, new_pkg)) in child_result.linkage {
                match result.linkage.entry(addr) {
                    Entry::Vacant(entry) => {
                        entry.insert((new_depth, new_pkg));
                    }
                    Entry::Occupied(mut linkage_entry) => {
                        let (old_depth, old_pkg) = linkage_entry.get();
                        let (min_depth, min_pkg, other_pkg) = if new_depth < *old_depth {
                            (new_depth, new_pkg.clone(), old_pkg.clone())
                        } else {
                            (*old_depth, old_pkg.clone(), new_pkg.clone())
                        };

                        if old_pkg.node != new_pkg.node {
                            // new conflict
                            let conflict = Conflict {
                                depth: min_depth,
                                node: min_pkg.clone(),
                                conflict: other_pkg,
                            };
                            result.best_conflict =
                                Conflict::min(result.best_conflict, Some(conflict).clone());
                        }

                        linkage_entry.insert((min_depth, min_pkg));
                    }
                };
            }
        }

        // include self
        result
            .linkage
            .insert(self.original_id(), (depth, self.clone()));

        Ok(result)
    }

    /// Returns the direct override dependencies of this node
    fn overrides<'a>(&'a self) -> LinkageResult<LinkageTable<'graph, F>> {
        let mut result: BTreeMap<OriginalID, (PackageName, PackageInfo<'graph, F>)> =
            BTreeMap::new();

        for edge in self.graph.inner.edges(self.node) {
            let dep = &edge.weight().dep;

            if !dep.is_override() {
                continue;
            }

            let target = self.graph.package_info(edge.target());
            match result.entry(target.original_id()) {
                Entry::Vacant(entry) => entry.insert((edge.weight().name.clone(), target)),
                Entry::Occupied(old) => {
                    if old.get().1.node == target.node {
                        continue;
                    } else {
                        return Err(LinkageError::conflicting_overrides(
                            self,
                            &old.get().0,
                            &edge.weight().name,
                            &old.get().1.original_id(),
                        ));
                    }
                }
            };
        }

        Ok(result
            .into_iter()
            .map(|(oid, (_, pkg))| (oid, pkg))
            .collect())
    }

    /// Check whether there are any cycles in the package graph; either between a node and itself
    /// or between two different nodes with the same original ID.
    ///
    /// `path` is the set of nodes that are on the path from `root` to this node, and
    /// `name_for_self` is the name that the last package in `path` uses to refer to `self`. `seen`
    /// is a map from original IDs to indices into `path`. A precondition is that every value of
    /// `seen` is in `path`
    ///
    /// If this method returns sucessfully, then `path` and `seen` will have exactly the same
    /// entries as when they were passed in.
    fn check_cycles(
        &self,
        name_for_self: PackageName,
        path: &mut Vec<(PackageName, PackageInfo<'graph, F>)>,
        seen: &mut BTreeMap<OriginalID, usize>,
    ) -> LinkageResult<()> {
        let self_index = path.len();
        path.push((name_for_self, self.clone()));

        if let Some(old) = seen.insert(self.original_id(), self_index) {
            return Err(LinkageError::cyclic_dependencies(path, old));
        }

        for (name, dep) in self.direct_deps() {
            dep.check_cycles(name, path, seen)?;
        }

        seen.remove(&self.original_id());
        path.pop();

        Ok(())
    }
}

impl LinkageError {
    /// Produce an error message indicating that the packages in `duplicates` can't be combined
    /// into a consistent linkage
    fn inconsistent_linkage<'graph, F: MoveFlavor>(
        d1: &PackageInfo<'graph, F>,
        d2: &PackageInfo<'graph, F>,
    ) -> Self {
        let oid = d1.original_id().truncated();

        let path1 = d1.display_path();
        let dep1 = d1.package().dep_for_self();
        let name1 = d1.name();

        let path2 = d2.display_path();
        let dep2 = d2.package().dep_for_self();

        let msg = formatdoc!(
            r###"
            Package depends on multiple versions of the package with ID {oid}:

              {path1} refers to {dep1}
              {path2} refers to {dep2}

            To resolve this, you must explicitly add an override in your Move.toml:

                [dependencies]
                _{name1} = {{ ..., override = true }}

            "###
        );

        Self::InconsistentLinkage(msg)
    }

    /// Given a `path` of dependencies starting from the root and ending in package P containing
    /// another package P' with the same original ID as P at index `i`, produce an error message
    /// describing the cycle
    fn cyclic_dependencies<F: MoveFlavor>(
        path: &Vec<(PackageName, PackageInfo<F>)>,
        conflict: usize,
    ) -> Self {
        let ancestor = path[conflict].1.display_name();
        let referenced_by = if conflict == 0 {
            "".to_string()
        } else {
            let mut conflict_path = String::from(" (referenced by ");
            conflict_path.push_str(path[0].0.as_str());
            for (name, _) in &path[1..conflict + 1] {
                conflict_path.push_str("::");
                conflict_path.push_str(name.as_str());
            }
            conflict_path.push(')');
            conflict_path
        };

        let mut conflict_path = ancestor.to_string();
        for (name, _) in &path[conflict + 1..] {
            conflict_path.push_str("::");
            conflict_path.push_str(name.as_str());
        }

        let detail = if path[conflict].1.node == path.last().expect("nonempty path").1.node {
            format!("dependency `{conflict_path}` refers back to `{ancestor}`")
        } else {
            let oid = path[conflict].1.original_id().truncated();
            format!(
                "dependency `{conflict_path}` resolves to a different version of the same package as `{ancestor}` (with original ID `{oid}`)"
            )
        };

        let msg =
            format!("Package `{ancestor}`{referenced_by} contains a cyclic dependency: {detail}");
        Self::CyclicDependencies(msg)
    }

    fn conflicting_overrides<F: MoveFlavor>(
        root: &PackageInfo<F>,
        dep1: &PackageName,
        dep2: &PackageName,
        oid: &OriginalID,
    ) -> Self {
        let root = root.display_name();
        let oid = oid.truncated();
        let msg = format!(
            "Package `{root}` has override dependencies `{dep1}` and `{dep2}` that both resolve to the package with ID {oid}"
        );
        Self::ConflictingOverrides(msg)
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
        test_utils::graph_builder::{Scenario, TestPackageGraph},
    };

    use insta::assert_snapshot;
    use test_log::test;

    async fn linkage_err(scenario: &Scenario, root: &str) -> String {
        scenario
            .graph_for(root)
            .await
            .linkage()
            .unwrap_err()
            .to_string()
    }

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

        assert_snapshot!(linkage_err(&scenario, "root").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          root::a::b::d1 refers to { local = "../d1" }
          root::a::c::d2 refers to { local = "../d2" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _d1 = { ..., override = true }
        "###);

        assert_snapshot!(linkage_err(&scenario, "a").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          a::b::d1 refers to { local = "../d1" }
          a::c::d2 refers to { local = "../d2" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _d1 = { ..., override = true }
        "###);
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

        assert_snapshot!(linkage_err(&scenario, "root").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          root::a::b::d1 refers to { local = "../d1" }
          root::a::c::d2 refers to { local = "../d2" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _d1 = { ..., override = true }
        "###);

        assert_snapshot!(linkage_err(&scenario, "a").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          a::b::d1 refers to { local = "../d1" }
          a::c::d2 refers to { local = "../d2" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _d1 = { ..., override = true }
        "###);
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

        assert_snapshot!(linkage_err(&scenario, "root").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          root::a::d3 refers to { local = "../d3" }
          root::a::b::d1 refers to { local = "../d1" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _d3 = { ..., override = true }
        "###);

        assert_snapshot!(linkage_err(&scenario, "a").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          a::d3 refers to { local = "../d3" }
          a::b::d1 refers to { local = "../d1" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _d3 = { ..., override = true }
        "###);
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

        assert_snapshot!(linkage_err(&scenario, "root").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          root::a::d1 refers to { local = "../d1" }
          root::a::b::d2 refers to { local = "../d2" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _d1 = { ..., override = true }
        "###);

        assert_snapshot!(linkage_err(&scenario, "a").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          a::d1 refers to { local = "../d1" }
          a::b::d2 refers to { local = "../d2" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _d1 = { ..., override = true }
        "###);
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

        assert_snapshot!(linkage_err(&scenario, "root").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          root::a::b1 refers to { local = "../b1" }
          root::a::b2 refers to { local = "../b2" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _b1 = { ..., override = true }
        "###);

        assert_snapshot!(linkage_err(&scenario, "a").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          a::b1 refers to { local = "../b1" }
          a::b2 refers to { local = "../b2" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _b1 = { ..., override = true }
        "###);
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

        assert_snapshot!(linkage_err(&scenario, "root").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          root::a::b1 refers to { local = "../b1" }
          root::a::b2 refers to { local = "../b2" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _b1 = { ..., override = true }
        "###);

        assert_snapshot!(linkage_err(&scenario, "a").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          a::b1 refers to { local = "../b1" }
          a::b2 refers to { local = "../b2" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _b1 = { ..., override = true }
        "###);
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

        assert_snapshot!(linkage_err(&scenario, "root").await, @"Package `a` has override dependencies `b2` and `b1` that both resolve to the package with ID 0x00...0001");
        assert_snapshot!(linkage_err(&scenario, "a").await, @"Package `a` has override dependencies `b2` and `b1` that both resolve to the package with ID 0x00...0001");
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

        assert_snapshot!(linkage_err(&scenario, "root").await, @"Package `a` (referenced by root::a) contains a cyclic dependency: dependency `a::b::c::a` refers back to `a`");
        assert_snapshot!(linkage_err(&scenario, "a").await, @"Package `a` contains a cyclic dependency: dependency `a::b::c::a` refers back to `a`");
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
    async fn test_cyclic_different_version() {
        let scenario = TestPackageGraph::new(["root", "b"])
            .add_published("a1", OriginalID::from(1), PublishedID::from(1))
            .add_published("a2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([("root", "a1"), ("a1", "b"), ("b", "a2")])
            .build();

        assert_snapshot!(linkage_err(&scenario, "root").await, @"Package `a1` (referenced by root::a1) contains a cyclic dependency: dependency `a1::b::a2` resolves to a different version of the same package as `a1` (with original ID `0x00...0001`)");
        assert_snapshot!(linkage_err(&scenario, "a1").await, @"Package `a1` contains a cyclic dependency: dependency `a1::b::a2` resolves to a different version of the same package as `a1` (with original ID `0x00...0001`)");
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

        assert_snapshot!(linkage_err(&scenario, "root").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          root::a::c::e2 refers to { local = "../e2" }
          root::a::c::d::e1 refers to { local = "../e1" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _e2 = { ..., override = true }
        "###);

        assert_snapshot!(linkage_err(&scenario, "a").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          a::c::e2 refers to { local = "../e2" }
          a::c::d::e1 refers to { local = "../e1" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _e2 = { ..., override = true }
        "###);
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
    ///     a --> c1
    ///     a --> e --> c2
    ///
    ///     c1 --> b1 --> d1
    ///     c2 --> b2 --> d2
    /// ```
    ///
    /// Here there are conflicts between b1/2, c1/2, and d1/2, but the best one to override is `c`
    /// because then you won't need to also override b and d. This test ensures that the error
    /// message describes the conflict on `c` instead of `b` or `d` (note that we choose addresses
    /// and names for `b` so that it's in the middle so we don't accidentally get the right answer
    /// because of sorting)
    ///
    /// This is important for good devX because if we reported `d` for example, the user would add
    /// an override for `d` and then hit a conflict for `b`, and finally would hit a conflict for
    /// `c`, when a override for `c` would do the trick.
    #[cfg_attr(doc, aquamarine::aquamarine)]
    #[cfg_attr(not(doc), test(tokio::test))]
    async fn test_best_error() {
        let scenario = TestPackageGraph::new(["root", "a", "e"])
            .add_published("c1", OriginalID::from(3), PublishedID::from(1))
            .add_published("c2", OriginalID::from(3), PublishedID::from(2))
            .add_published("b1", OriginalID::from(1), PublishedID::from(1))
            .add_published("b2", OriginalID::from(1), PublishedID::from(2))
            .add_published("d1", OriginalID::from(5), PublishedID::from(1))
            .add_published("d2", OriginalID::from(5), PublishedID::from(2))
            .add_deps([
                ("root", "a"),
                ("a", "c1"),
                ("c1", "b1"),
                ("b1", "d1"),
                ("a", "e"),
                ("e", "c2"),
                ("c2", "b2"),
                ("b2", "d2"),
            ])
            .build();

        // NOTE: read the doc comment for this test before updating snapshot
        assert_snapshot!(linkage_err(&scenario, "root").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0003:

          root::a::c1 refers to { local = "../c1" }
          root::a::e::c2 refers to { local = "../c2" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _c1 = { ..., override = true }
        "###);

        // NOTE: read the doc comment for this test before updating snapshot
        assert_snapshot!(linkage_err(&scenario, "a").await, @r###"
        Package depends on multiple versions of the package with ID 0x00...0003:

          a::c1 refers to { local = "../c1" }
          a::e::c2 refers to { local = "../c2" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _c1 = { ..., override = true }
        "###);
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
    #[ignore] // TODO: version checks not implemented
    async fn test_nested_overrides_bad_version() {
        let scenario = TestPackageGraph::new(["root", "a", "b", "c"])
            .add_published("d1", OriginalID::from(1), PublishedID::from(1))
            .add_published("d2", OriginalID::from(1), PublishedID::from(2))
            .add_published("d3", OriginalID::from(1), PublishedID::from(3))
            .add_deps([("root", "a"), ("a", "b"), ("b", "c"), ("c", "d1")])
            .add_dep("b", "d3", |dep| dep.set_override())
            .add_dep("a", "d2", |dep| dep.set_override())
            .build();

        assert_snapshot!(linkage_err(&scenario, "root").await, @"");
        assert_snapshot!(linkage_err(&scenario, "a").await, @"");
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
