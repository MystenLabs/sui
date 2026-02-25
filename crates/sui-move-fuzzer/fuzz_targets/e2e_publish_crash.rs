// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![no_main]

use libfuzzer_sys::fuzz_target;

use move_binary_format::binary_config::BinaryConfig;
use move_binary_format::file_format::CompiledModule;

use sui_move_fuzzer::authority_harness::FUZZ_AUTHORITY;
use sui_move_fuzzer::mutators::{apply_bytes_mutation, apply_mutation, ensure_code_unit};
use sui_move_fuzzer::oracle::check_crash;

fuzz_target!(|data: &[u8]| {
    // Step 1: Try to deserialize raw bytes into a CompiledModule.
    let config = BinaryConfig::standard();
    let mut module = match CompiledModule::deserialize_with_config(data, &config) {
        Ok(m) => m,
        Err(_) => return,
    };

    // Ensure the module has at least one function with a code unit for mutations.
    ensure_code_unit(&mut module);

    // Step 2: Apply multiple rounds of targeted mutations (2-5 rounds).
    let entropy_len = data.len().min(512);
    let entropy = &data[..entropy_len];
    let mut u = arbitrary::Unstructured::new(entropy);

    let rounds = match u.int_in_range(2u8..=5) {
        Ok(r) => r,
        Err(_) => 3,
    };
    for _ in 0..rounds {
        let _ = apply_mutation(&mut u, &mut module);
    }

    // Step 3: Serialize back to bytes.
    let mut bytes = Vec::new();
    if module.serialize(&mut bytes).is_err() {
        return;
    }

    // Step 4: Optionally apply a raw bytes mutation (IntegerOverflowOffset).
    let _ = apply_bytes_mutation(&mut u, &mut bytes);

    // Step 5: Publish -- the pipeline should NEVER panic on any input.
    // Any panic here is a real bug: the validator must handle all inputs gracefully.
    let result = check_crash("e2e_publish_crash", || {
        FUZZ_AUTHORITY.publish_module(bytes)
    });

    if let Err(bug) = result {
        panic!("Validator crashed on mutated input: {:?}", bug);
    }
});
