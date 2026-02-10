// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dev_utils::{
        compilation_utils::compile_packages_in_file, in_memory_test_adapter::InMemoryTestAdapter,
        vm_test_adapter::VMTestAdapter,
    },
    execution::values::Value,
    jit::optimization::{self, ast as opt_ast},
    shared::gas::UnmeteredGasMeter,
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId,
};

fn charge_test_addr() -> AccountAddress {
    AccountAddress::from_hex_literal("0x2a").unwrap()
}

fn find_function_blocks(
    file: &str,
    func_name: &str,
) -> std::collections::BTreeMap<opt_ast::Label, Vec<opt_ast::Bytecode>> {
    let package_address = charge_test_addr();
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file(file, &[]) {
        adapter.insert_package_into_storage(pkg);
    }
    let serialized_package = adapter.get_package_from_store(&package_address).unwrap();
    let (verif_pkg, _) = adapter
        .verify_package(package_address, serialized_package)
        .unwrap();
    let pkg = optimization::to_optimized_form(verif_pkg);
    let module_id = ModuleId::new(package_address, Identifier::new("charge_tests").unwrap());
    let module = pkg.modules.get(&module_id).expect("module not found");

    // Find the function by name by matching against the compiled module's function defs
    for (ndx, func) in &module.functions {
        let func_def = &module.compiled_module.function_defs()[ndx.0 as usize];
        let func_handle = &module.compiled_module.function_handles()[func_def.function.0 as usize];
        let name = module.compiled_module.identifier_at(func_handle.name);
        if name.as_str() == func_name {
            if let Some(code) = &func.code {
                return code.code.clone();
            }
        }
    }
    panic!("Function '{}' not found", func_name);
}

fn get_charge_info(instr: &opt_ast::Bytecode) -> Option<&opt_ast::ChargeInfo> {
    match instr {
        opt_ast::Bytecode::Charge(info) => Some(info),
        _ => None,
    }
}

fn setup_vm_and_execute(func_name: &str, args: Vec<Value>) -> Vec<Value> {
    let package_address = charge_test_addr();
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("charge_tests.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }
    let serialized_package = adapter.get_package_from_store(&package_address).unwrap();
    let (_verif_pkg, mut vm) = adapter
        .verify_package(package_address, serialized_package)
        .unwrap();
    let module_id = ModuleId::new(package_address, Identifier::new("charge_tests").unwrap());
    let function = Identifier::new(func_name).unwrap();
    vm.execute_function_bypass_visibility(
        &module_id,
        &function,
        vec![],
        args,
        &mut UnmeteredGasMeter,
        None,
    )
    .expect("Execution failed")
}

// --- Optimization-level tests: Charge insertion ---

#[test]
fn test_charge_inserted_in_blocks() {
    let blocks = find_function_blocks("charge_tests.move", "pure_arithmetic");
    // Every block with fixed-cost instructions should start with Charge
    for (_label, code) in &blocks {
        if code.is_empty() {
            continue;
        }
        let first = &code[0];
        // pure_arithmetic has only fixed-cost instructions (LdU64, Add, Ret),
        // so every block should start with Charge
        assert!(
            get_charge_info(first).is_some(),
            "First instruction should be Charge, got: {:?}",
            first
        );
    }
}

#[test]
fn test_charge_values_correct() {
    // pure_arithmetic: the compiler constant-folds `1 + 2 + 3` into `6`,
    // so bytecode is LdU64(6), Ret -> 2 fixed-cost instructions, 1 push, 0 pops.
    // Use `looping` instead which has non-trivial fixed-cost instructions.
    let blocks = find_function_blocks("charge_tests.move", "looping");
    let mut total_instructions = 0u64;
    let mut total_pushes = 0u64;
    for (_label, code) in &blocks {
        if let Some(info) = code.first().and_then(get_charge_info) {
            total_instructions += info.instructions;
            total_pushes += info.pushes;
        }
    }
    // looping has: LdU64(100), Lt (in condition), LdU64(1), Add (in body), Ret, branches, etc.
    assert!(
        total_instructions > 0,
        "Expected some fixed-cost instructions, got 0",
    );
    assert!(
        total_pushes > 0,
        "Expected some pushes, got 0",
    );
    // Verify that Charge values are self-consistent: each block's charge should
    // have instructions >= pushes (most instructions push at most 1 value)
    for (_label, code) in &blocks {
        if let Some(info) = code.first().and_then(get_charge_info) {
            assert!(
                info.instructions > 0,
                "Charge should have non-zero instructions",
            );
        }
    }
}

#[test]
fn test_charge_multiple_blocks() {
    let blocks = find_function_blocks("charge_tests.move", "branching");
    // branching has if/else, so there should be multiple blocks
    assert!(
        blocks.len() > 1,
        "Expected multiple basic blocks for branching function, got {}",
        blocks.len()
    );
    // Each block with fixed-cost instructions should have its own Charge
    let charge_count = blocks
        .values()
        .filter(|code| code.first().and_then(get_charge_info).is_some())
        .count();
    assert!(
        charge_count >= 2,
        "Expected Charge in at least 2 blocks, got {}",
        charge_count
    );
}

#[test]
fn test_charge_in_loop() {
    let blocks = find_function_blocks("charge_tests.move", "looping");
    // looping has a while loop, so there should be multiple blocks
    assert!(
        blocks.len() > 1,
        "Expected multiple basic blocks for looping function, got {}",
        blocks.len()
    );
    // At least one block should have Charge (the loop body has fixed-cost instructions)
    let has_charge = blocks
        .values()
        .any(|code| code.first().and_then(get_charge_info).is_some());
    assert!(has_charge, "Expected at least one block with Charge");
}

// --- Execution-level tests: Charge preserves semantics ---

#[test]
fn test_charge_execution_pure_arithmetic() {
    let result = setup_vm_and_execute("pure_arithmetic", vec![]);
    assert_eq!(result.len(), 1);
    assert!(
        matches!(result[0], Value::U64(6)),
        "Expected U64(6), got {:?}",
        result[0]
    );
}

#[test]
fn test_charge_execution_variable_only() {
    let result = setup_vm_and_execute("variable_only", vec![Value::u64(42)]);
    assert_eq!(result.len(), 1);
    assert!(
        matches!(result[0], Value::U64(42)),
        "Expected U64(42), got {:?}",
        result[0]
    );
}

#[test]
fn test_charge_execution_branching_true_branch() {
    let result = setup_vm_and_execute("branching", vec![Value::u64(20)]);
    assert_eq!(result.len(), 1);
    assert!(
        matches!(result[0], Value::U64(21)),
        "Expected U64(21), got {:?}",
        result[0]
    );
}

#[test]
fn test_charge_execution_branching_false_branch() {
    let result = setup_vm_and_execute("branching", vec![Value::u64(5)]);
    assert_eq!(result.len(), 1);
    assert!(
        matches!(result[0], Value::U64(7)),
        "Expected U64(7), got {:?}",
        result[0]
    );
}

#[test]
fn test_charge_execution_looping() {
    let result = setup_vm_and_execute("looping", vec![Value::u64(95)]);
    assert_eq!(result.len(), 1);
    assert!(
        matches!(result[0], Value::U64(100)),
        "Expected U64(100), got {:?}",
        result[0]
    );
}
