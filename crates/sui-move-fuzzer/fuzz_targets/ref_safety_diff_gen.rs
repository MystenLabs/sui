// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![no_main]

//! Grammar-based differential oracle for reference safety.
//!
//! Generates valid-by-construction modules via `ModuleBuilder` and checks that
//! graph-based and regex-based reference safety agree. Complements the raw-binary
//! `ref_safety_diff` target: this one explores valid module structures with complex
//! reference patterns, while `ref_safety_diff` explores unusual binary patterns
//! via the custom mutator.

use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use move_bytecode_verifier::verify_module_with_config_metered;
use move_bytecode_verifier_meter::dummy::DummyMeter;
use move_vm_config::verifier::VerifierConfig;
use sui_move_fuzzer::module_gen::{ModuleBuilder, ModuleGenConfig};

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

    let graph_config = VerifierConfig {
        deprecate_global_storage_ops: true,
        switch_to_regex_reference_safety: false,
        sanity_check_with_regex_reference_safety: None,
        ..Default::default()
    };

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
