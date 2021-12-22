// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

//! This module contains the public APIs supported by the bytecode verifier.

use fastx_types::error::FastPayResult;
use move_binary_format::file_format::CompiledModule;

use crate::{global_storage_access_verifier, id_leak_verifier, struct_with_key_verifier};

/// Helper for a "canonical" verification of a module.
pub fn verify_module(module: &CompiledModule) -> FastPayResult {
    struct_with_key_verifier::verify_module(module)?;
    global_storage_access_verifier::verify_module(module)?;
    id_leak_verifier::verify_module(module)
}
