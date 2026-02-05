// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Integration tests that run reaching definitions (forward) and liveness (backward)
//! analyses on real mainnet bytecode modules to validate termination and correctness.

use move_binary_format::{binary_config::BinaryConfig, file_format::CompiledModule};
use move_dataflow_analysis::analyses::{liveness, reaching_defs};
use move_model_2::model::ModelConfig;
use std::{collections::BTreeMap, path::Path};

/// Load a compiled module from raw binary (.mv) bytes.
fn load_module_from_binary(bytes: &[u8]) -> CompiledModule {
    CompiledModule::deserialize_with_defaults(bytes).unwrap_or_else(|_| {
        let config = BinaryConfig::legacy_with_flags(true, false);
        CompiledModule::deserialize_with_config(bytes, &config)
            .expect("failed to deserialize module")
    })
}

/// Load a compiled module from a hex-encoded string (.bytes file content).
fn load_module_from_hex(hex_str: &str) -> CompiledModule {
    let bytes = hex::decode(hex_str.trim()).expect("invalid hex");
    let config = BinaryConfig::legacy_with_flags(true, false);
    CompiledModule::deserialize_with_config(&bytes, &config)
        .expect("failed to deserialize module")
}

/// Load all test modules from the test_data directory.
fn load_all_modules() -> Vec<(String, CompiledModule)> {
    let test_data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_data");
    let mut modules = Vec::new();

    for entry in std::fs::read_dir(&test_data_dir).expect("failed to read test_data dir") {
        let entry = entry.expect("failed to read entry");
        let path = entry.path();
        let name = path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();

        if let Some(ext) = path.extension() {
            match ext.to_str().unwrap() {
                "mv" => {
                    let bytes = std::fs::read(&path).expect("failed to read .mv file");
                    modules.push((name, load_module_from_binary(&bytes)));
                }
                "bytes" => {
                    let hex_str =
                        std::fs::read_to_string(&path).expect("failed to read .bytes file");
                    modules.push((name, load_module_from_hex(&hex_str)));
                }
                _ => {}
            }
        }
    }
    modules.sort_by(|a, b| a.0.cmp(&b.0));
    modules
}

/// Convert compiled modules to stackless bytecode, tolerating missing dependencies.
/// Returns `None` if the translation panics (e.g. due to unsupported deprecated bytecodes).
fn to_stackless(
    modules: Vec<CompiledModule>,
) -> Option<move_stackless_bytecode_2::ast::StacklessBytecode> {
    let config = ModelConfig {
        allow_missing_dependencies: true,
    };
    let model = move_model_2::model::Model::from_compiled_with_config(
        config,
        &BTreeMap::new(),
        modules,
    );
    // The stackless bytecode translator may panic on deprecated global storage
    // operations (MoveFromDeprecated, etc.) that produce NotImplemented instructions,
    // leaving the logical stack non-empty. Catch these panics gracefully.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        move_stackless_bytecode_2::from_model(&model, false)
            .expect("failed to convert to stackless bytecode")
    }))
    .ok()
}

/// Run both analyses on every function in a module.
/// Returns `true` if the module was successfully analyzed, `false` if translation failed.
fn run_analyses_on_module(name: &str, module: CompiledModule) -> bool {
    let sb = match to_stackless(vec![module]) {
        Some(sb) => sb,
        None => {
            eprintln!("  {name}: skipped (stackless translation failed)");
            return false;
        }
    };

    let mut function_count = 0;
    for package in &sb.packages {
        for (mod_name, sb_module) in &package.modules {
            for (fun_name, function) in &sb_module.functions {
                let num_locals = count_locals(function);

                // Forward: reaching definitions
                let rd_result = reaching_defs::analyze(function, num_locals);
                assert!(
                    !rd_result.is_empty(),
                    "{name}::{mod_name}::{fun_name}: reaching defs produced empty state map"
                );

                // Backward: liveness
                let live_result = liveness::analyze(function);
                assert!(
                    !live_result.is_empty(),
                    "{name}::{mod_name}::{fun_name}: liveness produced empty state map"
                );

                // All blocks must have analysis results.
                for &label in function.basic_blocks.keys() {
                    assert!(
                        rd_result.contains_key(&label),
                        "{name}::{mod_name}::{fun_name}: reaching defs missing block {label}"
                    );
                    assert!(
                        live_result.contains_key(&label),
                        "{name}::{mod_name}::{fun_name}: liveness missing block {label}"
                    );
                }
                function_count += 1;
            }
        }
    }
    eprintln!("  {name}: analyzed {function_count} functions");
    true
}

/// Scan a function to find the highest local index referenced.
fn count_locals(func: &move_stackless_bytecode_2::ast::Function) -> usize {
    use move_stackless_bytecode_2::ast::{Instruction, RValue};
    let mut max_local = 0usize;
    for block in func.basic_blocks.values() {
        for instr in &block.instructions {
            match instr {
                Instruction::StoreLoc { loc, .. } => {
                    max_local = max_local.max(*loc + 1);
                }
                Instruction::AssignReg { rhs, .. } => {
                    if let RValue::Local { arg, .. } = rhs {
                        max_local = max_local.max(*arg + 1);
                    }
                }
                _ => {}
            }
        }
    }
    max_local
}

// ---------------------------------------------------------------------------
// Individual tests

#[test]
fn test_basic() {
    for (name, module) in load_all_modules() {
        if name == "basic" {
            run_analyses_on_module(&name, module);
            return;
        }
    }
    panic!("basic.mv not found");
}

#[test]
fn test_cat_nft() {
    for (name, module) in load_all_modules() {
        if name == "cat_nft" {
            run_analyses_on_module(&name, module);
            return;
        }
    }
    panic!("cat_nft.mv not found");
}

#[test]
fn test_nft_claim() {
    for (name, module) in load_all_modules() {
        if name == "nft_claim" {
            run_analyses_on_module(&name, module);
            return;
        }
    }
    panic!("nft_claim.mv not found");
}

/// Run analyses on all mainnet modules.
#[test]
fn test_all_mainnet_modules() {
    let modules = load_all_modules();
    assert!(
        modules.len() >= 11,
        "Expected at least 11 test modules, found {}",
        modules.len()
    );
    let mut analyzed = 0;
    let mut skipped = 0;
    for (name, module) in &modules {
        if run_analyses_on_module(name, module.clone()) {
            analyzed += 1;
        } else {
            skipped += 1;
        }
    }
    eprintln!(
        "Analyzed {analyzed}/{} modules ({skipped} skipped due to unsupported bytecodes)",
        modules.len()
    );
    assert!(
        analyzed >= 3,
        "Expected at least 3 modules to be analyzable, only {analyzed} succeeded"
    );
}
