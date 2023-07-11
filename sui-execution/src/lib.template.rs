// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// $GENERATED_MESSAGE

use std::sync::Arc;

use sui_protocol_config::ProtocolConfig;
use sui_types::{error::SuiResult, metrics::BytecodeVerifierMetrics};

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
    paranoid_type_checks: bool,
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
    is_metered: bool,
    metrics: &'m Arc<BytecodeVerifierMetrics>,
) -> Box<dyn Verifier + 'm> {
    let version = protocol_config.execution_version_as_option().unwrap_or(0);
    match version {
        // $VERIFIER_CUTS
        v => panic!("Unsupported execution version {v}"),
    }
}
