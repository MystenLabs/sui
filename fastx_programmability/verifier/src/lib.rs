// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

pub mod verifier;

pub mod global_storage_access_verifier;
pub mod id_immutable_verifier;
pub mod id_leak_verifier;
pub mod struct_with_key_verifier;

use fastx_types::error::FastPayError;

fn verification_failure(error: String) -> FastPayError {
    FastPayError::ModuleVerificationFailure { error }
}
