// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use sui_move_fuzzer::authority_harness::FUZZ_AUTHORITY;
use sui_move_fuzzer::crash_validator::validate_crash;
use sui_move_fuzzer::module_gen::{ModuleGenConfig, ModuleBuilder};
use sui_move_fuzzer::mutators::{MutationKind, apply_mutation};
use sui_move_fuzzer::oracle::{BugClass, check_crash, check_effects_for_bugs};

#[derive(Debug, Arbitrary)]
struct E2eInput {
    config: ModuleGenConfig,
    mutation: Option<MutationKind>,
    raw_entropy: Vec<u8>,
}

fuzz_target!(|input: E2eInput| {
    let mut u = arbitrary::Unstructured::new(&input.raw_entropy);

    // Step 1: Generate a CompiledModule from the grammar-based builder.
    let builder = ModuleBuilder::new(input.config);
    let mut module = match builder.build(&mut u) {
        Ok(m) => m,
        Err(_) => return,
    };

    // Step 2: Optionally apply a targeted mutation.
    if let Some(_kind) = input.mutation {
        let _ = apply_mutation(&mut u, &mut module);
    }

    // Step 3: Serialize the module to bytes.
    let mut bytes = Vec::new();
    if module.serialize(&mut bytes).is_err() {
        return;
    }

    // Step 4: Publish through the real authority, catching any panics.
    let publish_result = check_crash("e2e_publish", || {
        FUZZ_AUTHORITY.publish_module(bytes.clone())
    });

    match publish_result {
        Err(bug @ BugClass::ValidatorCrash { .. }) => {
            // A panic in publish -- validate it is a real bug, not a fuzzer artifact.
            if validate_crash(&module, &bug) {
                panic!("Confirmed validator crash during publish: {:?}", bug);
            }
            return;
        }
        Err(_) => return,
        Ok(Err(_)) => {
            // Publish returned an error (e.g., verification rejected) -- expected for
            // mutated modules. Not a bug.
            return;
        }
        Ok(Ok((package_id, effects))) => {
            // Step 5: Check effects for invariant violations.
            if let Some(bug) = check_effects_for_bugs(&effects) {
                panic!("Verifier soundness bug after publish: {:?}", bug);
            }

            // Step 6: Try calling the first entry function if one exists.
            let has_entry = module.function_defs.iter().any(|f| f.is_entry);
            if has_entry {
                let call_result = check_crash("e2e_call", || {
                    FUZZ_AUTHORITY.call_entry_function(package_id, "fuzz_mod", "f0")
                });
                match call_result {
                    Err(bug @ BugClass::ValidatorCrash { .. }) => {
                        if validate_crash(&module, &bug) {
                            panic!("Confirmed validator crash during call: {:?}", bug);
                        }
                    }
                    Ok(Ok(call_effects)) => {
                        if let Some(bug) = check_effects_for_bugs(&call_effects) {
                            panic!("Verifier soundness bug after call: {:?}", bug);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
});
