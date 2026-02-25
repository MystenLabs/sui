// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![no_main]

//! Roundtrip oracle on verified modules.
//!
//! Generates modules via `ModuleBuilder`, filters to those that pass full
//! Move + Sui verification, then checks that verified modules survive
//! serialize → deserialize → serialize idempotently. Any failure indicates the
//! verifier accepted something the serializer disagrees about.

use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use sui_move_fuzzer::module_gen::{ModuleBuilder, ModuleGenConfig};
use sui_move_fuzzer::sui_harness;

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);

    let config: ModuleGenConfig = match u.arbitrary() {
        Ok(c) => c,
        Err(_) => return,
    };

    let module = match ModuleBuilder::new(config).build(&mut u) {
        Ok(m) => m,
        Err(_) => return,
    };

    // Only test modules that pass full verification
    if sui_harness::run_full_verification(&module).is_err() {
        return;
    }

    // Verified modules must survive roundtrip
    if let Err(e) = sui_harness::roundtrip_check(&module) {
        panic!("Verified module fails roundtrip: {e}");
    }
});
