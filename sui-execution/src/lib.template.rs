// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// $GENERATED_MESSAGE

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use move_core_types::account_address::AccountAddress;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    error::SuiResult,
    metrics::BytecodeVerifierMetrics,
    storage::BackingPackageStore,
    transaction::ProgrammableTransaction,
};

pub use executor::Executor;
pub use verifier::Verifier;

pub mod executor;
pub mod verifier;

// $MOD_CUTS

#[cfg(test)]
mod tests;

// $FEATURE_CONSTS
pub fn executor(
    protocol_config: &ProtocolConfig,
    silent: bool,
) -> SuiResult<Arc<dyn Executor + Send + Sync>> {
    let version = protocol_config.execution_version_as_option().unwrap_or(0);
    Ok(match version {
        // $EXECUTOR_CUTS
        v => panic!("Unsupported execution version {v}"),
    })
}

pub fn verifier<'m>(
    protocol_config: &ProtocolConfig,
    signing_limits: Option<(usize, usize, usize)>,
    metrics: &'m Arc<BytecodeVerifierMetrics>,
) -> Box<dyn Verifier + 'm> {
    let version = protocol_config.execution_version_as_option().unwrap_or(0);
    let config = protocol_config.verifier_config(signing_limits);
    match version {
        // $VERIFIER_CUTS
        v => panic!("Unsupported execution version {v}"),
    }
}

pub fn collect_unification_information_for_signing(
    protocol_config: &ProtocolConfig,
    pt: &ProgrammableTransaction,
    package_store: &dyn BackingPackageStore,
) -> SuiResult<(BTreeSet<AccountAddress>, BTreeMap<AccountAddress, AccountAddress>)> {
    if !protocol_config.enable_unified_linkage() {
        return Ok((BTreeSet::new(), BTreeMap::new()));
    }

    let version = protocol_config.execution_version_as_option().unwrap_or(0);
    match version {
        // $COLLECT_UNIFICATION_INFORMATION_FOR_SIGNING_CUTS
        v => panic!("Unsupported execution version {v}"),
    }
}
