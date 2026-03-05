// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Custom native functions support for the latest execution version.
//!
//! This module is only available when the `custom-natives` feature is enabled.

use std::sync::Arc;

use sui_protocol_config::ProtocolConfig;
use sui_types::error::SuiResult;

use crate::Executor;
use crate::latest;

pub use move_vm_runtime_latest::native_functions::{NativeFunction, NativeFunctionTable};
pub use sui_move_natives_latest::{all_natives, make_native};

/// Create an executor with custom native functions for the latest execution version (3).
///
/// This is useful for e.g. research projects that use the `sui-execution` crate but require
/// additional or different native functions. Only protocol configs with execution version 3
/// are supported; for earlier versions this function panics.
pub fn latest_executor_with_custom_natives(
    protocol_config: &ProtocolConfig,
    silent: bool,
    native_functions: NativeFunctionTable,
) -> SuiResult<Arc<dyn Executor + Send + Sync>> {
    let version = protocol_config.execution_version_as_option().unwrap_or(0);
    Ok(match version {
        0..=2 => {
            panic!("Custom native functions are not supported for execution versions before 3")
        }
        3 => Arc::new(latest::Executor::new_with_custom_natives(
            protocol_config,
            silent,
            native_functions,
        )?),
        v => panic!("Unsupported execution version {v}"),
    })
}
