// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![no_main]

use libfuzzer_sys::{fuzz_mutator, fuzz_target};
use move_binary_format::binary_config::BinaryConfig;
use move_binary_format::file_format::CompiledModule;
use move_bytecode_verifier::verify_module_with_config_metered;
use move_bytecode_verifier_meter::dummy::DummyMeter;
use move_vm_config::verifier::VerifierConfig;
use sui_move_fuzzer::custom_mutator::mutate_move_module;

fuzz_mutator!(|data: &mut [u8], size: usize, max_size: usize, seed: u32| {
    match mutate_move_module(data, size, max_size, seed) {
        Some(n) => n,
        None => libfuzzer_sys::fuzzer_mutate(data, size, max_size),
    }
});

fuzz_target!(|data: &[u8]| {
    let config = BinaryConfig::standard();
    let module = match CompiledModule::deserialize_with_config(data, &config) {
        Ok(m) => m,
        Err(_) => return,
    };

    // Graph-based reference safety (the traditional implementation)
    let graph_config = VerifierConfig {
        deprecate_global_storage_ops: true,
        switch_to_regex_reference_safety: false,
        sanity_check_with_regex_reference_safety: None,
        ..Default::default()
    };

    // Regex-based reference safety (the newer implementation)
    let regex_config = VerifierConfig {
        deprecate_global_storage_ops: true,
        switch_to_regex_reference_safety: true,
        sanity_check_with_regex_reference_safety: None,
        ..Default::default()
    };

    let graph_result = verify_module_with_config_metered(&graph_config, &module, &mut DummyMeter);
    let regex_result = verify_module_with_config_metered(&regex_config, &module, &mut DummyMeter);

    if graph_result.is_ok() && regex_result.is_err() {
        panic!(
            "reference safety inconsistency: graph accepts, regex rejects: {}",
            regex_result.unwrap_err()
        );
    }

    if regex_result.is_ok() && graph_result.is_err() {
        panic!(
            "reference safety inconsistency: regex accepts, graph rejects: {}",
            graph_result.unwrap_err()
        );
    }
});
