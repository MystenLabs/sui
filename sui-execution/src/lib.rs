// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

pub use executor::Executor;
use sui_protocol_config::ProtocolConfig;
use sui_types::error::SuiError;

pub mod executor;
pub mod latest;

pub fn executor(
    protocol_config: &ProtocolConfig,
    paranoid_type_checks: bool,
    silent: bool,
) -> Result<Arc<dyn Executor + Send + Sync>, SuiError> {
    Ok(Arc::new(latest::VM::new(
        protocol_config,
        paranoid_type_checks,
        silent,
    )?))
}
