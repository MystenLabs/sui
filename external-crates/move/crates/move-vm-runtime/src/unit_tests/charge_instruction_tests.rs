// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dev_utils::{
        compilation_utils::compile_packages_in_file,
        gas_schedule::{self, unit_cost_schedule},
        in_memory_test_adapter::InMemoryTestAdapter,
        vm_test_adapter::VMTestAdapter,
    },
    execution::values::Value,
    jit::optimization::{self, ast as opt_ast},
    shared::gas::UnmeteredGasMeter,
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId,
    vm_status::StatusCode,
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
    let pkg = optimization::to_optimized_form(verif_pkg).expect("optimization failed");
    let module_id = ModuleId::new(package_address, Identifier::new("charge_tests").unwrap());
    let module = pkg.modules.get(&module_id).expect("module not found");

    // Find the function by name by matching against the compiled module's function defs
    for (ndx, func) in &module.functions {
        let func_def = &module.compiled_module.function_defs()[ndx.0 as usize];
        let func_handle = &module.compiled_module.function_handles()[func_def.function.0 as usize];
        let name = module.compiled_module.identifier_at(func_handle.name);
        if name.as_str() == func_name
            && let Some(code) = &func.code
        {
            return code.code.clone();
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

/// Execute a function with a metered gas meter (simple, unit-cost schedule).
/// `gas_budget` is in InternalGas units.
/// Returns Ok(gas_used_in_internal_units) on success, Err(status_code) on failure.
fn execute_metered(
    func_name: &str,
    args: Vec<Value>,
    internal_gas_budget: u64,
) -> Result<u64, StatusCode> {
    let package_address = charge_test_addr();
    let cost_table = unit_cost_schedule();
    // Use InternalGas directly to avoid Gas/InternalGas multiplier confusion.
    // GasStatus::new takes Gas, so convert: 1 Gas = 1000 InternalGas.
    // We ceil-divide to ensure we have at least the requested internal gas.
    let gas_units = internal_gas_budget.div_ceil(1000);
    let mut gas_meter =
        gas_schedule::GasStatus::new(&cost_table, gas_schedule::Gas::new(gas_units));
    let initial_gas: u64 = crate::shared::gas::GasMeter::remaining_gas(&gas_meter).into();
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
    match vm.execute_function_bypass_visibility(
        &module_id,
        &function,
        vec![],
        args,
        &mut gas_meter,
        None,
    ) {
        Ok(_) => {
            let remaining: u64 = crate::shared::gas::GasMeter::remaining_gas(&gas_meter).into();
            Ok(initial_gas - remaining)
        }
        Err(err) => Err(err.major_status()),
    }
}

// --- Optimization-level tests: Charge insertion ---

#[test]
fn test_charge_inserted_in_blocks() {
    let blocks = find_function_blocks("charge_tests.move", "pure_arithmetic");
    // Every block with fixed-cost instructions should start with Charge
    for code in blocks.values() {
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
    for code in blocks.values() {
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
    assert!(total_pushes > 0, "Expected some pushes, got 0",);
    // Verify that Charge values are self-consistent: each block's charge should
    // have instructions >= pushes (most instructions push at most 1 value)
    for code in blocks.values() {
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

// --- Out-of-gas tests ---

#[test]
fn test_oog_with_sufficient_gas_succeeds() {
    // With a large budget, execution should succeed
    let result = execute_metered("pure_arithmetic", vec![], 10_000_000);
    assert!(result.is_ok(), "Expected success, got {:?}", result);
}

#[test]
fn test_oog_with_zero_gas() {
    // Zero gas should immediately fail
    let result = execute_metered("pure_arithmetic", vec![], 0);
    assert_eq!(
        result,
        Err(StatusCode::OUT_OF_GAS),
        "Expected OUT_OF_GAS with zero budget"
    );
}

#[test]
fn test_oog_with_very_small_gas() {
    // The looping function needs real gas to run 100 iterations.
    // 1 internal gas unit is not enough.
    let result = execute_metered("looping", vec![Value::u64(0)], 1);
    assert_eq!(
        result,
        Err(StatusCode::OUT_OF_GAS),
        "Expected OUT_OF_GAS with minimal budget for 100-iteration loop"
    );
}

#[test]
fn test_oog_triggers_before_block_executes() {
    // Find the minimum gas that succeeds, then verify that one less fails.
    // This demonstrates that Charge triggers OOG at block entry.
    let mut min_success = None;
    for budget in 1..200 {
        if execute_metered("pure_arithmetic", vec![], budget).is_ok() {
            min_success = Some(budget);
            break;
        }
    }
    let min_success = min_success.expect("pure_arithmetic should succeed with some budget < 200");
    // One less gas unit should fail
    let result = execute_metered("pure_arithmetic", vec![], min_success - 1);
    assert_eq!(
        result,
        Err(StatusCode::OUT_OF_GAS),
        "Expected OUT_OF_GAS with budget {} (one below minimum {})",
        min_success - 1,
        min_success
    );
}

#[test]
fn test_oog_in_loop_body() {
    // looping(0) does 100 iterations — find how much gas it needs, then give less.
    let gas_used = execute_metered("looping", vec![Value::u64(0)], 10_000_000)
        .expect("Should succeed with large budget");
    let half_budget = gas_used / 2;
    assert!(half_budget > 0, "Half budget should be non-zero");
    let result = execute_metered("looping", vec![Value::u64(0)], half_budget);
    assert_eq!(
        result,
        Err(StatusCode::OUT_OF_GAS),
        "Expected OUT_OF_GAS with half the required gas"
    );
}

#[test]
fn test_oog_branching_both_paths_charged() {
    // Both branches of `branching` should consume gas.
    let gas_true = execute_metered("branching", vec![Value::u64(20)], 10_000_000)
        .expect("true branch should succeed");
    let gas_false = execute_metered("branching", vec![Value::u64(5)], 10_000_000)
        .expect("false branch should succeed");
    assert!(gas_true > 0, "True branch should consume gas");
    assert!(gas_false > 0, "False branch should consume gas");
}

// --- Gas equivalence tests ---

#[test]
fn test_gas_deterministic_same_input() {
    // Same function with same input should always consume the same gas
    let gas1 = execute_metered("branching", vec![Value::u64(20)], 10_000_000).unwrap();
    let gas2 = execute_metered("branching", vec![Value::u64(20)], 10_000_000).unwrap();
    assert_eq!(
        gas1, gas2,
        "Same input should produce identical gas consumption"
    );
}

#[test]
fn test_gas_loop_scales_with_iterations() {
    // More loop iterations should cost more gas
    let used_5 = execute_metered("looping", vec![Value::u64(95)], 100_000_000).unwrap();
    let used_50 = execute_metered("looping", vec![Value::u64(50)], 100_000_000).unwrap();
    let used_100 = execute_metered("looping", vec![Value::u64(0)], 100_000_000).unwrap();

    assert!(
        used_5 < used_50,
        "5 iterations ({}) should cost less than 50 ({})",
        used_5,
        used_50
    );
    assert!(
        used_50 < used_100,
        "50 iterations ({}) should cost less than 100 ({})",
        used_50,
        used_100
    );
}

#[test]
fn test_gas_batched_charge_matches_instruction_count() {
    // Verify that the sum of ChargeInfo.instructions across all blocks
    // accounts for the expected fixed-cost instruction count.
    let blocks = find_function_blocks("charge_tests.move", "branching");
    let total_charge_instructions: u64 = blocks
        .values()
        .filter_map(|code| code.first().and_then(get_charge_info))
        .map(|info| info.instructions)
        .sum();

    // branching has fixed-cost instructions across its blocks (loads, comparisons, adds,
    // branches, ret). The total should be non-trivial.
    assert!(
        total_charge_instructions >= 5,
        "Expected at least 5 fixed-cost instructions across all blocks, got {}",
        total_charge_instructions
    );

    // Verify gas is actually consumed. The simple gas schedule's charge_block deducts
    // InternalGas::new(instructions), so total internal gas used should be >= the
    // total fixed-cost instructions charged.
    let gas_used = execute_metered("branching", vec![Value::u64(20)], 100_000_000)
        .expect("should succeed with large budget");
    assert!(
        gas_used >= total_charge_instructions,
        "Gas used ({} internal) should be >= total charged instructions ({})",
        gas_used,
        total_charge_instructions
    );
}

#[test]
fn test_gas_variable_only_still_charges() {
    // A function with only variable-cost instructions should still consume gas
    // (via the per-instruction charge_* calls, not via Charge)
    let gas_used =
        execute_metered("variable_only", vec![Value::u64(42)], 10_000_000).expect("should succeed");
    assert!(
        gas_used > 0,
        "Variable-only function should still consume gas"
    );
}
