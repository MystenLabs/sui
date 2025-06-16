// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    static_programmable_transactions::linkage::{
        analysis::LinkageAnalysis,
        config::{LinkageConfig, ResolutionConfig},
        resolution::{ConflictResolution, ResolutionTable, add_and_unify, get_package},
        resolved_linkage::{ResolvedLinkage, ResolvedLinkage_},
    },
};
use move_binary_format::binary_config::BinaryConfig;
use sui_types::{base_types::ObjectID, error::ExecutionError, transaction as P};

#[derive(Debug)]
pub struct LegacyLinkage {
    internal: ResolutionConfig,
}

impl LinkageAnalysis for LegacyLinkage {
    fn compute_call_linkage(
        &self,
        move_call: &P::ProgrammableMoveCall,
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        Ok(ResolvedLinkage_::from_resolution_table(
            self.compute_call_linkage(move_call, store)?,
        ))
    }

    fn compute_publication_linkage(
        &self,
        deps: &[ObjectID],
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        Ok(ResolvedLinkage_::from_resolution_table(
            self.compute_publication_linkage(deps, store)?,
        ))
    }

    fn config(&self) -> &ResolutionConfig {
        &self.internal
    }
}

impl LegacyLinkage {
    #[allow(dead_code)]
    pub fn new(
        always_include_system_packages: bool,
        binary_config: BinaryConfig,
        _store: &dyn PackageStore,
    ) -> Result<Self, ExecutionError> {
        let linkage_config = LinkageConfig::legacy_linkage_settings(always_include_system_packages);
        Ok(Self {
            internal: ResolutionConfig {
                linkage_config,
                binary_config,
            },
        })
    }

    fn compute_call_linkage(
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
                ConflictResolution::exact,
            )?;
        }
        Ok(resolution_table)
    }

    /// Compute the linkage for a publish or upgrade command. This is a special case because
    pub(crate) fn compute_publication_linkage(
        &self,
        deps: &[ObjectID],
        store: &dyn PackageStore,
    ) -> Result<ResolutionTable, ExecutionError> {
        let mut resolution_table = self
            .internal
            .linkage_config
            .resolution_table_with_native_packages(store)?;
        for id in deps {
            let pkg = get_package(id, store)?;
            resolution_table.resolution_table.insert(
                pkg.original_package_id(),
                ConflictResolution::Exact(pkg.version(), pkg.id()),
            );
            resolution_table
                .all_versions_resolution_table
                .insert(pkg.id(), pkg.original_package_id());
        }
        Ok(resolution_table)
    }
}
