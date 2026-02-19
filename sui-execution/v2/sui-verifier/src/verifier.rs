// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module contains the public APIs supported by the bytecode verifier.

use move_binary_format::file_format::CompiledModule;
use move_vm_config::verifier::VerifierConfig;
use sui_types::{error::ExecutionError, move_package::FnInfoMap};

use crate::{
    entry_points_verifier, global_storage_access_verifier, id_leak_verifier,
    one_time_witness_verifier, private_generics, struct_with_key_verifier,
};
use move_bytecode_verifier_meter::dummy::DummyMeter;
use move_bytecode_verifier_meter::Meter;

/// Helper for a "canonical" verification of a module.
pub fn sui_verify_module_metered(
    module: &CompiledModule,
    fn_info_map: &FnInfoMap,
    meter: &mut (impl Meter + ?Sized),
    verifier_config: &VerifierConfig,
) -> Result<(), ExecutionError> {
    struct_with_key_verifier::verify_module(module)?;
    global_storage_access_verifier::verify_module(module)?;
    id_leak_verifier::verify_module(module, meter)?;
    private_generics::verify_module(module, verifier_config)?;
    entry_points_verifier::verify_module(module, fn_info_map, verifier_config)?;
    one_time_witness_verifier::verify_module(module, fn_info_map)
}

/// Runs the Sui verifier and checks if the error counts as a Sui verifier timeout
/// NOTE: this function only check if the verifier error is a timeout
/// All other errors are ignored
pub fn sui_verify_module_metered_check_timeout_only(
    module: &CompiledModule,
    fn_info_map: &FnInfoMap,
    meter: &mut (impl Meter + ?Sized),
    verifier_config: &VerifierConfig,
) -> Result<(), ExecutionError> {
    // Checks if the error counts as a Sui verifier timeout
    if let Err(error) = sui_verify_module_metered(module, fn_info_map, meter, verifier_config) {
        if matches!(
            error.kind(),
            sui_types::execution_status::ExecutionFailureStatus::SuiMoveVerificationTimedout
        ) {
            return Err(error);
        }
    }
    // Any other scenario, including a non-timeout error counts as Ok
    Ok(())
}

pub fn sui_verify_module_unmetered(
    module: &CompiledModule,
    fn_info_map: &FnInfoMap,
    verifier_config: &VerifierConfig,
) -> Result<(), ExecutionError> {
    sui_verify_module_metered(module, fn_info_map, &mut DummyMeter, verifier_config).inspect_err(
        |err| {
            // We must never see timeout error in execution
            debug_assert!(
                !matches!(
                err.kind(),
                sui_types::execution_status::ExecutionFailureStatus::SuiMoveVerificationTimedout
            ),
                "Unexpected timeout error in execution"
            );
        },
    )
}
