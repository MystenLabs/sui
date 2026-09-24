// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod analysis;
pub mod config;
pub(crate) mod facts;
pub(crate) mod raw_facts;
pub mod resolution;
pub mod resolved_linkage;
pub mod single_linkage;

use crate::{
    data_store::{
        VerifiedPackageStore, backing_package_metadata_store::BackingPackageMetadataStore,
    },
    execution_mode::{ExecutionMode, Normal},
    static_programmable_transactions::{
        linkage::{
            analysis::LinkageAnalyzer, raw_facts::linkage_facts_from_programmable_transaction,
        },
        loading::ast as loading,
    },
};
use std::collections::BTreeSet;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    error::{ExecutionError, SuiResult},
    storage::BackingPackageStore,
    transaction::{ProgrammableTransaction, UnifiedLinkageInformation},
};

pub fn collect_unification_information_for_signing(
    protocol_config: &ProtocolConfig,
    pt: &ProgrammableTransaction,
    backing_package_store: &dyn BackingPackageStore,
) -> SuiResult<UnifiedLinkageInformation> {
    let backing_package_metadata_store =
        BackingPackageMetadataStore::new(protocol_config, backing_package_store);
    let facts = linkage_facts_from_programmable_transaction(pt, &backing_package_metadata_store)?;
    let linkage_analyzer = LinkageAnalyzer::new::<Normal<ExecutionError>>(protocol_config)
        .map_err(|error| sui_types::error::SuiError::from(error.to_string()))?;

    let mut execution_original_ids = BTreeSet::new();
    let linkage = single_linkage::compute_unified_linkage::<ExecutionError, _>(
        facts,
        &linkage_analyzer,
        &backing_package_metadata_store,
        protocol_config,
        Some(&mut execution_original_ids),
    )
    .map_err(sui_types::error::SuiError::from)?;

    Ok(UnifiedLinkageInformation {
        execution_original_ids,
        resolved_packages: linkage
            .resolution_table
            .iter()
            .map(|(original_id, resolution)| {
                (*original_id, (resolution.object_id(), resolution.version()))
            })
            .collect(),
    })
}

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
