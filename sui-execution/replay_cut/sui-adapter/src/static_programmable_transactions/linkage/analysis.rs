// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::rc::Rc;

use crate::{
    data_store::PackageStore,
    execution_mode::ExecutionMode,
    static_programmable_transactions::{
        linkage::{
            config::{LinkageConfig, ResolutionConfig},
            resolution::{ResolutionTable, VersionConstraint, add_and_unify, get_package},
            resolved_linkage::{ResolvedLinkage, RootedLinkage},
        },
        loading::ast::Type,
    },
};
use sui_protocol_config::ProtocolConfig;
use sui_types::{base_types::ObjectID, error::ExecutionError, transaction as P};

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
        let binary_config = protocol_config.binary_config(None);
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
    ) -> Result<ResolvedLinkage, ExecutionError> {
        Ok(ResolvedLinkage::from_resolution_table(
            self.compute_call_linkage_(move_call, store)?,
        ))
    }

    pub fn compute_publication_linkage(
        &self,
        deps: &[ObjectID],
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        Ok(ResolvedLinkage::from_resolution_table(
            self.compute_publication_linkage_(deps, store)?,
        ))
    }

    pub fn config(&self) -> &ResolutionConfig {
        &self.internal
    }

    pub fn framework_call_linkage(
        &self,
        // Type arguments do not need to be included in the linkage in the current VM.
        _type_args: &[Type],
        store: &dyn PackageStore,
    ) -> Result<RootedLinkage, ExecutionError> {
        Ok(RootedLinkage {
            link_context: *sui_types::SUI_FRAMEWORK_PACKAGE_ID,
            resolved_linkage: Rc::new(ResolvedLinkage::from_resolution_table(
                self.internal
                    .linkage_config
                    .resolution_table_with_native_packages(store)?,
            )),
        })
    }

    fn compute_call_linkage_(
        &self,
        move_call: &P::ProgrammableMoveCall,
        store: &dyn PackageStore,
    ) -> Result<ResolutionTable, ExecutionError> {
        let mut resolution_table = self
            .internal
            .linkage_config
            .resolution_table_with_native_packages(store)?;
        let pkg = get_package(&move_call.package, store)?;
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
            )?;
        }
        add_and_unify(
            &move_call.package,
            store,
            &mut resolution_table,
            VersionConstraint::exact,
        )?;
        Ok(resolution_table)
    }

    /// Compute the linkage for a publish or upgrade command. This is a special case because
    fn compute_publication_linkage_(
        &self,
        deps: &[ObjectID],
        store: &dyn PackageStore,
    ) -> Result<ResolutionTable, ExecutionError> {
        let mut resolution_table = self
            .internal
            .linkage_config
            .resolution_table_with_native_packages(store)?;
        for id in deps {
            add_and_unify(id, store, &mut resolution_table, VersionConstraint::exact)?;
        }
        Ok(resolution_table)
    }
}
