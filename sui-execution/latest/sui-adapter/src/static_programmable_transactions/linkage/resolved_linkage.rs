// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::linkage::resolution::{
    ConflictResolution, ResolutionTable,
};
use move_core_types::account_address::AccountAddress;
use std::{collections::BTreeMap, rc::Rc};
use sui_types::base_types::{ObjectID, SequenceNumber};

pub type ResolvedLinkage = Rc<ResolvedLinkage_>;

#[derive(Clone, Debug)]
pub struct RootedLinkage {
    pub link_context: AccountAddress,
    pub resolved_linkage: ResolvedLinkage,
}

impl RootedLinkage {
    pub fn new(link_context: AccountAddress, resolved_linkage: ResolvedLinkage) -> RootedLinkage {
        Self {
            link_context,
            resolved_linkage,
        }
    }

    pub fn new_with_default_context(resolved_linkage: ResolvedLinkage) -> RootedLinkage {
        Self {
            link_context: AccountAddress::ZERO,
            resolved_linkage,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedLinkage_ {
    pub linkage: BTreeMap<ObjectID, ObjectID>,
    // A mapping of every package ID to its runtime ID.
    // Note: Multiple packages can have the same runtime ID in this mapping, and domain of this map
    // is a superset of range of `linkage`.
    pub linkage_resolution: BTreeMap<ObjectID, ObjectID>,
    pub versions: BTreeMap<ObjectID, SequenceNumber>,
}

impl ResolvedLinkage_ {
    /// In the current linkage resolve an object ID to its original package ID.
    pub fn resolve_to_original_id(&self, object_id: &ObjectID) -> Option<ObjectID> {
        self.linkage_resolution.get(object_id).copied()
    }

    /// Create a `ResolvedLinkage` from a `ResolutionTable`.
    pub(crate) fn from_resolution_table(resolution_table: ResolutionTable) -> Rc<Self> {
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
        Rc::new(Self {
            linkage,
            linkage_resolution: resolution_table.all_versions_resolution_table,
            versions,
        })
    }
}
