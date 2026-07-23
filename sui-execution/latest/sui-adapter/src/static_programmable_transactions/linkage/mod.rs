// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod analysis;
pub(crate) mod backing_package_metadata_store;
pub mod config;
pub(crate) mod facts;
pub(crate) mod raw_facts;
pub mod resolution;
pub mod resolved_linkage;
pub mod single_linkage;

use crate::{
    data_store::VerifiedPackageStore,
    execution_mode::{ExecutionMode, Normal},
    static_programmable_transactions::{
        linkage::{
            analysis::LinkageAnalyzer, backing_package_metadata_store::BackingPackageMetadataStore,
            raw_facts::linkage_facts_from_programmable_transaction,
        },
        loading::ast as loading,
    },
};
use move_core_types::account_address::AccountAddress;
use std::collections::{BTreeMap, BTreeSet};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    error::{ExecutionError, SuiResult},
    storage::BackingPackageStore,
    transaction::ProgrammableTransaction,
};

pub fn collect_unification_information_for_signing(
    protocol_config: &ProtocolConfig,
    pt: &ProgrammableTransaction,
    backing_package_store: &dyn BackingPackageStore,
) -> SuiResult<(
    BTreeSet<AccountAddress>,
    BTreeMap<AccountAddress, AccountAddress>,
)> {
    let backing_package_metadata_store =
        BackingPackageMetadataStore::new(protocol_config, backing_package_store);
    let facts = linkage_facts_from_programmable_transaction(pt, &backing_package_metadata_store)?;
    let linkage_analyzer = LinkageAnalyzer::new::<Normal<ExecutionError>>(protocol_config)
        .map_err(|error| sui_types::error::SuiError::from(error.to_string()))?;

    let mut non_type_original_ids = BTreeSet::new();
    let linkage = single_linkage::compute_unified_linkage_from_facts::<ExecutionError, _>(
        facts,
        &linkage_analyzer,
        &backing_package_metadata_store,
        protocol_config,
        Some(&mut non_type_original_ids),
    )
    .map_err(sui_types::error::SuiError::from)?;

    Ok((
        non_type_original_ids.into_iter().map(Into::into).collect(),
        linkage
            .0
            .linkage
            .iter()
            .map(|(original_id, version_id)| ((*original_id).into(), **version_id))
            .collect(),
    ))
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
