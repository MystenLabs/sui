// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    shared::{linkage_context::LinkageContext, types::PackageStorageId},
    validation::verification,
};

use move_binary_format::errors::{Location, PartialVMError, PartialVMResult, VMResult};
use move_core_types::vm_status::StatusCode;
use petgraph::{algo::toposort, prelude::DiGraphMap};

use std::collections::{BTreeMap, BTreeSet};

// -------------------------------------------------------------------------------------------------
// Dependency Analysis
// -------------------------------------------------------------------------------------------------

// FIXME(cswords): I think this is duplicated already in other places.

// Compute the immediate dependencies of a package in terms of their storage IDs.
pub fn compute_immediate_package_dependencies<'a>(
    link_context: &LinkageContext,
    pkg: &verification::ast::Package,
) -> VMResult<BTreeSet<PackageStorageId>> {
    pkg.modules
        .iter()
        .flat_map(|(_, m)| m.value.immediate_dependencies())
        .map(|m| Ok(*link_context.relocate(&m)?.address()))
        .filter(|m| m.as_ref().is_ok_and(|m| *m != pkg.storage_id))
        .collect::<PartialVMResult<BTreeSet<_>>>()
        .map_err(|e| {
            PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                .with_message(format!(
                    "Failed to locate immediate dependencies of package {}: {}",
                    pkg.storage_id, e
                ))
                .finish(Location::Undefined)
        })
}

pub fn compute_dependency_order(
    mut pkgs_to_cache: BTreeMap<
        PackageStorageId,
        (verification::ast::Package, BTreeSet<PackageStorageId>),
    >,
) -> PartialVMResult<Vec<(PackageStorageId, verification::ast::Package)>> {
    // Compute edges for the dependency graph
    let package_edges = pkgs_to_cache.iter().flat_map(|(package_id, (_, deps))| {
        deps.iter()
            .filter(|pkg| pkgs_to_cache.contains_key(pkg))
            .map(|dep_pkg| (*package_id, *dep_pkg))
    });

    let mut digraph = DiGraphMap::<PackageStorageId, ()>::from_edges(package_edges);

    // Make sure every package is in the graph (even if it has no dependencies)
    for pkg in pkgs_to_cache.keys() {
        digraph.add_node(*pkg);
    }

    Ok(toposort(&digraph, None)
        .map_err(|_| {
            PartialVMError::new(StatusCode::CYCLIC_PACKAGE_DEPENDENCY)
                .with_message("Cyclic package dependency detected".to_string())
        })?
        .into_iter()
        .map(|pkg| {
            (
                pkg,
                pkgs_to_cache
                    .remove(&pkg)
                    .expect("dependency order computation error")
                    .0,
            )
        })
        .collect())
}
