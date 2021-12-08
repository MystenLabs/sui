// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

//! This module contains the public APIs supported by the bytecode verifier.

use fastpay_core::error::FastPayResult;
use move_binary_format::file_format::CompiledModule;

use crate::dummy_verifier;

/// Helper for a "canonical" verification of a module.
pub fn verify_module(module: &CompiledModule) -> FastPayResult {
    dummy_verifier::verify_module(module)
}
