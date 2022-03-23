// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod verifier;

pub mod entry_function_param_verifier;
pub mod global_storage_access_verifier;
pub mod id_immutable_verifier;
pub mod id_leak_verifier;
pub mod struct_with_key_verifier;

use sui_types::error::SuiError;

fn verification_failure(error: String) -> SuiError {
    SuiError::ModuleVerificationFailure { error }
}
