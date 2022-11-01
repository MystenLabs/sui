// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module contains the public APIs supported by the bytecode verifier.

use move_binary_format::{access::ModuleAccess, file_format::CompiledModule};
use move_core_types::identifier::IdentStr;
use sui_types::{error::ExecutionError, SUI_FRAMEWORK_ADDRESS};

use crate::{
    entry_points_verifier, global_storage_access_verifier, id_leak_verifier,
    one_time_witness_verifier, private_generics, struct_with_key_verifier,
};

const TEST_SCENARIO_MODULE_NAME: &str = "test_scenario";

/// Helper for a "canonical" verification of a module.
pub fn verify_module(module: &CompiledModule) -> Result<(), ExecutionError> {
    if *module.address() == SUI_FRAMEWORK_ADDRESS
        && module.name() == IdentStr::new(TEST_SCENARIO_MODULE_NAME).unwrap()
    {
        // exclude test_module which is a test-only module in the Sui framework which "emulates"
        // transactional execution and in the process does things that do not quite agree with the
        // Sui verifier
        return Ok(());
    }

    struct_with_key_verifier::verify_module(module)?;
    global_storage_access_verifier::verify_module(module)?;
    id_leak_verifier::verify_module(module)?;
    private_generics::verify_module(module)?;
    entry_points_verifier::verify_module(module)?;
    one_time_witness_verifier::verify_module(module)
}
