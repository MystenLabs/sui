// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module contains the public APIs supported by the bytecode verifier.

use move_binary_format::file_format::CompiledModule;
use sui_types::error::SuiResult;

use crate::{
    global_storage_access_verifier, id_immutable_verifier, id_leak_verifier,
    param_typecheck_verifier, struct_with_key_verifier,
};

/// Helper for a "canonical" verification of a module.
pub fn verify_module(module: &CompiledModule) -> SuiResult {
    struct_with_key_verifier::verify_module(module)?;
    global_storage_access_verifier::verify_module(module)?;
    id_immutable_verifier::verify_module(module)?;
    id_leak_verifier::verify_module(module)?;
    param_typecheck_verifier::verify_module(module)
}
