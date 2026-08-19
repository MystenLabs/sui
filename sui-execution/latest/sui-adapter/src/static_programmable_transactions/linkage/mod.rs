// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod analysis;
pub mod config;
pub mod resolution;
pub mod resolved_linkage;
pub mod single_linkage;

use crate::{
    data_store::VerifiedPackageStore,
    execution_mode::ExecutionMode,
    static_programmable_transactions::{
        linkage::analysis::LinkageAnalyzer, loading::ast as loading,
    },
};
use sui_protocol_config::ProtocolConfig;

/// Refine the transaction's per-call linkages into a single, unified linkage for the whole
/// transaction (when enabled by the protocol config).
pub fn refine_linkage<Mode: ExecutionMode>(
    mut txn: loading::Transaction,
    linkage_analysis: &LinkageAnalyzer,
    package_store: &VerifiedPackageStore<'_>,
    protocol_config: &ProtocolConfig,
) -> Result<loading::Transaction, Mode::Error> {
    if !protocol_config.enable_unified_linkage() {
        return Ok(txn);
    }

    single_linkage::refine_to_single_linkage::<Mode::Error>(
        &mut txn,
        linkage_analysis,
        package_store,
        protocol_config,
    )?;

    Ok(txn)
}
