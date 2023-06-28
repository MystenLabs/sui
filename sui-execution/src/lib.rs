// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_protocol_config::ProtocolConfig;
use sui_types::{error::SuiResult, metrics::BytecodeVerifierMetrics};

pub use executor::Executor;
pub use verifier::Verifier;

pub mod executor;
pub mod verifier;

mod latest;
mod v0;

pub const MIN_EXECUTION_VERSION: u64 = 0;
pub const LATEST_EXECUTION_VERSION: u64 = 1;

pub fn executor(
    protocol_config: &ProtocolConfig,
    paranoid_type_checks: bool,
    silent: bool,
) -> SuiResult<Arc<dyn Executor + Send + Sync>> {
    executor_impl(None, protocol_config, paranoid_type_checks, silent)
}

fn executor_impl(
    executor_version_override: Option<u64>,
    protocol_config: &ProtocolConfig,
    paranoid_type_checks: bool,
    silent: bool,
) -> SuiResult<Arc<dyn Executor + Send + Sync>> {
    let version = executor_version_override.unwrap_or(
        protocol_config
            .execution_version_as_option()
            .unwrap_or(MIN_EXECUTION_VERSION),
    );

    Ok(match version {
        0 => Arc::new(v0::Executor::new(
            protocol_config,
            paranoid_type_checks,
            silent,
        )?),

        LATEST_EXECUTION_VERSION => Arc::new(latest::Executor::new(
            protocol_config,
            paranoid_type_checks,
            silent,
        )?),

        v => panic!("Unsupported execution version {v}"),
    })
}

pub fn verifier<'m>(
    protocol_config: &ProtocolConfig,
    is_metered: bool,
    metrics: &'m Arc<BytecodeVerifierMetrics>,
) -> Box<dyn Verifier + 'm> {
    let version = protocol_config
        .execution_version_as_option()
        .unwrap_or(MIN_EXECUTION_VERSION);
    match version {
        0 => Box::new(v0::Verifier::new(protocol_config, is_metered, metrics)),
        LATEST_EXECUTION_VERSION => {
            Box::new(latest::Verifier::new(protocol_config, is_metered, metrics))
        }
        v => panic!("Unsupported execution version {v}"),
    }
}

/// Sometimes we want to invoke specific versions of the executor regardless of the protocol version
/// for debugging purposes.
#[cfg(debug_assertions)]
pub fn executor_for_version_debug_only(
    executor_version: u64,
    protocol_config: &ProtocolConfig,
    paranoid_type_checks: bool,
    silent: bool,
) -> SuiResult<Arc<dyn Executor + Send + Sync>> {
    executor_impl(
        Some(executor_version),
        protocol_config,
        paranoid_type_checks,
        silent,
    )
}
