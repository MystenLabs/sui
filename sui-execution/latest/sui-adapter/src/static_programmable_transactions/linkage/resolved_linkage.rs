// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    static_programmable_transactions::linkage::resolution::{
        ResolutionTable, VersionConstraint, add_and_unify, get_package,
    },
};
use move_vm_runtime::shared::linkage_context::LinkageContext;
use std::{collections::BTreeMap, rc::Rc};
use sui_types::{base_types::ObjectID, error::ExecutionError};

#[derive(Clone, Debug)]
pub struct ExecutableLinkage(pub Rc<ResolvedLinkage>);

impl ExecutableLinkage {
    pub fn new(resolved_linkage: ResolvedLinkage) -> Self {
        Self(Rc::new(resolved_linkage))
    }

    /// Given a list of object IDs, generate a `ResolvedLinkage` for them.
    /// Since this linkage analysis should only be used for types, all packages are resolved
    /// "upwards" (i.e., later versions of the package are preferred).
    pub fn type_linkage(
        ids: &[ObjectID],
        store: &dyn PackageStore,
    ) -> Result<Self, ExecutionError> {
        let mut resolution_table = ResolutionTable::empty();
        for id in ids {
            let pkg = get_package(id, store)?;
            let transitive_deps = pkg
                .linkage_table()
                .values()
                .map(|version_id| ObjectID::from(*version_id))
                .collect::<Vec<_>>();
            let package_id = pkg.version_id().into();
            add_and_unify(
                &package_id,
                store,
                &mut resolution_table,
                VersionConstraint::at_least,
            )?;
            for object_id in transitive_deps.iter() {
                add_and_unify(
                    object_id,
                    store,
                    &mut resolution_table,
                    VersionConstraint::at_least,
                )?;
            }
        }

        Ok(Self::new(ResolvedLinkage::from_resolution_table(
            resolution_table,
        )))
    }

    pub fn linkage_context(&self) -> LinkageContext {
        LinkageContext::new(self.0.linkage.iter().map(|(k, v)| (**k, **v)).collect())
    }
}

#[derive(Debug)]
pub struct ResolvedLinkage {
    // A mapping of original package ID to its resolved version ID for that linkage.
    pub linkage: BTreeMap<ObjectID, ObjectID>,
    // A mapping of every package ID to its runtime ID.
    // Note: Multiple packages can have the same runtime ID in this mapping, and domain of this map
    // is a superset of range of `linkage`.
    pub linkage_resolution: BTreeMap<ObjectID, ObjectID>,
}

impl ResolvedLinkage {
    /// In the current linkage resolve an object ID to its original package ID.
    pub fn resolve_to_original_id(&self, object_id: &ObjectID) -> Option<ObjectID> {
        self.linkage_resolution.get(object_id).copied()
    }

    /// Create a `ResolvedLinkage` from a `ResolutionTable`.
    pub(crate) fn from_resolution_table(resolution_table: ResolutionTable) -> Self {
        let mut linkage = BTreeMap::new();
        for (original_id, resolution) in resolution_table.resolution_table {
            match resolution {
                VersionConstraint::Exact(_version, object_id)
                | VersionConstraint::AtLeast(_version, object_id) => {
                    linkage.insert(original_id, object_id);
                }
            }
        }
        Self {
            linkage,
            linkage_resolution: resolution_table.all_versions_resolution_table,
        }
    }

    /// We need to late-bind the "self" resolution since for publication and upgrade we don't know
    /// this a priori when loading the PTB.
    pub fn update_for_publication(
        package_version_id: ObjectID,
        original_package_id: ObjectID,
        mut resolved_linkage: ResolvedLinkage,
    ) -> ExecutableLinkage {
        // original package ID maps to the link context (new package ID) in this context
        resolved_linkage
            .linkage
            .insert(original_package_id, package_version_id);
        // Add resolution from the new package ID to the original package ID.
        resolved_linkage
            .linkage_resolution
            .insert(package_version_id, original_package_id);
        ExecutableLinkage::new(resolved_linkage)
    }
}
