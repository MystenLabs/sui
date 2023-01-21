// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;

/// TEMPORARY: Detect if we should update to a new framework before epoch `epoch` starts.  Returns
/// the modules to replace the Sui Framework with, if an upgrade is detected, or None otherwise.
pub fn override_sui_framework(epoch: u64) -> Option<Vec<CompiledModule>> {
    match epoch {
        5 => Some(sui_framework_override::get_sui_framework()),
        _ => None,
    }
}
