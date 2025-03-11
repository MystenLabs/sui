// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// $GENERATED_MESSAGE

use std::path::PathBuf;
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
    silent: bool,
    enable_profiler: Option<PathBuf>,
) -> SuiResult<Arc<dyn Executor + Send + Sync>> {
    let version = protocol_config.execution_version_as_option().unwrap_or(0);
    Ok(match version {
        // $EXECUTOR_CUTS
        v => panic!("Unsupported execution version {v}"),
    })
}

pub fn verifier<'m>(
    protocol_config: &ProtocolConfig,
    signing_limits: Option<(usize, usize)>,
    metrics: &'m Arc<BytecodeVerifierMetrics>,
) -> Box<dyn Verifier + 'm> {
    let version = protocol_config.execution_version_as_option().unwrap_or(0);
    let config = protocol_config.verifier_config(signing_limits);
    match version {
        // $VERIFIER_CUTS
        v => panic!("Unsupported execution version {v}"),
    }
}

#[cfg(msim)]
/// Initialize the VM for MSIM -- MSIM doesn't like any global statics that are unititialized. This
/// function is called by the MSIM runtime to initialize the VM and any statics/globals in it.
pub fn init_vm_for_msim() {
    // $INIT_CUTS
}
