// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![no_main]

use libfuzzer_sys::{fuzz_mutator, fuzz_target};
use move_binary_format::binary_config::BinaryConfig;
use move_binary_format::file_format::CompiledModule;
use sui_move_fuzzer::custom_mutator::mutate_move_module;

fuzz_mutator!(|data: &mut [u8], size: usize, max_size: usize, seed: u32| {
    match mutate_move_module(data, size, max_size, seed) {
        Some(n) => n,
        None => libfuzzer_sys::fuzzer_mutate(data, size, max_size),
    }
});

fuzz_target!(|data: &[u8]| {
    let config = BinaryConfig::standard();

    // Step 1: Try to deserialize raw bytes into a CompiledModule.
    let module = match CompiledModule::deserialize_with_config(data, &config) {
        Ok(m) => m,
        Err(_) => return, // Invalid binary is expected; not a bug.
    };

    // Step 2: Serialize back.
    let mut bytes1 = Vec::new();
    if module.serialize(&mut bytes1).is_err() {
        panic!("Module deserializes but fails to re-serialize");
    }

    // Step 3: Deserialize from re-serialized bytes.
    let module2 = match CompiledModule::deserialize_with_config(&bytes1, &config) {
        Ok(m) => m,
        Err(_) => panic!("Re-serialized bytes fail to deserialize"),
    };

    // Step 4: Serialize the second module and compare bytes.
    let mut bytes2 = Vec::new();
    if module2.serialize(&mut bytes2).is_err() {
        panic!("Second re-serialization failed");
    }

    if bytes1 != bytes2 {
        panic!(
            "Non-idempotent serialization roundtrip: first {} bytes vs second {} bytes",
            bytes1.len(),
            bytes2.len()
        );
    }
});
