// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// $GENERATED_MESSAGE

use std::sync::Arc;

use sui_protocol_config::ProtocolConfig;
use sui_types::error::SuiResult;

pub use executor::Executor;

pub mod executor;

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
