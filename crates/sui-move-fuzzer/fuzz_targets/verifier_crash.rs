// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use move_bytecode_verifier::verify_module_with_config_metered;
use move_bytecode_verifier_meter::dummy::DummyMeter;

use sui_move_fuzzer::module_gen::{ModuleBuilder, ModuleGenConfig};
use sui_move_fuzzer::mutators::{MutationKind, apply_mutation};
use sui_move_fuzzer::sui_harness::{run_full_verification, run_sui_passes_individually,
                                    sui_verifier_config};

#[derive(Debug, Arbitrary)]
struct Input {
    config: ModuleGenConfig,
    mutation: Option<MutationKind>,
    raw_entropy: Vec<u8>,
}

fuzz_target!(|input: Input| {
    let mut u = arbitrary::Unstructured::new(&input.raw_entropy);

    let builder = ModuleBuilder::new(input.config);
    let mut module = match builder.build(&mut u) {
        Ok(m) => m,
        Err(_) => return,
    };

    if input.mutation.is_some() {
        let _ = apply_mutation(&mut u, &mut module);
    }

    // Strategy 1: Full pipeline crash detection.
    // Any panic in the full Move + Sui verification pipeline is a real bug.
    // This is the highest-value signal.
    run_full_verification(&module).ok();
    // If run_full_verification panics, libfuzzer catches the signal and saves the artifact.
    // No need for manual catch_unwind — libfuzzer's own signal handler is more reliable.

    // Strategy 2: Run individual Sui passes on modules that pass Move verification.
    // This finds Sui-specific panics that only trigger after Move's bounds/type checks pass.
    let config = sui_verifier_config();
    let move_ok = verify_module_with_config_metered(&config, &module, &mut DummyMeter).is_ok();

    if move_ok {
        // Module passed Move verification — now stress-test each Sui pass individually.
        // Panics here mean the Sui verifier crashes on a module the Move verifier accepted.
        let results = run_sui_passes_individually(&module);
        for (pass_name, result) in results {
            if let Err(ref err_msg) = result
                && err_msg.contains("CRASH")
            {
                panic!(
                    "Sui verifier crash on Move-valid module in {}: {}",
                    pass_name, err_msg
                );
            }
        }
    }
});
