// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Lightweight verification-only harness for the Sui Move VM fuzzer.
//!
//! Runs the full Move + Sui verification pipeline without needing a full
//! authority instance. Useful for fast crash-finding fuzz targets.

use std::collections::BTreeMap;

use move_binary_format::binary_config::BinaryConfig;
use move_binary_format::file_format::CompiledModule;
use move_bytecode_verifier::verify_module_with_config_metered;
use move_bytecode_verifier_meter::dummy::DummyMeter;
use move_vm_config::verifier::VerifierConfig;

use sui_verifier::{
    entry_points_verifier, global_storage_access_verifier, id_leak_verifier,
    one_time_witness_verifier, struct_with_key_verifier,
};

use crate::oracle;

/// Returns a `VerifierConfig` matching Sui production settings.
pub fn sui_verifier_config() -> VerifierConfig {
    VerifierConfig {
        deprecate_global_storage_ops: true,
        private_generics_verifier_v2: true,
        sanity_check_with_regex_reference_safety: Some(8_000_000),
        bytecode_version: 7,
        max_loop_depth: Some(5),
        max_function_parameters: Some(128),
        max_generic_instantiation_length: Some(32),
        max_basic_blocks: Some(1024),
        max_value_stack_size: 1024,
        max_type_nodes: Some(256),
        max_push_size: Some(10000),
        max_dependency_depth: Some(100),
        max_data_definitions: Some(200),
        max_fields_in_struct: Some(32),
        max_function_definitions: Some(1000),
        ..VerifierConfig::default()
    }
}

/// Run the full Move + Sui verification pipeline. Returns `Ok(())` or an error string.
pub fn run_full_verification(module: &CompiledModule) -> Result<(), String> {
    let config = sui_verifier_config();
    let mut meter = DummyMeter;

    // Phase 1: Move bytecode verification
    verify_module_with_config_metered(&config, module, &mut meter)
        .map_err(|e| format!("Move verification failed: {:?}", e))?;

    // Phase 2: Sui-specific verification
    let fn_info_map = BTreeMap::new();
    sui_verifier::verifier::sui_verify_module_metered(
        module,
        &fn_info_map,
        &mut meter,
        &config,
    )
    .map_err(|e| format!("Sui verification failed: {}", e))
}

/// Run each Sui verifier pass individually with `catch_unwind`.
pub fn run_sui_passes_individually(
    module: &CompiledModule,
) -> Vec<(&'static str, Result<(), String>)> {
    let config = sui_verifier_config();
    let fn_info_map: BTreeMap<_, _> = BTreeMap::new();
    let mut results = Vec::new();

    // struct_with_key_verifier
    let r = oracle::check_crash("struct_with_key_verifier", || {
        struct_with_key_verifier::verify_module(module)
    });
    results.push((
        "struct_with_key_verifier",
        flatten_crash_result(r),
    ));

    // global_storage_access_verifier
    let r = oracle::check_crash("global_storage_access_verifier", || {
        global_storage_access_verifier::verify_module(module)
    });
    results.push((
        "global_storage_access_verifier",
        flatten_crash_result(r),
    ));

    // id_leak_verifier
    let r = oracle::check_crash("id_leak_verifier", || {
        id_leak_verifier::verify_module(module, &mut DummyMeter)
    });
    results.push(("id_leak_verifier", flatten_crash_result(r)));

    // entry_points_verifier
    let config_clone = config.clone();
    let fn_info_clone = fn_info_map.clone();
    let r = oracle::check_crash("entry_points_verifier", || {
        entry_points_verifier::verify_module(module, &fn_info_clone, &config_clone)
    });
    results.push(("entry_points_verifier", flatten_crash_result(r)));

    // one_time_witness_verifier
    let r = oracle::check_crash("one_time_witness_verifier", || {
        one_time_witness_verifier::verify_module(module, &fn_info_map)
    });
    results.push(("one_time_witness_verifier", flatten_crash_result(r)));

    results
}

/// Serialize -> deserialize -> re-serialize -> compare bytes.
pub fn roundtrip_check(module: &CompiledModule) -> Result<(), String> {
    let mut bytes1 = Vec::new();
    module
        .serialize(&mut bytes1)
        .map_err(|e| format!("Initial serialization failed: {:?}", e))?;

    let config = BinaryConfig::standard();
    let module2 = CompiledModule::deserialize_with_config(&bytes1, &config)
        .map_err(|e| format!("Deserialization failed: {:?}", e))?;

    let mut bytes2 = Vec::new();
    module2
        .serialize(&mut bytes2)
        .map_err(|e| format!("Re-serialization failed: {:?}", e))?;

    if bytes1 != bytes2 {
        return Err(format!(
            "Roundtrip mismatch: original {} bytes vs re-serialized {} bytes",
            bytes1.len(),
            bytes2.len()
        ));
    }

    Ok(())
}

/// Convert a `Result<Result<(), impl Display>, BugClass>` from `check_crash` into a flat
/// `Result<(), String>`, propagating panics as `Err`.
fn flatten_crash_result<E: std::fmt::Display>(
    result: Result<Result<(), E>, oracle::BugClass>,
) -> Result<(), String> {
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(format!("{}", e)),
        Err(oracle::BugClass::ValidatorCrash { pass, message }) => {
            Err(format!("CRASH in {}: {}", pass, message))
        }
        Err(bug) => Err(format!("{:?}", bug)),
    }
}
