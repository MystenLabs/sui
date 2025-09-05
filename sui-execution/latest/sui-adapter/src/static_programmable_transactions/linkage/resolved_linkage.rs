// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::linkage::resolution::{
    ConflictResolution, ResolutionTable,
};
use move_vm_runtime::shared::linkage_context::LinkageContext;
use std::{collections::BTreeMap, rc::Rc};
use sui_types::base_types::{ObjectID, SequenceNumber};

#[derive(Clone, Debug)]
pub struct ExecutableLinkage(pub Rc<ResolvedLinkage>);

impl ExecutableLinkage {
    pub fn new(resolved_linkage: ResolvedLinkage) -> Self {
        Self(Rc::new(resolved_linkage))
    }

    pub fn linkage_context(&self) -> LinkageContext {
        LinkageContext::new(self.0.linkage.iter().map(|(k, v)| (**k, **v)).collect())
    }
}

#[derive(Debug)]
pub struct ResolvedLinkage {
    pub linkage: BTreeMap<ObjectID, ObjectID>,
    // A mapping of every package ID to its runtime ID.
    // Note: Multiple packages can have the same runtime ID in this mapping, and domain of this map
    // is a superset of range of `linkage`.
    pub linkage_resolution: BTreeMap<ObjectID, ObjectID>,
    pub versions: BTreeMap<ObjectID, SequenceNumber>,
}

impl ResolvedLinkage {
    /// In the current linkage resolve an object ID to its original package ID.
    pub fn resolve_to_original_id(&self, object_id: &ObjectID) -> Option<ObjectID> {
        self.linkage_resolution.get(object_id).copied()
    }

    /// Create a `ResolvedLinkage` from a `ResolutionTable`.
    pub(crate) fn from_resolution_table(resolution_table: ResolutionTable) -> Self {
        let mut linkage = BTreeMap::new();
        let mut versions = BTreeMap::new();
        for (original_id, resolution) in resolution_table.resolution_table {
            match resolution {
                ConflictResolution::Exact(version, object_id)
                | ConflictResolution::AtLeast(version, object_id) => {
                    linkage.insert(original_id, object_id);
                    versions.insert(original_id, version);
                }
            }
        }
        Self {
            linkage,
            linkage_resolution: resolution_table.all_versions_resolution_table,
            versions,
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
