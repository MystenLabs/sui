// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data_store::PackageStore;
use std::{
    collections::{BTreeMap, btree_map::Entry},
    rc::Rc,
};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    error::{ExecutionError, ExecutionErrorKind},
    move_package::MovePackage,
};

/// Unifiers. These are used to determine how to unify two packages.
#[derive(Debug, Clone)]
pub enum ConflictResolution {
    /// An exact constraint unifies as follows:
    /// 1. Exact(a) ~ Exact(b) ==> Exact(a), iff a == b
    /// 2. Exact(a) ~ AtLeast(b) ==> Exact(a), iff a >= b
    Exact(SequenceNumber, ObjectID),
    /// An at least constraint unifies as follows:
    /// * AtLeast(a, a_version) ~ AtLeast(b, b_version) ==> AtLeast(x, max(a_version, b_version)),
    ///   where x is the package id of either a or b (the one with the greatest version).
    AtLeast(SequenceNumber, ObjectID),
}

#[derive(Debug, Clone)]
pub(crate) struct ResolutionTable {
    pub(crate) resolution_table: BTreeMap<ObjectID, ConflictResolution>,
    /// For every version of every package that we have seen, a mapping of the ObjectID for that
    /// package to its runtime ID.
    pub(crate) all_versions_resolution_table: BTreeMap<ObjectID, ObjectID>,
}

impl ResolutionTable {
    pub fn empty() -> Self {
        Self {
            resolution_table: BTreeMap::new(),
            all_versions_resolution_table: BTreeMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn merge(&mut self, other: &ResolutionTable) -> Result<(), ExecutionError> {
        for (object_id, resolution) in other.resolution_table.iter() {
            if let Entry::Vacant(e) = self.resolution_table.entry(*object_id) {
                e.insert(resolution.clone());
            } else {
                let existing_unifier = self
                    .resolution_table
                    .get_mut(object_id)
                    .expect("Guaranteed to exist");
                *existing_unifier = existing_unifier.unify(resolution)?;
            }
        }

        for (object_id, original_pkg_id) in other.all_versions_resolution_table.iter() {
            if !self.all_versions_resolution_table.contains_key(object_id) {
                self.all_versions_resolution_table
                    .insert(*object_id, *original_pkg_id);
            }
        }

        Ok(())
    }
}

impl ConflictResolution {
    pub fn exact(pkg: &MovePackage) -> Option<ConflictResolution> {
        Some(ConflictResolution::Exact(pkg.version(), pkg.id()))
    }

    pub fn at_least(pkg: &MovePackage) -> Option<ConflictResolution> {
        Some(ConflictResolution::AtLeast(pkg.version(), pkg.id()))
    }

    #[allow(dead_code)]
    pub fn no_constraint(_pkg: &MovePackage) -> Option<ConflictResolution> {
        None
    }

    pub fn unify(&self, other: &ConflictResolution) -> Result<ConflictResolution, ExecutionError> {
        match (&self, other) {
            // If we have two exact resolutions, they must be the same.
            (ConflictResolution::Exact(sv, self_id), ConflictResolution::Exact(ov, other_id)) => {
                if self_id != other_id || sv != ov {
                    Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::InvalidLinkage,
                        format!(
                            "exact/exact conflicting resolutions for package: linkage requires the same package \
                                 at different versions. Linkage requires exactly {self_id} (version {sv}) and \
                                 {other_id} (version {ov}) to be used in the same transaction"
                        ),
                    ))
                } else {
                    Ok(ConflictResolution::Exact(*sv, *self_id))
                }
            }
            // Take the max if you have two at least resolutions.
            (
                ConflictResolution::AtLeast(self_version, sid),
                ConflictResolution::AtLeast(other_version, oid),
            ) => {
                let id = if self_version > other_version {
                    *sid
                } else {
                    *oid
                };

                Ok(ConflictResolution::AtLeast(
                    *self_version.max(other_version),
                    id,
                ))
            }
            // If you unify an exact and an at least, the exact must be greater than or equal to
            // the at least. It unifies to an exact.
            (
                ConflictResolution::Exact(exact_version, exact_id),
                ConflictResolution::AtLeast(at_least_version, at_least_id),
            )
            | (
                ConflictResolution::AtLeast(at_least_version, at_least_id),
                ConflictResolution::Exact(exact_version, exact_id),
            ) => {
                if exact_version < at_least_version {
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::InvalidLinkage,
                        format!(
                            "Exact/AtLeast conflicting resolutions for package: linkage requires exactly this \
                                 package {exact_id} (version {exact_version}) and also at least the following \
                                 version of the package {at_least_id} at version {at_least_version}. However \
                                 {exact_id} is at version {exact_version} which is less than {at_least_version}."
                        ),
                    ));
                }

                Ok(ConflictResolution::Exact(*exact_version, *exact_id))
            }
        }
    }
}

/// Load a package from the store, and update the type origin map with the types in that
/// package.
pub(crate) fn get_package(
    object_id: &ObjectID,
    store: &dyn PackageStore,
) -> Result<Rc<MovePackage>, ExecutionError> {
    store
        .get_package(object_id)
        .map_err(|e| {
            ExecutionError::new_with_source(ExecutionErrorKind::PublishUpgradeMissingDependency, e)
        })?
        .ok_or_else(|| ExecutionError::from_kind(ExecutionErrorKind::InvalidLinkage))
}

// Add a package to the unification table, unifying it with any existing package in the table.
// Errors if the packages cannot be unified (e.g., if one is exact and the other is not).
pub(crate) fn add_and_unify(
    object_id: &ObjectID,
    store: &dyn PackageStore,
    resolution_table: &mut ResolutionTable,
    resolution_fn: fn(&MovePackage) -> Option<ConflictResolution>,
) -> Result<(), ExecutionError> {
    let package = get_package(object_id, store)?;

    let Some(resolution) = resolution_fn(&package) else {
        // If the resolution function returns None, we do not need to add this package to the
        // resolution table, and this does not contribute to the linkage analysis.
        return Ok(());
    };
    let original_pkg_id = package.original_package_id();

    if let Entry::Vacant(e) = resolution_table.resolution_table.entry(original_pkg_id) {
        e.insert(resolution);
    } else {
        let existing_unifier = resolution_table
            .resolution_table
            .get_mut(&original_pkg_id)
            .expect("Guaranteed to exist");
        *existing_unifier = existing_unifier.unify(&resolution)?;
    }

    if !resolution_table
        .all_versions_resolution_table
        .contains_key(object_id)
    {
        resolution_table
            .all_versions_resolution_table
            .insert(*object_id, original_pkg_id);
    }

    Ok(())
}
