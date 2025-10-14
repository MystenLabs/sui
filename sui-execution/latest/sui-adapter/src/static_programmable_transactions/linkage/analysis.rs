// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    execution_mode::ExecutionMode,
    static_programmable_transactions::{
        linkage::{
            config::{LinkageConfig, ResolutionConfig},
            resolution::{ResolutionTable, VersionConstraint, add_and_unify, get_package},
            resolved_linkage::ResolvedLinkage,
        },
        transaction_meter::TransactionMeter,
    },
};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::ObjectID, error::ExecutionError, execution_config_utils::to_binary_config,
    transaction as P,
};

#[derive(Debug)]
pub struct LinkageAnalyzer {
    internal: ResolutionConfig,
}

impl LinkageAnalyzer {
    pub fn new<Mode: ExecutionMode>(
        protocol_config: &ProtocolConfig,
    ) -> Result<Self, ExecutionError> {
        let always_include_system_packages = !Mode::packages_are_predefined();
        let linkage_config = LinkageConfig::legacy_linkage_settings(always_include_system_packages);
        let binary_config = to_binary_config(protocol_config);
        Ok(Self {
            internal: ResolutionConfig {
                linkage_config,
                binary_config,
            },
        })
    }

    pub fn compute_call_linkage(
        &self,
        move_call: &P::ProgrammableMoveCall,
        store: &dyn PackageStore,
        gas: &TransactionMeter<'_, '_>,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        Ok(ResolvedLinkage::from_resolution_table(
            self.compute_call_linkage_(move_call, store, gas)?,
        ))
    }

    pub fn compute_publication_linkage(
        &self,
        deps: &[ObjectID],
        store: &dyn PackageStore,
        gas: &TransactionMeter<'_, '_>,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        Ok(ResolvedLinkage::from_resolution_table(
            self.compute_publication_linkage_(deps, store, gas)?,
        ))
    }

    pub fn config(&self) -> &ResolutionConfig {
        &self.internal
    }

    fn compute_call_linkage_(
        &self,
        move_call: &P::ProgrammableMoveCall,
        store: &dyn PackageStore,
        gas: &TransactionMeter<'_, '_>,
    ) -> Result<ResolutionTable, ExecutionError> {
        let mut resolution_table = self
            .internal
            .linkage_config
            .resolution_table_with_native_packages(store, gas)?;
        let pkg = get_package(&move_call.package, store, gas)?;
        let transitive_deps = pkg
            .linkage_table()
            .values()
            .map(|info| info.upgraded_id)
            .collect::<Vec<_>>();
        for object_id in transitive_deps.iter() {
            add_and_unify(
                object_id,
                store,
                &mut resolution_table,
                VersionConstraint::exact,
                gas,
            )?;
        }
        add_and_unify(
            &move_call.package,
            store,
            &mut resolution_table,
            VersionConstraint::exact,
            gas,
        )?;
        Ok(resolution_table)
    }

    /// Compute the linkage for a publish or upgrade command. This is a special case because
    fn compute_publication_linkage_(
        &self,
        deps: &[ObjectID],
        store: &dyn PackageStore,
        gas: &TransactionMeter<'_, '_>,
    ) -> Result<ResolutionTable, ExecutionError> {
        let mut resolution_table = self
            .internal
            .linkage_config
            .resolution_table_with_native_packages(store, gas)?;
        for id in deps {
            add_and_unify(
                id,
                store,
                &mut resolution_table,
                VersionConstraint::exact,
                gas,
            )?;
        }
        Ok(resolution_table)
    }
}
