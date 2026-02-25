// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Double-check crash reproduction.
//!
//! Re-runs a failing input to confirm it is a real bug rather than a
//! fuzzer artifact (e.g., corrupted memory during mutation).

use std::panic::{catch_unwind, AssertUnwindSafe};

use move_binary_format::file_format::CompiledModule;
use move_bytecode_verifier_meter::dummy::DummyMeter;

use crate::oracle::BugClass;
use crate::sui_harness::sui_verifier_config;

/// Re-run a failing input to confirm it is a real bug, not a fuzzer artifact.
///
/// Returns `true` if the crash reproduces (confirmed bug), `false` otherwise.
pub fn validate_crash(module: &CompiledModule, bug: &BugClass) -> bool {
    // Step 1: Re-serialize the module. If serialization fails, the module bytes
    // were likely corrupted by the fuzzer — not a real bug.
    let mut bytes = Vec::new();
    if module.serialize(&mut bytes).is_err() {
        return false;
    }

    // Step 2: Determine which pass crashed and re-run it.
    let pass_name = match bug {
        BugClass::ValidatorCrash { pass, .. } => pass.as_str(),
        _ => return false,
    };

    let config = sui_verifier_config();
    let fn_info_map = std::collections::BTreeMap::new();

    catch_unwind(AssertUnwindSafe(|| {
        run_pass(pass_name, module, &config, &fn_info_map);
    }))
    .is_err()
}

/// Dispatch to the specific verifier pass by name.
fn run_pass(
    pass_name: &str,
    module: &CompiledModule,
    config: &move_vm_config::verifier::VerifierConfig,
    fn_info_map: &sui_types::move_package::FnInfoMap,
) {
    match pass_name {
        "struct_with_key_verifier" => {
            let _ = sui_verifier::struct_with_key_verifier::verify_module(module);
        }
        "global_storage_access_verifier" => {
            let _ = sui_verifier::global_storage_access_verifier::verify_module(module);
        }
        "id_leak_verifier" => {
            let _ = sui_verifier::id_leak_verifier::verify_module(module, &mut DummyMeter);
        }
        "entry_points_verifier" => {
            let _ = sui_verifier::entry_points_verifier::verify_module(module, fn_info_map, config);
        }
        "one_time_witness_verifier" => {
            let _ = sui_verifier::one_time_witness_verifier::verify_module(module, fn_info_map);
        }
        "move_bytecode_verifier" => {
            let _ = move_bytecode_verifier::verify_module_with_config_metered(
                config,
                module,
                &mut DummyMeter,
            );
        }
        "sui_verify_module" => {
            let _ = sui_verifier::verifier::sui_verify_module_metered(
                module,
                fn_info_map,
                &mut DummyMeter,
                config,
            );
        }
        _ => {
            // Unknown pass name — run the full pipeline as a best effort.
            let _ = move_bytecode_verifier::verify_module_with_config_metered(
                config,
                module,
                &mut DummyMeter,
            );
            let _ = sui_verifier::verifier::sui_verify_module_metered(
                module,
                fn_info_map,
                &mut DummyMeter,
                config,
            );
        }
    }
}
