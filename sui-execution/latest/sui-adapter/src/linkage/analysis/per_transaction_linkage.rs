// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    linkage::analysis::{
        LinkageAnalysis, ResolvedLinkage,
        config::{LinkageConfig, ResolutionConfig},
        resolution::ResolutionTable,
    },
};
use move_binary_format::binary_config::BinaryConfig;
use std::cell::RefCell;
use sui_types::{error::ExecutionError, transaction as P};

#[derive(Debug)]
pub struct UnifiedLinkage {
    /// Current unification table we have for packages. This is a mapping from the original
    /// package ID for a package to its current resolution. This is the "constraint set" that we
    /// are building/solving as we progress across the PTB.
    unification_table: RefCell<ResolutionTable>,
    internal: ResolutionConfig,
}

impl LinkageAnalysis for UnifiedLinkage {
    fn add_command(
        &self,
        command: &P::Command,
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        self.add_command(command, store)
    }

    fn config(&self) -> &ResolutionConfig {
        &self.internal
    }
}

impl UnifiedLinkage {
    #[allow(dead_code)]
    pub fn new(
        always_include_system_packages: bool,
        binary_config: BinaryConfig,
        store: &dyn PackageStore,
    ) -> Result<Self, ExecutionError> {
        let linkage_config =
            LinkageConfig::unified_linkage_settings(always_include_system_packages);
        let unification_table = linkage_config.resolution_table_with_native_packages(store)?;
        Ok(Self {
            internal: ResolutionConfig {
                linkage_config,
                binary_config,
            },
            unification_table: RefCell::new(unification_table),
        })
    }

    pub fn add_command(
        &self,
        command: &P::Command,
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        Ok(ResolvedLinkage::from_resolution_table(
            self.internal
                .add_command(command, store, &mut self.unification_table.borrow_mut())?,
        ))
    }
}
