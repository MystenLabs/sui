// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::identifier_interner::IdentifierInterner,
    jit::{execution::ast::Package as RuntimePackage, translate_package},
    natives::functions::NativeFunctions,
    validation::verification::ast as verif_ast,
};
use indexmap::IndexMap;
use move_binary_format::file_format::{
    AddressIdentifierIndex, IdentifierIndex, ModuleHandle, TableIndex, empty_module,
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId,
};
use move_vm_config::runtime::VMConfig;
use std::collections::BTreeMap;

fn make_verified_empty_package(
    original_id: AccountAddress,
    version_id: AccountAddress,
) -> verif_ast::Package {
    // Minimal valid module
    let module = empty_module();
    let module_id: ModuleId = module.self_id();

    // Assemble verification package with a single module and minimal tables
    verif_ast::Package {
        original_id,
        version_id,
        modules: BTreeMap::from([(module_id, verif_ast::Module { value: module })]),
        type_origin_table: IndexMap::new(),
        linkage_table: BTreeMap::from([(original_id, version_id)]),
        version: 0,
    }
}

fn assert_basic_runtime_pkg(
    pkg: &RuntimePackage,
    original_id: AccountAddress,
    version_id: AccountAddress,
) {
    assert_eq!(pkg.original_id, original_id);
    assert_eq!(pkg.version_id, version_id);
    // One module translated from the single compiled module
    assert_eq!(pkg.loaded_modules.len(), 1);
}

#[test]
fn translate_without_optimization() {
    let original_id = AccountAddress::from([1u8; 32]);
    let version_id = AccountAddress::from([2u8; 32]);
    let verified = make_verified_empty_package(original_id, version_id);

    let vm_config = VMConfig {
        optimize_bytecode: false,
        ..VMConfig::default()
    };
    let natives = NativeFunctions::empty_for_testing().unwrap();
    let interner = IdentifierInterner::new();

    let result = translate_package(&vm_config, &interner, &natives, verified);
    let runtime_pkg = result.expect("translate_package should succeed for minimal package");
    assert_basic_runtime_pkg(&runtime_pkg, original_id, version_id);
}

#[test]
fn translate_with_optimization() {
    let original_id = AccountAddress::from([3u8; 32]);
    let version_id = AccountAddress::from([4u8; 32]);
    let verified = make_verified_empty_package(original_id, version_id);

    let vm_config = VMConfig {
        optimize_bytecode: true,
        ..VMConfig::default()
    };
    let natives = NativeFunctions::empty_for_testing().unwrap();
    let interner = IdentifierInterner::new();

    let result = translate_package(&vm_config, &interner, &natives, verified);
    let runtime_pkg = result.expect("translate_package should succeed for minimal package");
    assert_basic_runtime_pkg(&runtime_pkg, original_id, version_id);
}

// -------------------------------------------------------------------------------------------------
// Helper functions for creating modules with dependencies
// -------------------------------------------------------------------------------------------------

const TEST_ADDR: AccountAddress = {
    let mut address = [0u8; AccountAddress::LENGTH];
    address[AccountAddress::LENGTH - 1] = 2u8;
    AccountAddress::new(address)
};

/// Creates a leaf module (no dependencies) with the given name.
fn make_leaf_module(name: &str) -> verif_ast::Module {
    let mut module = empty_module();
    module.identifiers[0] = Identifier::new(name).unwrap();
    module.address_identifiers[0] = TEST_ADDR;
    verif_ast::Module { value: module }
}

/// Creates a module with dependencies on other modules in the same package.
fn make_module_with_deps(name: &str, deps: &[&str]) -> verif_ast::Module {
    let mut module = empty_module();
    module.address_identifiers[0] = TEST_ADDR;
    module.identifiers[0] = Identifier::new(name).unwrap();
    for dep in deps {
        module.identifiers.push(Identifier::new(*dep).unwrap());
        module.module_handles.push(ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex((module.identifiers.len() - 1) as TableIndex),
        });
    }
    verif_ast::Module { value: module }
}

/// Creates a verified package from multiple modules.
fn make_verified_package_from_modules(
    original_id: AccountAddress,
    version_id: AccountAddress,
    modules: Vec<verif_ast::Module>,
) -> verif_ast::Package {
    let modules_map: BTreeMap<ModuleId, verif_ast::Module> = modules
        .into_iter()
        .map(|m| (m.value.self_id(), m))
        .collect();

    verif_ast::Package {
        original_id,
        version_id,
        modules: modules_map,
        type_origin_table: IndexMap::new(),
        linkage_table: BTreeMap::from([(original_id, version_id)]),
        version: 0,
    }
}

fn translate_and_verify(
    verified: verif_ast::Package,
    expected_module_count: usize,
) -> RuntimePackage {
    let vm_config = VMConfig::default();
    let natives = NativeFunctions::empty_for_testing().unwrap();
    let interner = IdentifierInterner::new();

    let result = translate_package(&vm_config, &interner, &natives, verified);
    let runtime_pkg = result.expect("translate_package should succeed");
    assert_eq!(runtime_pkg.loaded_modules.len(), expected_module_count);
    runtime_pkg
}

// -------------------------------------------------------------------------------------------------
// Tests for translate_modules with various dependency structures
// -------------------------------------------------------------------------------------------------

/// Test: Single leaf module (no dependencies).
/// This exercises the leaf node optimization path where the module is loaded directly
/// without going through the Visiting state.
#[test]
fn translate_single_leaf_module() {
    let original_id = AccountAddress::from([10u8; 32]);
    let version_id = AccountAddress::from([11u8; 32]);

    let module_a = make_leaf_module("A");
    let verified = make_verified_package_from_modules(original_id, version_id, vec![module_a]);

    let pkg = translate_and_verify(verified, 1);
    assert_eq!(pkg.original_id, original_id);
    assert_eq!(pkg.version_id, version_id);
}

/// Test: Multiple independent leaf modules (no dependencies between them).
/// All modules should use the leaf node fast path.
#[test]
fn translate_multiple_leaf_modules() {
    let original_id = AccountAddress::from([20u8; 32]);
    let version_id = AccountAddress::from([21u8; 32]);

    let modules = vec![
        make_leaf_module("A"),
        make_leaf_module("B"),
        make_leaf_module("C"),
        make_leaf_module("D"),
        make_leaf_module("E"),
    ];
    let verified = make_verified_package_from_modules(original_id, version_id, modules);

    let pkg = translate_and_verify(verified, 5);
    assert_eq!(pkg.original_id, original_id);
}

/// Test: Linear dependency chain (A <- B <- C <- D).
/// D depends on C, C depends on B, B depends on A, A is a leaf.
/// Only A uses the leaf fast path; B, C, D go through Visiting state.
#[test]
fn translate_dependency_chain() {
    let original_id = AccountAddress::from([30u8; 32]);
    let version_id = AccountAddress::from([31u8; 32]);

    let modules = vec![
        make_leaf_module("A"),
        make_module_with_deps("B", &["A"]),
        make_module_with_deps("C", &["B"]),
        make_module_with_deps("D", &["C"]),
    ];
    let verified = make_verified_package_from_modules(original_id, version_id, modules);

    let pkg = translate_and_verify(verified, 4);
    assert_eq!(pkg.original_id, original_id);
}

/// Test: Diamond dependency pattern.
///       A
///      / \
///     B   C
///      \ /
///       D
/// D depends on both B and C, both B and C depend on A.
/// A is a leaf (fast path), B and C depend on A, D depends on both.
#[test]
fn translate_diamond_dependency() {
    let original_id = AccountAddress::from([40u8; 32]);
    let version_id = AccountAddress::from([41u8; 32]);

    let modules = vec![
        make_leaf_module("A"),
        make_module_with_deps("B", &["A"]),
        make_module_with_deps("C", &["A"]),
        make_module_with_deps("D", &["B", "C"]),
    ];
    let verified = make_verified_package_from_modules(original_id, version_id, modules);

    let pkg = translate_and_verify(verified, 4);
    assert_eq!(pkg.original_id, original_id);
}

/// Test: Wide dependency tree (root depends on many leaves).
///     Root
///    / | \
///   A  B  C  (all leaves)
#[test]
fn translate_wide_dependency_tree() {
    let original_id = AccountAddress::from([50u8; 32]);
    let version_id = AccountAddress::from([51u8; 32]);

    let modules = vec![
        make_leaf_module("A"),
        make_leaf_module("B"),
        make_leaf_module("C"),
        make_module_with_deps("Root", &["A", "B", "C"]),
    ];
    let verified = make_verified_package_from_modules(original_id, version_id, modules);

    let pkg = translate_and_verify(verified, 4);
    assert_eq!(pkg.original_id, original_id);
}

/// Test: Complex dependency graph with multiple levels.
///        E
///       /|\
///      B C D
///      |/|/
///      A F   (A and F are leaves)
#[test]
fn translate_complex_dependency_graph() {
    let original_id = AccountAddress::from([60u8; 32]);
    let version_id = AccountAddress::from([61u8; 32]);

    let modules = vec![
        make_leaf_module("A"),
        make_leaf_module("F"),
        make_module_with_deps("B", &["A"]),
        make_module_with_deps("C", &["A", "F"]),
        make_module_with_deps("D", &["F"]),
        make_module_with_deps("E", &["B", "C", "D"]),
    ];
    let verified = make_verified_package_from_modules(original_id, version_id, modules);

    let pkg = translate_and_verify(verified, 6);
    assert_eq!(pkg.original_id, original_id);
}

/// Test: Large number of leaf modules to stress the fast path.
#[test]
fn translate_many_leaf_modules() {
    let original_id = AccountAddress::from([70u8; 32]);
    let version_id = AccountAddress::from([71u8; 32]);

    let module_count = 50;
    let modules: Vec<_> = (0..module_count)
        .map(|i| make_leaf_module(&format!("Module{}", i)))
        .collect();
    let verified = make_verified_package_from_modules(original_id, version_id, modules);

    let pkg = translate_and_verify(verified, module_count);
    assert_eq!(pkg.original_id, original_id);
}

/// Test: Deep dependency chain to verify correct ordering.
#[test]
fn translate_deep_dependency_chain() {
    let original_id = AccountAddress::from([80u8; 32]);
    let version_id = AccountAddress::from([81u8; 32]);

    let depth = 20;
    let mut modules = vec![make_leaf_module("M0")];
    for i in 1..depth {
        modules.push(make_module_with_deps(
            &format!("M{}", i),
            &[&format!("M{}", i - 1)],
        ));
    }
    let verified = make_verified_package_from_modules(original_id, version_id, modules);

    let pkg = translate_and_verify(verified, depth);
    assert_eq!(pkg.original_id, original_id);
}

/// Test: Mixed leaf and non-leaf modules interleaved.
/// Tests that the algorithm correctly handles a mix of fast-path and regular-path modules.
#[test]
fn translate_mixed_leaf_and_nonleaf() {
    let original_id = AccountAddress::from([90u8; 32]);
    let version_id = AccountAddress::from([91u8; 32]);

    // Leaf1, Leaf2 are independent leaves
    // NonLeaf1 depends on Leaf1
    // NonLeaf2 depends on Leaf2
    // Root depends on NonLeaf1 and NonLeaf2
    let modules = vec![
        make_leaf_module("Leaf1"),
        make_leaf_module("Leaf2"),
        make_module_with_deps("NonLeaf1", &["Leaf1"]),
        make_module_with_deps("NonLeaf2", &["Leaf2"]),
        make_module_with_deps("Root", &["NonLeaf1", "NonLeaf2"]),
    ];
    let verified = make_verified_package_from_modules(original_id, version_id, modules);

    let pkg = translate_and_verify(verified, 5);
    assert_eq!(pkg.original_id, original_id);
}
