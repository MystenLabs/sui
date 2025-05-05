// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    execution_mode::ExecutionMode,
    static_programmable_transactions::linkage::analysis::{
        config::ResolutionConfig,
        resolution::{ConflictResolution, ResolutionTable, add_and_unify, get_package},
    },
};
use std::{collections::BTreeMap, rc::Rc};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    error::ExecutionError,
    execution_config_utils::to_binary_config,
    transaction as P,
};

mod config;
mod legacy_linkage;
mod resolution;

pub trait LinkageAnalysis {
    fn compute_call_linkage(
        &self,
        move_call: &P::ProgrammableMoveCall,
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError>;

    fn compute_publication_linkage(
        &self,
        deps: &[ObjectID],
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError>;

    fn config(&self) -> &ResolutionConfig;
}

pub fn linkage_analysis_for_protocol_config<Mode: ExecutionMode>(
    protocol_config: &ProtocolConfig,
    _tx: &P::ProgrammableTransaction,
    store: &dyn PackageStore,
) -> Result<Box<dyn LinkageAnalysis>, ExecutionError> {
    Ok(Box::new(legacy_linkage::LegacyLinkage::new(
        !Mode::packages_are_predefined(),
        to_binary_config(protocol_config),
        store,
    )?))
}

pub type ResolvedLinkage = Rc<ResolvedLinkage_>;

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
    fn from_resolution_table(resolution_table: ResolutionTable) -> Rc<Self> {
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

/// Given a list of object IDs, generate a `ResolvedLinkage` for them.
/// Since this linkage analysis should only be used for types, all packages are resolved
/// "upwards" (i.e., later versions of the package are preferred).
pub fn type_linkage(
    ids: &[ObjectID],
    store: &dyn PackageStore,
) -> Result<ResolvedLinkage, ExecutionError> {
    let mut resolution_table = ResolutionTable::empty();
    for id in ids {
        let pkg = get_package(id, store)?;
        let transitive_deps = pkg
            .linkage_table()
            .values()
            .map(|info| info.upgraded_id)
            .collect::<Vec<_>>();
        let package_id = pkg.id();
        add_and_unify(
            &package_id,
            store,
            &mut resolution_table,
            ConflictResolution::at_least,
        )?;
        for object_id in transitive_deps.iter() {
            add_and_unify(
                object_id,
                store,
                &mut resolution_table,
                ConflictResolution::at_least,
            )?;
        }
    }

    Ok(ResolvedLinkage_::from_resolution_table(resolution_table))
}
