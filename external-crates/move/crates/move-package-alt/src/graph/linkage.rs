// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use move_core_types::account_address::AccountAddress;
use petgraph::{
    algo::{Cycle, toposort},
    graph::{DiGraph, NodeIndex},
};
use thiserror::Error;

use crate::{
    flavor::MoveFlavor,
    package::{Package, PackageName},
    schema::Address,
};

use super::PackageGraph;

#[derive(Debug, Error)]
pub enum LinkageError {
    #[error("packages TODO and TODO are different versions of the same package")]
    InconsistentLinkage {
        path1: Vec<PackageName>,
        path2: Vec<PackageName>,
    },

    #[error("found a cycle in the dependency graph (TODO: show cycle)")]
    CyclicDependencies(Cycle<NodeIndex>),
}

pub type LinkageResult<T> = Result<T, LinkageError>;

/// Mapping from original ID to the package to use for that address
pub type LinkageTable<F> = BTreeMap<AccountAddress, Package<F>>;

impl<F: MoveFlavor> PackageGraph<F> {
    /// Construct and return a linkage table for the root package of `self`
    pub fn linkage(&self) -> LinkageResult<LinkageTable<F>> {
        let sorted =
            toposort(&self.inner, None).map_err(|cycle| LinkageError::CyclicDependencies(cycle))?;

        let mut linkages: BTreeMap<NodeIndex, LinkageTable<F>> = BTreeMap::new();
        for node in sorted.iter().rev() {
            // note: since we are iterating in reverse topological order, the linkages for the
            // dependencies have already been computed
        }

        Ok(linkages
            .remove(&sorted[0])
            .expect("all linkages have been computed"))
    }

    fn overrides(&self, node: NodeIndex) -> BTreeSet<Address> {
        todo!()
    }
}

/// Update `parent`
fn merge_linkage<F: MoveFlavor>(parent: &mut LinkageTable<F>, child: &LinkageTable<F>) {
    todo!()
}
