// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

pub mod verifier;

mod struct_with_key_verifier;

use fastx_types::error::FastPayError;

fn verification_failure(error: String) -> FastPayError {
    FastPayError::ModuleVerificationFailure { error }
}
