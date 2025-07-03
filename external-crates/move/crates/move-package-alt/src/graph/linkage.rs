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

use crate::{
    flavor::MoveFlavor,
    package::{EnvironmentName, PackageName},
    schema::OriginalID,
};

use super::PackageGraph;

#[derive(Debug, Error)]
pub enum LinkageError {
    #[error(
        "Package <TODO: root> depends on <TODO: p1> and <TODO: p2>, but these depend on different versions of <TODO: conflict>:

           <TODO: p1> -> <TODO: p1'> -> <TODO: p1''> refers version <TODO: v1> (published at <TODO: abbrev. addr1>)
           <TODO: p2> -> <TODO: p2'> -> <TODO: p2''> -> <TODO: p2'''> refers to version <TODO: v2> (published at <TODO: abbrev. addr2>)

        To resolve this, you must explicitly add an override in <TODO: root>'s Move.toml:

           <TODO: conflict> = {{ <TODO: manifest dep for later version of conflict>, override = true }}
    "
    )]
    InconsistentLinkage {
        root: NodeIndex,
        node1: NodeIndex,
        node2: NodeIndex,
    },

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
    pub fn linkage(&self, env: &EnvironmentName) -> LinkageResult<LinkageTable> {
        let sorted = toposort(&self.inner, None).map_err(LinkageError::CyclicDependencies)?;

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
            let overrides = self.overrides(env, *node);
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
    fn overrides(&self, env: &EnvironmentName, node: NodeIndex) -> BTreeMap<OriginalID, NodeIndex> {
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

        let result = *nodes.first().expect("nodes is nonempty");
        for node in nodes {
            if node != result {
                // TODO: possibly look at nodes to see which is newer to produce an error message
                // suggestion
                //
                // TODO: possibly allow overlaps if the published-at fields are the same (e.g. to
                // handle bytecode and source packages for the same on-chain package)
                return Err(LinkageError::MultipleImplementations {
                    root: *root,
                    node1: *result,
                    node2: *node,
                });
            }
        }

        Ok(*result)
    }
}
