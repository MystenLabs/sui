// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod analysis;
pub mod component_based_linkage;
pub mod config;
pub mod resolution;
pub mod resolved_linkage;

use crate::{
    data_store::PackageStore,
    execution_mode::ExecutionMode,
    static_programmable_transactions::{
        linkage::analysis::LinkageAnalyzer, loading::ast as loading,
    },
};
use sui_protocol_config::ProtocolConfig;

pub fn refine_linkage<Mode: ExecutionMode>(
    mut txn: loading::Transaction,
    linkage_analysis: &LinkageAnalyzer,
    package_store: &dyn PackageStore,
    protocol_config: &ProtocolConfig,
) -> Result<loading::Transaction, Mode::Error> {
    if !protocol_config.enable_component_based_linkage() {
        return Ok(txn);
    }

    component_based_linkage::refine_per_component_linkage::<Mode::Error>(
        &mut txn,
        linkage_analysis,
        package_store,
    )?;

    Ok(txn)
}
