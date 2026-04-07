// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bytecode comparison tests for direct call inlining.
//! These tests verify the exact bytecode output after inlining optimization.

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
};
use move_core_types::account_address::AccountAddress;

/// Represents a simplified bytecode for comparison purposes.
/// This strips out pointer-based data that can't be directly compared.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SimpleBytecode {
    Pop,
    Ret,
    BrTrue(u16),
    BrFalse(u16),
    Branch(u16),
    LdU8(u8),
    LdU16(u16),
    LdU32(u32),
    LdU64(u64),
    LdU128(u128),
    LdU256(String), // Use string representation for U256
    LdTrue,
    LdFalse,
    LdConst, // Simplified - just marks presence of constant load (includes addresses)
    CastU8,
    CastU16,
    CastU32,
    CastU64,
    CastU128,
    CastU256,
    CopyLoc(u8),
    MoveLoc(u8),
    StLoc(u8),
    MutBorrowLoc(u8),
    ImmBorrowLoc(u8),
    DirectCall,  // Simplified - just marks presence of direct call
    VirtualCall, // Simplified - just marks presence of virtual call
    CallGeneric, // Simplified - just marks presence of generic call
    Pack,
    PackGeneric,
    Unpack,
    UnpackGeneric,
    ReadRef,
    WriteRef,
    FreezeRef,
    MutBorrowField,
    MutBorrowFieldGeneric,
    ImmBorrowField,
    ImmBorrowFieldGeneric,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitOr,
    BitAnd,
    Xor,
    Shl,
    Shr,
    Or,
    And,
    Not,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    Abort,
    Nop,
    VecPack(u64),
    VecLen,
    VecImmBorrow,
    VecMutBorrow,
    VecPushBack,
    VecPopBack,
    VecUnpack(u64),
    VecSwap,
    PackVariant,
    PackVariantGeneric,
    UnpackVariant,
    UnpackVariantImmRef,
    UnpackVariantMutRef,
    UnpackVariantGeneric,
    UnpackVariantGenericImmRef,
    UnpackVariantGenericMutRef,
    VariantSwitch,
}

/// Converts a Function's bytecode to a simplified form for comparison
fn to_simple_bytecode(func: &Function) -> Vec<SimpleBytecode> {
    func.code()
        .iter()
        .map(|bc| match bc {
            Bytecode::Pop => SimpleBytecode::Pop,
            Bytecode::Ret => SimpleBytecode::Ret,
            Bytecode::BrTrue(t) => SimpleBytecode::BrTrue(*t),
            Bytecode::BrFalse(t) => SimpleBytecode::BrFalse(*t),
            Bytecode::Branch(t) => SimpleBytecode::Branch(*t),
            Bytecode::LdU8(v) => SimpleBytecode::LdU8(*v),
            Bytecode::LdU16(v) => SimpleBytecode::LdU16(*v),
            Bytecode::LdU32(v) => SimpleBytecode::LdU32(*v),
            Bytecode::LdU64(v) => SimpleBytecode::LdU64(*v),
            Bytecode::LdU128(v) => SimpleBytecode::LdU128(**v),
            Bytecode::LdU256(v) => SimpleBytecode::LdU256(format!("{:?}", v)),
            Bytecode::LdTrue => SimpleBytecode::LdTrue,
            Bytecode::LdFalse => SimpleBytecode::LdFalse,
            Bytecode::LdConst(_) => SimpleBytecode::LdConst,
            Bytecode::CastU8 => SimpleBytecode::CastU8,
            Bytecode::CastU16 => SimpleBytecode::CastU16,
            Bytecode::CastU32 => SimpleBytecode::CastU32,
            Bytecode::CastU64 => SimpleBytecode::CastU64,
            Bytecode::CastU128 => SimpleBytecode::CastU128,
            Bytecode::CastU256 => SimpleBytecode::CastU256,
            Bytecode::CopyLoc(l) => SimpleBytecode::CopyLoc(*l),
            Bytecode::MoveLoc(l) => SimpleBytecode::MoveLoc(*l),
            Bytecode::StLoc(l) => SimpleBytecode::StLoc(*l),
            Bytecode::MutBorrowLoc(l) => SimpleBytecode::MutBorrowLoc(*l),
            Bytecode::ImmBorrowLoc(l) => SimpleBytecode::ImmBorrowLoc(*l),
            Bytecode::DirectCall(_) => SimpleBytecode::DirectCall,
            Bytecode::VirtualCall(_) => SimpleBytecode::VirtualCall,
            Bytecode::CallGeneric(_) => SimpleBytecode::CallGeneric,
            Bytecode::Pack(_) => SimpleBytecode::Pack,
            Bytecode::PackGeneric(_) => SimpleBytecode::PackGeneric,
            Bytecode::Unpack(_) => SimpleBytecode::Unpack,
            Bytecode::UnpackGeneric(_) => SimpleBytecode::UnpackGeneric,
            Bytecode::ReadRef => SimpleBytecode::ReadRef,
            Bytecode::WriteRef => SimpleBytecode::WriteRef,
            Bytecode::FreezeRef => SimpleBytecode::FreezeRef,
            Bytecode::MutBorrowField(_) => SimpleBytecode::MutBorrowField,
            Bytecode::MutBorrowFieldGeneric(_) => SimpleBytecode::MutBorrowFieldGeneric,
            Bytecode::ImmBorrowField(_) => SimpleBytecode::ImmBorrowField,
            Bytecode::ImmBorrowFieldGeneric(_) => SimpleBytecode::ImmBorrowFieldGeneric,
            Bytecode::Add => SimpleBytecode::Add,
            Bytecode::Sub => SimpleBytecode::Sub,
            Bytecode::Mul => SimpleBytecode::Mul,
            Bytecode::Div => SimpleBytecode::Div,
            Bytecode::Mod => SimpleBytecode::Mod,
            Bytecode::BitOr => SimpleBytecode::BitOr,
            Bytecode::BitAnd => SimpleBytecode::BitAnd,
            Bytecode::Xor => SimpleBytecode::Xor,
            Bytecode::Shl => SimpleBytecode::Shl,
            Bytecode::Shr => SimpleBytecode::Shr,
            Bytecode::Or => SimpleBytecode::Or,
            Bytecode::And => SimpleBytecode::And,
            Bytecode::Not => SimpleBytecode::Not,
            Bytecode::Eq => SimpleBytecode::Eq,
            Bytecode::Neq => SimpleBytecode::Neq,
            Bytecode::Lt => SimpleBytecode::Lt,
            Bytecode::Gt => SimpleBytecode::Gt,
            Bytecode::Le => SimpleBytecode::Le,
            Bytecode::Ge => SimpleBytecode::Ge,
            Bytecode::Abort => SimpleBytecode::Abort,
            Bytecode::Nop => SimpleBytecode::Nop,
            Bytecode::VecPack(_, n) => SimpleBytecode::VecPack(*n),
            Bytecode::VecLen(_) => SimpleBytecode::VecLen,
            Bytecode::VecImmBorrow(_) => SimpleBytecode::VecImmBorrow,
            Bytecode::VecMutBorrow(_) => SimpleBytecode::VecMutBorrow,
            Bytecode::VecPushBack(_) => SimpleBytecode::VecPushBack,
            Bytecode::VecPopBack(_) => SimpleBytecode::VecPopBack,
            Bytecode::VecUnpack(_, n) => SimpleBytecode::VecUnpack(*n),
            Bytecode::VecSwap(_) => SimpleBytecode::VecSwap,
            Bytecode::PackVariant(_) => SimpleBytecode::PackVariant,
            Bytecode::PackVariantGeneric(_) => SimpleBytecode::PackVariantGeneric,
            Bytecode::UnpackVariant(_) => SimpleBytecode::UnpackVariant,
            Bytecode::UnpackVariantImmRef(_) => SimpleBytecode::UnpackVariantImmRef,
            Bytecode::UnpackVariantMutRef(_) => SimpleBytecode::UnpackVariantMutRef,
            Bytecode::UnpackVariantGeneric(_) => SimpleBytecode::UnpackVariantGeneric,
            Bytecode::UnpackVariantGenericImmRef(_) => SimpleBytecode::UnpackVariantGenericImmRef,
            Bytecode::UnpackVariantGenericMutRef(_) => SimpleBytecode::UnpackVariantGenericMutRef,
            Bytecode::VariantSwitch(_) => SimpleBytecode::VariantSwitch,
        })
        .collect()
}

/// Helper to load the test module
fn load_test_module() -> (crate::jit::execution::ast::Package, IdentifierInterner) {
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

    (runtime_pkg, interner)
}

/// Find a function by name in the loaded module
fn find_function<'a>(
    pkg: &'a crate::jit::execution::ast::Package,
    interner: &IdentifierInterner,
    name: &str,
) -> &'a Function {
    pkg.loaded_modules
        .values()
        .next()
        .expect("Expected at least one module")
        .functions
        .iter()
        .find(|f| f.name(interner).as_str() == name)
        .unwrap_or_else(|| panic!("Expected to find '{}' function", name))
}

/// Tests bytecode for inline_caller after inlining.
///
/// Tests the following Move function:
/// ```move
/// fun get_constant(): u64 { 42 }
///
/// public fun inline_caller(): u64 {
///     get_constant()
/// }
/// ```
#[test]
fn test_bytecode_inline_caller() {
    let (pkg, interner) = load_test_module();
    let func = find_function(&pkg, &interner, "inline_caller");
    let bytecode = to_simple_bytecode(func);

    // Expected bytecode after inlining:
    // get_constant() was: LdU64(42), Ret
    // After inlining into inline_caller: LdU64(42), Nop (Ret becomes Nop), Ret
    use SimpleBytecode::*;
    let expected = vec![
        LdU64(42), // From inlined get_constant
        Nop,       // Ret from get_constant converted to Nop
        Ret,       // Original return of inline_caller
    ];

    assert_eq!(
        bytecode, expected,
        "inline_caller bytecode mismatch.\nActual:   {:?}\nExpected: {:?}",
        bytecode, expected
    );
}

/// Tests bytecode for multi_inline_caller with multiple inlined calls.
///
/// Tests the following Move function:
/// ```move
/// fun get_constant(): u64 { 42 }
///
/// public fun multi_inline_caller(): u64 {
///     let a = get_constant();
///     let b = get_constant();
///     a + b
/// }
/// ```
#[test]
fn test_bytecode_multi_inline_caller() {
    let (pkg, interner) = load_test_module();
    let func = find_function(&pkg, &interner, "multi_inline_caller");
    let bytecode = to_simple_bytecode(func);

    // Expected bytecode after inlining:
    // Original: LdU64(42), Ret -> inlined as LdU64(42), Nop
    // The function stores results in locals, adds them, returns
    use SimpleBytecode::*;
    let expected = vec![
        LdU64(42),  // First inlined get_constant
        Nop,        // Ret -> Nop
        StLoc(0),   // let a = ...
        LdU64(42),  // Second inlined get_constant
        Nop,        // Ret -> Nop
        StLoc(1),   // let b = ...
        MoveLoc(0), // a
        MoveLoc(1), // b
        Add,        // a + b
        Ret,
    ];

    assert_eq!(
        bytecode, expected,
        "multi_inline_caller bytecode mismatch.\nActual:   {:?}\nExpected: {:?}",
        bytecode, expected
    );
}

/// Tests bytecode for inline_in_conditional with inlined call inside a branch.
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
fn test_bytecode_inline_in_conditional() {
    let (pkg, interner) = load_test_module();
    let func = find_function(&pkg, &interner, "inline_in_conditional");
    let bytecode = to_simple_bytecode(func);

    // Expected bytecode structure:
    // 0: MoveLoc(0)       - load flag
    // 1: BrFalse(6)       - if !flag goto else branch (adjusted for inlining)
    // 2: LdU64(42)        - inlined get_constant
    // 3: Nop              - Ret -> Nop
    // 4: StLoc(1)         - store result
    // 5: Branch(8)        - skip else branch (adjusted for inlining)
    // 6: LdU64(100)       - else: 100
    // 7: StLoc(1)         - store result
    // 8: MoveLoc(1)       - load result
    // 9: Ret
    use SimpleBytecode::*;
    let expected = vec![
        MoveLoc(0), // load flag parameter
        BrFalse(6), // if !flag goto else (target adjusted for inlined code)
        LdU64(42),  // inlined get_constant
        Nop,        // Ret -> Nop
        StLoc(1),   // store to local
        Branch(8),  // skip else branch (target adjusted)
        LdU64(100), // else branch: 100
        StLoc(1),   // store to local
        MoveLoc(1), // load result
        Ret,
    ];

    assert_eq!(
        bytecode, expected,
        "inline_in_conditional bytecode mismatch.\nActual:   {:?}\nExpected: {:?}",
        bytecode, expected
    );
}

/// Tests bytecode for branch_over_inline where branch jumps over inlined code.
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
fn test_bytecode_branch_over_inline() {
    let (pkg, interner) = load_test_module();
    let func = find_function(&pkg, &interner, "branch_over_inline");
    let bytecode = to_simple_bytecode(func);

    // Expected bytecode structure:
    // 0: MoveLoc(0)       - load flag
    // 1: BrFalse(5)       - if !flag goto else (adjusted for inlined code)
    // 2: LdU64(50)        - true branch: 50
    // 3: StLoc(1)         - store result
    // 4: Branch(8)        - skip else (target adjusted for inlined code expansion)
    // 5: LdU64(42)        - inlined get_constant
    // 6: Nop              - Ret -> Nop
    // 7: StLoc(1)         - store result
    // 8: MoveLoc(1)       - load result
    // 9: LdU64(1)         - load 1
    // 10: Add             - result + 1
    // 11: Ret
    use SimpleBytecode::*;
    let expected = vec![
        MoveLoc(0), // load flag parameter
        BrFalse(5), // if !flag goto else
        LdU64(50),  // true branch: 50
        StLoc(1),   // store to local
        Branch(8),  // skip else branch (adjusted for inlined code)
        LdU64(42),  // inlined get_constant
        Nop,        // Ret -> Nop
        StLoc(1),   // store to local
        MoveLoc(1), // load result
        LdU64(1),   // load 1
        Add,        // result + 1
        Ret,
    ];

    assert_eq!(
        bytecode, expected,
        "branch_over_inline bytecode mismatch.\nActual:   {:?}\nExpected: {:?}",
        bytecode, expected
    );
}

/// Tests bytecode for complex_branches with multiple conditionals and inlined calls.
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
fn test_bytecode_complex_branches() {
    let (pkg, interner) = load_test_module();
    let func = find_function(&pkg, &interner, "complex_branches");
    let bytecode = to_simple_bytecode(func);

    // Expected bytecode structure (with two inlined calls):
    use SimpleBytecode::*;
    let expected = vec![
        // First conditional: if (a) { get_constant() } else { 0 }
        MoveLoc(0), // 0: load a
        BrFalse(6), // 1: if !a goto else (adjusted)
        LdU64(42),  // 2: inlined get_constant
        Nop,        // 3: Ret -> Nop
        StLoc(2),   // 4: store to local x
        Branch(8),  // 5: skip else (adjusted)
        LdU64(0),   // 6: else: 0
        StLoc(2),   // 7: store to local x
        MoveLoc(2), // 8: load x
        StLoc(4),   // 9: store in temp for later use
        // Second conditional: if (b) { get_constant() } else { 1 }
        MoveLoc(1),  // 10: load b
        BrFalse(16), // 11: if !b goto else (adjusted)
        LdU64(42),   // 12: inlined get_constant
        Nop,         // 13: Ret -> Nop
        StLoc(3),    // 14: store to local y
        Branch(18),  // 15: skip else (adjusted)
        LdU64(1),    // 16: else: 1
        StLoc(3),    // 17: store to local y
        MoveLoc(3),  // 18: load y
        StLoc(5),    // 19: store in temp
        MoveLoc(4),  // 20: load x
        MoveLoc(5),  // 21: load y
        Add,         // 22: x + y
        Ret,         // 23: return
    ];

    assert_eq!(
        bytecode, expected,
        "complex_branches bytecode mismatch.\nActual:   {:?}\nExpected: {:?}",
        bytecode, expected
    );
}

/// Tests bytecode for double_caller with 1-param inlined call.
///
/// Tests the following Move function:
/// ```move
/// fun double(x: u64): u64 { x + x }
///
/// public fun double_caller(): u64 {
///     double(21)
/// }
/// ```
#[test]
fn test_bytecode_double_caller() {
    let (pkg, interner) = load_test_module();
    let func = find_function(&pkg, &interner, "double_caller");
    let bytecode = to_simple_bytecode(func);

    // double has 1 param, so it should NOT be inlined (until locals expansion is implemented)
    use SimpleBytecode::*;
    let expected = vec![
        LdU64(21),  // Load argument
        DirectCall, // NOT inlined - still a call
        Ret,        // Return
    ];

    assert_eq!(
        bytecode, expected,
        "double_caller bytecode mismatch (should NOT be inlined).\nActual:   {:?}\nExpected: {:?}",
        bytecode, expected
    );
}

/// Tests bytecode for caller with 2-param call (add function - NOT inlined).
///
/// Tests the following Move function:
/// ```move
/// fun add(a: u64, b: u64): u64 { a + b }
///
/// public fun caller(): u64 {
///     let x = 10;
///     let y = 20;
///     add(x, y)
/// }
/// ```
#[test]
fn test_bytecode_caller_with_add() {
    let (pkg, interner) = load_test_module();
    let func = find_function(&pkg, &interner, "caller");
    let bytecode = to_simple_bytecode(func);

    // add has 2 params, so it should NOT be inlined (until locals expansion is implemented)
    use SimpleBytecode::*;
    let expected = vec![
        LdU64(10),  // push 10 (was: let x = 10)
        LdU64(20),  // push 20 (was: let y = 20)
        DirectCall, // NOT inlined - still a call
        Ret,        // Original return
    ];

    assert_eq!(
        bytecode, expected,
        "caller bytecode mismatch (should NOT be inlined).\nActual:   {:?}\nExpected: {:?}",
        bytecode, expected
    );
}

/// Tests that add3_caller still has DirectCall (3 params not inlined).
///
/// Tests the following Move function:
/// ```move
/// fun add3(a: u64, b: u64, c: u64): u64 { a + b + c }
///
/// public fun add3_caller(): u64 {
///     add3(1, 2, 3)
/// }
/// ```
#[test]
fn test_bytecode_add3_caller_not_inlined() {
    let (pkg, interner) = load_test_module();
    let func = find_function(&pkg, &interner, "add3_caller");
    let bytecode = to_simple_bytecode(func);

    // add3 has 3 params, so it should NOT be inlined
    // Expected: LdU64(1), LdU64(2), LdU64(3), DirectCall, Ret
    use SimpleBytecode::*;
    let expected = vec![
        LdU64(1),   // Load first arg
        LdU64(2),   // Load second arg
        LdU64(3),   // Load third arg
        DirectCall, // NOT inlined - still a call
        Ret,        // Return
    ];

    assert_eq!(
        bytecode, expected,
        "add3_caller bytecode mismatch (should NOT be inlined).\nActual:   {:?}\nExpected: {:?}",
        bytecode, expected
    );
}

// ============================================================================
// Non-integral parameter type bytecode tests
// Functions with parameters are NOT inlined until locals expansion is implemented
// ============================================================================

/// Tests bytecode for negate_caller - NOT inlined (has params).
///
/// Tests the following Move function:
/// ```move
/// fun negate(b: bool): bool { !b }
///
/// public fun negate_caller(): bool {
///     negate(true)
/// }
/// ```
#[test]
fn test_bytecode_negate_caller() {
    let (pkg, interner) = load_test_module();
    let func = find_function(&pkg, &interner, "negate_caller");
    let bytecode = to_simple_bytecode(func);

    // negate has 1 param, so it should NOT be inlined
    use SimpleBytecode::*;
    let expected = vec![
        LdTrue,     // Load true
        DirectCall, // NOT inlined - still a call
        Ret,        // Original return
    ];

    assert_eq!(
        bytecode, expected,
        "negate_caller bytecode mismatch (should NOT be inlined).\nActual:   {:?}\nExpected: {:?}",
        bytecode, expected
    );
}

/// Tests bytecode for bool_and_caller - NOT inlined (has params).
///
/// Tests the following Move function:
/// ```move
/// fun bool_and(a: bool, b: bool): bool { a && b }
///
/// public fun bool_and_caller(): bool {
///     bool_and(true, false)
/// }
/// ```
#[test]
fn test_bytecode_bool_and_caller() {
    let (pkg, interner) = load_test_module();
    let func = find_function(&pkg, &interner, "bool_and_caller");
    let bytecode = to_simple_bytecode(func);

    // bool_and has 2 params, so it should NOT be inlined
    use SimpleBytecode::*;
    let expected = vec![
        LdTrue,     // Load true (first arg)
        LdFalse,    // Load false (second arg)
        DirectCall, // NOT inlined - still a call
        Ret,        // Original return
    ];

    assert_eq!(
        bytecode, expected,
        "bool_and_caller bytecode mismatch (should NOT be inlined).\nActual:   {:?}\nExpected: {:?}",
        bytecode, expected
    );
}

/// Tests bytecode for is_zero_addr_caller - NOT inlined (has params).
///
/// Tests the following Move function:
/// ```move
/// fun is_zero_addr(addr: address): bool { addr == @0x0 }
///
/// public fun is_zero_addr_caller(): bool {
///     is_zero_addr(@0x1)
/// }
/// ```
#[test]
fn test_bytecode_is_zero_addr_caller() {
    let (pkg, interner) = load_test_module();
    let func = find_function(&pkg, &interner, "is_zero_addr_caller");
    let bytecode = to_simple_bytecode(func);

    // is_zero_addr has 1 param, so it should NOT be inlined
    use SimpleBytecode::*;
    let expected = vec![
        LdConst,    // Load @0x1 (the argument)
        DirectCall, // NOT inlined - still a call
        Ret,        // Original return
    ];

    assert_eq!(
        bytecode, expected,
        "is_zero_addr_caller bytecode mismatch (should NOT be inlined).\nActual:   {:?}\nExpected: {:?}",
        bytecode, expected
    );
}

/// Tests bytecode for check_value_caller - NOT inlined (has params).
///
/// Tests the following Move function:
/// ```move
/// fun check_value(addr: address, expected: u64): bool {
///     addr != @0x0 && expected > 0
/// }
///
/// public fun check_value_caller(): bool {
///     check_value(@0x42, 100)
/// }
/// ```
#[test]
fn test_bytecode_check_value_caller() {
    let (pkg, interner) = load_test_module();
    let func = find_function(&pkg, &interner, "check_value_caller");
    let bytecode = to_simple_bytecode(func);

    // check_value has 2 params, so it should NOT be inlined
    use SimpleBytecode::*;
    let expected = vec![
        LdConst,    // Load @0x42 (first arg - address)
        LdU64(100), // Load 100 (second arg - u64)
        DirectCall, // NOT inlined - still a call
        Ret,        // Original return
    ];

    assert_eq!(
        bytecode, expected,
        "check_value_caller bytecode mismatch (should NOT be inlined).\nActual:   {:?}\nExpected: {:?}",
        bytecode, expected
    );
}
