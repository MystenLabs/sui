// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use sui_move_fuzzer::module_gen::{ModuleBuilder, ModuleGenConfig};
use sui_move_fuzzer::mutators;
use sui_move_fuzzer::sui_harness::run_full_verification;

#[derive(Debug, Arbitrary)]
struct BoundsEscapeInput {
    config: ModuleGenConfig,
    num_corruptions: u8,
    raw_entropy: Vec<u8>,
}

fuzz_target!(|input: BoundsEscapeInput| {
    let num_corruptions = input.num_corruptions.clamp(1, 5) as usize;

    let mut u = arbitrary::Unstructured::new(&input.raw_entropy);

    let builder = ModuleBuilder::new(input.config);
    let mut module = match builder.build(&mut u) {
        Ok(m) => m,
        Err(_) => return,
    };

    // Apply N mutations; some will be BoundsCorrupt, others will be
    // different kinds -- all contribute to verifier stress testing.
    for _ in 0..num_corruptions {
        if mutators::apply_mutation(&mut u, &mut module).is_err() {
            break;
        }
    }

    // The full verification pipeline (Move + Sui) should never panic on any input.
    // Errors are expected and fine; panics indicate a bug.
    // libfuzzer's signal handler catches panics directly — no manual catch_unwind needed.
    run_full_verification(&module).ok();
});
