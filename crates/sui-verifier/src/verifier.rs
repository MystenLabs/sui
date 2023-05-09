// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module contains the public APIs supported by the bytecode verifier.

use move_binary_format::file_format::CompiledModule;
use sui_protocol_config::ProtocolConfig;
use sui_types::{error::ExecutionError, move_package::FnInfoMap};

use crate::{
    entry_points_verifier, global_storage_access_verifier, id_leak_verifier,
    one_time_witness_verifier, private_generics, struct_with_key_verifier,
};
use move_bytecode_verifier::meter::DummyMeter;
use move_bytecode_verifier::meter::Meter;

/// Helper for a "canonical" verification of a module.
pub fn sui_verify_module_metered(
    config: &ProtocolConfig,
    module: &CompiledModule,
    fn_info_map: &FnInfoMap,
    meter: &mut impl Meter,
) -> Result<(), ExecutionError> {
    struct_with_key_verifier::verify_module(module)?;
    global_storage_access_verifier::verify_module(module)?;
    id_leak_verifier::verify_module(module, meter)?;
    private_generics::verify_module(module)?;
    entry_points_verifier::verify_module(config, module, fn_info_map)?;
    one_time_witness_verifier::verify_module(module, fn_info_map)
}

pub fn sui_verify_module_unmetered(
    config: &ProtocolConfig,
    module: &CompiledModule,
    fn_info_map: &FnInfoMap,
) -> Result<(), ExecutionError> {
    sui_verify_module_metered(config, module, fn_info_map, &mut DummyMeter)
}
