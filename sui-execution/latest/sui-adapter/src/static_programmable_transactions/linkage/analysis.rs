// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    execution_mode::ExecutionMode,
    static_programmable_transactions::linkage::{
        config::ResolutionConfig,
        legacy_linkage,
        resolution::{ConflictResolution, ResolutionTable, add_and_unify, get_package},
        resolved_linkage::ResolvedLinkage,
    },
};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::ObjectID, error::ExecutionError, execution_config_utils::to_binary_config,
    transaction as P,
};

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

    Ok(ResolvedLinkage::from_resolution_table(resolution_table))
}
