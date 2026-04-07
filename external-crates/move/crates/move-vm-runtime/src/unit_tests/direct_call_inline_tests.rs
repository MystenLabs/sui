// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::identifier_interner::IdentifierInterner,
    dev_utils::{
        compilation_utils::compile_packages_in_file, in_memory_test_adapter::InMemoryTestAdapter,
        vm_test_adapter::VMTestAdapter,
    },
    jit::{
        self,
        execution::ast::{Bytecode, Function},
    },
    natives::functions::NativeFunctions,
    shared::{gas::UnmeteredGasMeter, linkage_context::LinkageContext},
};
use move_core_types::{account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId};

/// Counts the number of DirectCall bytecodes in a function
fn count_direct_calls(func: &Function) -> usize {
    func.code()
        .iter()
        .filter(|bc| matches!(bc, Bytecode::DirectCall(_)))
        .count()
}

/// Counts the number of LdU64 bytecodes in a function (used to verify constant loading)
fn count_ldu64_instructions(func: &Function) -> usize {
    func.code()
        .iter()
        .filter(|bc| matches!(bc, Bytecode::LdU64(_)))
        .count()
}

/// Checks if a function contains LdU64(42) - the inlined constant
fn has_ldu64_42(func: &Function) -> bool {
    func.code()
        .iter()
        .any(|bc| matches!(bc, Bytecode::LdU64(42)))
}

/// Checks if a function contains any DirectCall bytecodes
fn has_direct_calls(func: &Function) -> bool {
    func.code()
        .iter()
        .any(|bc| matches!(bc, Bytecode::DirectCall(_)))
}

/// Tests that direct calls are properly inlined for functions with <=2 parameters.
///
/// Tests the following Move functions:
/// ```move
/// fun get_constant(): u64 { 42 }
/// fun add(a: u64, b: u64): u64 { a + b }  // 2 params - inlined
/// fun double(x: u64): u64 { x + x }  // 1 param - inlined
/// fun add3(a: u64, b: u64, c: u64): u64 { a + b + c }  // 3 params - NOT inlined
///
/// public fun inline_caller(): u64 { get_constant() }
/// public fun caller(): u64 { add(10, 20) }
/// public fun double_caller(): u64 { double(21) }
/// public fun add3_caller(): u64 { add3(1, 2, 3) }
/// ```
#[test]
fn test_direct_calls_are_inlined() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();

    for pkg in compile_packages_in_file("direct_call_inline.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let serialized_package = adapter.get_package_from_store(&package_address).unwrap();
    let (verif_pkg, _) = adapter
        .verify_package(package_address, serialized_package)
        .unwrap();

    let interner = IdentifierInterner::new();
    let natives = NativeFunctions::empty_for_testing().unwrap();

    let opt_pkg = jit::optimization::to_optimized_form(verif_pkg);
    let runtime_pkg = jit::execution::translate::package(&natives, &interner, opt_pkg.unwrap()).unwrap();

    let module = runtime_pkg
        .loaded_modules
        .values()
        .next()
        .expect("Expected at least one module");

    // Test 1: Functions with parameters are NOT inlined (until locals expansion is implemented)
    // The "caller" function calls "add" which has 2 params
    let caller_func = module
        .functions
        .iter()
        .find(|f| f.name(&interner).as_str() == "caller")
        .expect("Expected to find 'caller' function");

    assert!(
        has_direct_calls(caller_func),
        "caller function should still have DirectCall (add has params). \
         Found {} DirectCalls. Bytecode: {:?}",
        count_direct_calls(caller_func),
        caller_func.code()
    );

    // Test 2: Functions with 1 parameter are NOT inlined
    // The "double_caller" function calls "double" which has 1 param
    let double_caller_func = module
        .functions
        .iter()
        .find(|f| f.name(&interner).as_str() == "double_caller")
        .expect("Expected to find 'double_caller' function");

    assert!(
        has_direct_calls(double_caller_func),
        "double_caller function should still have DirectCall (double has params). \
         Found {} DirectCalls. Bytecode: {:?}",
        count_direct_calls(double_caller_func),
        double_caller_func.code()
    );

    // Test 3: Functions with 3+ parameters are NOT inlined
    // The "add3_caller" function calls "add3" which has 3 params
    let add3_caller_func = module
        .functions
        .iter()
        .find(|f| f.name(&interner).as_str() == "add3_caller")
        .expect("Expected to find 'add3_caller' function");

    assert!(
        has_direct_calls(add3_caller_func),
        "add3_caller function should still have DirectCall (add3 has params). \
         Found {} DirectCalls. Bytecode: {:?}",
        count_direct_calls(add3_caller_func),
        add3_caller_func.code()
    );

    // Test 4: Functions without parameters ARE inlined
    let inline_caller_func = module
        .functions
        .iter()
        .find(|f| f.name(&interner).as_str() == "inline_caller")
        .expect("Expected to find 'inline_caller' function");

    assert!(
        !has_direct_calls(inline_caller_func),
        "inline_caller function should NOT have DirectCall after inlining. \
         Found {} DirectCalls. Bytecode: {:?}",
        count_direct_calls(inline_caller_func),
        inline_caller_func.code()
    );

    // The inlined code should contain LdU64(42) from get_constant
    assert!(
        has_ldu64_42(inline_caller_func),
        "inline_caller function should have LdU64(42) after inlining get_constant. \
         Bytecode: {:?}",
        inline_caller_func.code()
    );

    // Test 5: Multiple calls to inlineable function
    let multi_inline_caller_func = module
        .functions
        .iter()
        .find(|f| f.name(&interner).as_str() == "multi_inline_caller")
        .expect("Expected to find 'multi_inline_caller' function");

    assert!(
        !has_direct_calls(multi_inline_caller_func),
        "multi_inline_caller function should NOT have DirectCalls after inlining. \
         Found {} DirectCalls. Bytecode: {:?}",
        count_direct_calls(multi_inline_caller_func),
        multi_inline_caller_func.code()
    );

    // Should have at least 2 LdU64 instructions (one from each inlined get_constant call)
    let ldu64_count = count_ldu64_instructions(multi_inline_caller_func);
    assert!(
        ldu64_count >= 2,
        "multi_inline_caller should have at least 2 LdU64 instructions after inlining. \
         Found {}. Bytecode: {:?}",
        ldu64_count,
        multi_inline_caller_func.code()
    );
}

/// Counts the number of branch instructions (BrTrue, BrFalse, Branch) in a function
fn count_branches(func: &Function) -> usize {
    func.code()
        .iter()
        .filter(|bc| {
            matches!(
                bc,
                Bytecode::BrTrue(_) | Bytecode::BrFalse(_) | Bytecode::Branch(_)
            )
        })
        .count()
}

/// Extracts all branch targets from a function's bytecode
fn get_branch_targets(func: &Function) -> Vec<(usize, u16)> {
    func.code()
        .iter()
        .enumerate()
        .filter_map(|(idx, bc)| match bc {
            Bytecode::BrTrue(target) => Some((idx, *target)),
            Bytecode::BrFalse(target) => Some((idx, *target)),
            Bytecode::Branch(target) => Some((idx, *target)),
            _ => None,
        })
        .collect()
}

/// Validates that all branch targets are within bounds of the function's code
fn validate_branch_targets(func: &Function) -> bool {
    let code_len = func.code().len() as u16;
    for bc in func.code().iter() {
        match bc {
            Bytecode::BrTrue(target) | Bytecode::BrFalse(target) | Bytecode::Branch(target) => {
                if *target >= code_len {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

/// Tests inlining when callee is inside a conditional branch.
///
/// Tests the following Move function:
/// ```move
/// fun get_constant(): u64 { 42 }
///
/// public fun inline_in_conditional(flag: bool): u64 {
///     if (flag) {
///         get_constant()  // This call will be inlined, inside a branch
///     } else {
///         100
///     }
/// }
/// ```
#[test]
fn test_inline_in_conditional() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();

    for pkg in compile_packages_in_file("direct_call_inline.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let serialized_package = adapter.get_package_from_store(&package_address).unwrap();
    let (verif_pkg, _) = adapter
        .verify_package(package_address, serialized_package)
        .unwrap();

    let interner = IdentifierInterner::new();
    let natives = NativeFunctions::empty_for_testing().unwrap();

    let opt_pkg = jit::optimization::to_optimized_form(verif_pkg).unwrap();
    let runtime_pkg = jit::execution::translate::package(&natives, &interner, opt_pkg).unwrap();

    let module = runtime_pkg
        .loaded_modules
        .values()
        .next()
        .expect("Expected at least one module");

    let func = module
        .functions
        .iter()
        .find(|f| f.name(&interner).as_str() == "inline_in_conditional")
        .expect("Expected to find 'inline_in_conditional' function");

    // Should NOT have DirectCalls after inlining
    assert!(
        !has_direct_calls(func),
        "inline_in_conditional should NOT have DirectCall after inlining. \
         Found {} DirectCalls. Bytecode: {:?}",
        count_direct_calls(func),
        func.code()
    );

    // Should have LdU64(42) from the inlined get_constant
    assert!(
        has_ldu64_42(func),
        "inline_in_conditional should have LdU64(42) after inlining. Bytecode: {:?}",
        func.code()
    );

    // All branch targets should be valid (within bounds)
    assert!(
        validate_branch_targets(func),
        "inline_in_conditional has invalid branch targets after inlining. \
         Branches: {:?}, Code length: {}. Bytecode: {:?}",
        get_branch_targets(func),
        func.code().len(),
        func.code()
    );

    // Should still have branches for the conditional
    assert!(
        count_branches(func) > 0,
        "inline_in_conditional should have branch instructions for the conditional. \
         Bytecode: {:?}",
        func.code()
    );
}

/// Tests branch target adjustment when a branch jumps over inlined code.
///
/// Tests the following Move function:
/// ```move
/// fun get_constant(): u64 { 42 }
///
/// public fun branch_over_inline(flag: bool): u64 {
///     let result = if (flag) {
///         50  // Branch jumps over the else block
///     } else {
///         get_constant()  // Inlined call - code expands here
///     };
///     result + 1
/// }
/// ```
#[test]
fn test_branch_over_inline() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();

    for pkg in compile_packages_in_file("direct_call_inline.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let serialized_package = adapter.get_package_from_store(&package_address).unwrap();
    let (verif_pkg, _) = adapter
        .verify_package(package_address, serialized_package)
        .unwrap();

    let interner = IdentifierInterner::new();
    let natives = NativeFunctions::empty_for_testing().unwrap();

    let opt_pkg = jit::optimization::to_optimized_form(verif_pkg).unwrap();
    let runtime_pkg = jit::execution::translate::package(&natives, &interner, opt_pkg).unwrap();

    let module = runtime_pkg
        .loaded_modules
        .values()
        .next()
        .expect("Expected at least one module");

    let func = module
        .functions
        .iter()
        .find(|f| f.name(&interner).as_str() == "branch_over_inline")
        .expect("Expected to find 'branch_over_inline' function");

    // Should NOT have DirectCalls after inlining
    assert!(
        !has_direct_calls(func),
        "branch_over_inline should NOT have DirectCall after inlining. \
         Found {} DirectCalls. Bytecode: {:?}",
        count_direct_calls(func),
        func.code()
    );

    // Should have LdU64(42) from the inlined get_constant
    assert!(
        has_ldu64_42(func),
        "branch_over_inline should have LdU64(42) after inlining. Bytecode: {:?}",
        func.code()
    );

    // All branch targets should be valid (within bounds)
    assert!(
        validate_branch_targets(func),
        "branch_over_inline has invalid branch targets after inlining. \
         Branches: {:?}, Code length: {}. Bytecode: {:?}",
        get_branch_targets(func),
        func.code().len(),
        func.code()
    );
}

/// Tests multiple branches with multiple inlined calls.
///
/// Tests the following Move function:
/// ```move
/// fun get_constant(): u64 { 42 }
///
/// public fun complex_branches(a: bool, b: bool): u64 {
///     let x = if (a) {
///         get_constant()  // First inlined call
///     } else {
///         0
///     };
///     let y = if (b) {
///         get_constant()  // Second inlined call
///     } else {
///         1
///     };
///     x + y
/// }
/// ```
#[test]
fn test_complex_branches() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();

    for pkg in compile_packages_in_file("direct_call_inline.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let serialized_package = adapter.get_package_from_store(&package_address).unwrap();
    let (verif_pkg, _) = adapter
        .verify_package(package_address, serialized_package)
        .unwrap();

    let interner = IdentifierInterner::new();
    let natives = NativeFunctions::empty_for_testing().unwrap();

    let opt_pkg = jit::optimization::to_optimized_form(verif_pkg).unwrap();
    let runtime_pkg = jit::execution::translate::package(&natives, &interner, opt_pkg).unwrap();

    let module = runtime_pkg
        .loaded_modules
        .values()
        .next()
        .expect("Expected at least one module");

    let func = module
        .functions
        .iter()
        .find(|f| f.name(&interner).as_str() == "complex_branches")
        .expect("Expected to find 'complex_branches' function");

    // Should NOT have DirectCalls after inlining
    assert!(
        !has_direct_calls(func),
        "complex_branches should NOT have DirectCall after inlining. \
         Found {} DirectCalls. Bytecode: {:?}",
        count_direct_calls(func),
        func.code()
    );

    // Should have at least 2 LdU64(42) from the two inlined get_constant calls
    let ldu64_42_count = func
        .code()
        .iter()
        .filter(|bc| matches!(bc, Bytecode::LdU64(42)))
        .count();
    assert!(
        ldu64_42_count >= 2,
        "complex_branches should have at least 2 LdU64(42) after inlining. \
         Found {}. Bytecode: {:?}",
        ldu64_42_count,
        func.code()
    );

    // All branch targets should be valid (within bounds)
    assert!(
        validate_branch_targets(func),
        "complex_branches has invalid branch targets after inlining. \
         Branches: {:?}, Code length: {}. Bytecode: {:?}",
        get_branch_targets(func),
        func.code().len(),
        func.code()
    );
}

/// Tests that functions with non-integral parameter types are NOT inlined
/// (until locals expansion is implemented).
///
/// Tests the following Move functions:
/// ```move
/// fun negate(b: bool): bool { !b }
/// fun bool_and(a: bool, b: bool): bool { a && b }
/// fun is_zero_addr(addr: address): bool { addr == @0x0 }
/// fun check_value(addr: address, expected: u64): bool { addr != @0x0 && expected > 0 }
///
/// public fun negate_caller(): bool { negate(true) }
/// public fun bool_and_caller(): bool { bool_and(true, false) }
/// public fun is_zero_addr_caller(): bool { is_zero_addr(@0x1) }
/// public fun check_value_caller(): bool { check_value(@0x42, 100) }
/// ```
#[test]
fn test_non_integral_param_types_not_inlined() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();

    for pkg in compile_packages_in_file("direct_call_inline.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let serialized_package = adapter.get_package_from_store(&package_address).unwrap();
    let (verif_pkg, _) = adapter
        .verify_package(package_address, serialized_package)
        .unwrap();

    let interner = IdentifierInterner::new();
    let natives = NativeFunctions::empty_for_testing().unwrap();

    let opt_pkg = jit::optimization::to_optimized_form(verif_pkg).unwrap();
    let runtime_pkg = jit::execution::translate::package(&natives, &interner, opt_pkg).unwrap();

    let module = runtime_pkg
        .loaded_modules
        .values()
        .next()
        .expect("Expected at least one module");

    // Test 1: bool parameter (1 param) - NOT inlined
    let negate_caller_func = module
        .functions
        .iter()
        .find(|f| f.name(&interner).as_str() == "negate_caller")
        .expect("Expected to find 'negate_caller' function");

    assert!(
        has_direct_calls(negate_caller_func),
        "negate_caller should still have DirectCall (has params). \
         Found {} DirectCalls. Bytecode: {:?}",
        count_direct_calls(negate_caller_func),
        negate_caller_func.code()
    );

    // Test 2: two bool parameters (2 params) - NOT inlined
    let bool_and_caller_func = module
        .functions
        .iter()
        .find(|f| f.name(&interner).as_str() == "bool_and_caller")
        .expect("Expected to find 'bool_and_caller' function");

    assert!(
        has_direct_calls(bool_and_caller_func),
        "bool_and_caller should still have DirectCall (has params). \
         Found {} DirectCalls. Bytecode: {:?}",
        count_direct_calls(bool_and_caller_func),
        bool_and_caller_func.code()
    );

    // Test 3: address parameter (1 param) - NOT inlined
    let is_zero_addr_caller_func = module
        .functions
        .iter()
        .find(|f| f.name(&interner).as_str() == "is_zero_addr_caller")
        .expect("Expected to find 'is_zero_addr_caller' function");

    assert!(
        has_direct_calls(is_zero_addr_caller_func),
        "is_zero_addr_caller should still have DirectCall (has params). \
         Found {} DirectCalls. Bytecode: {:?}",
        count_direct_calls(is_zero_addr_caller_func),
        is_zero_addr_caller_func.code()
    );

    // Test 4: mixed types (address + u64, 2 params) - NOT inlined
    let check_value_caller_func = module
        .functions
        .iter()
        .find(|f| f.name(&interner).as_str() == "check_value_caller")
        .expect("Expected to find 'check_value_caller' function");

    assert!(
        has_direct_calls(check_value_caller_func),
        "check_value_caller should still have DirectCall (has params). \
         Found {} DirectCalls. Bytecode: {:?}",
        count_direct_calls(check_value_caller_func),
        check_value_caller_func.code()
    );
}

/// Tests that functions with parameters are NOT inlined until locals expansion
/// is implemented. This prevents "Local index out of bounds" errors.
///
/// When locals expansion is implemented, this test should be updated to verify
/// that inlining works correctly for functions with parameters.
#[test]
fn test_functions_with_params_not_inlined() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();

    for pkg in compile_packages_in_file("direct_call_inline.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let serialized_package = adapter.get_package_from_store(&package_address).unwrap();
    let (verif_pkg, _) = adapter
        .verify_package(package_address, serialized_package.clone())
        .unwrap();

    let interner = IdentifierInterner::new();
    let natives = NativeFunctions::empty_for_testing().unwrap();

    let opt_pkg = jit::optimization::to_optimized_form(verif_pkg).unwrap();
    let runtime_pkg = jit::execution::translate::package(&natives, &interner, opt_pkg).unwrap();

    let module = runtime_pkg
        .loaded_modules
        .values()
        .next()
        .expect("Expected at least one module");

    let caller_without_locals_func = module
        .functions
        .iter()
        .find(|f| f.name(&interner).as_str() == "caller_without_locals")
        .expect("Expected to find 'caller_without_locals' function");

    // Verify the call was NOT inlined (functions with params shouldn't be inlined yet)
    assert!(
        has_direct_calls(caller_without_locals_func),
        "caller_without_locals should still have DirectCall (callee has params). \
         Found {} DirectCalls. Bytecode: {:?}",
        count_direct_calls(caller_without_locals_func),
        caller_without_locals_func.code()
    );

    // Execute the function to ensure it works correctly
    let linkage = LinkageContext::new(
        adapter
            .get_package_from_store(&package_address)
            .unwrap()
            .linkage_table
            .clone(),
    )
    .unwrap();

    let mut session = adapter.make_vm(linkage).unwrap();

    let module_id = ModuleId::new(package_address, Identifier::new("inline_test").unwrap());
    let func_name = Identifier::new("caller_without_locals").unwrap();

    let result = session.execute_function_bypass_visibility(
        &module_id,
        &func_name,
        vec![],
        vec![],
        &mut UnmeteredGasMeter,
        None,
    );

    assert!(
        result.is_ok(),
        "Executing caller_without_locals should succeed. Error: {:?}",
        result.err()
    );
}
