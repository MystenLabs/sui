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
use std::collections::BTreeMap;
use sui_types::{error::ExecutionError, transaction as P};

#[derive(Debug)]
pub struct LegacyLinkage {
    internal: ResolutionConfig,
}

impl LinkageAnalysis for LegacyLinkage {
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

    pub fn add_command(
        &self,
        command: &P::Command,
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        let mut unification_table = ResolutionTable {
            resolution_table: BTreeMap::new(),
            all_versions_resolution_table: BTreeMap::new(),
        };
        Ok(ResolvedLinkage::from_resolution_table(
            self.internal
                .add_command(command, store, &mut unification_table)?,
        ))
    }
}
