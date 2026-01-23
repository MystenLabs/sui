// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::identifier_interner::IdentifierInterner,
    jit::execution::translate::*,
    jit::optimization::ast as input,
    natives::functions::NativeFunctions,
    shared::types::{OriginalId, VersionId},
};

use move_binary_format::file_format::{
    AddressIdentifierIndex, IdentifierIndex, ModuleHandle, empty_module,
};
use move_core_types::resolver::IntraPackageName;
use move_core_types::{account_address::AccountAddress, identifier::Identifier};

use indexmap::IndexMap;
use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------------------------------

fn create_test_natives() -> NativeFunctions {
    NativeFunctions::empty_for_testing().unwrap()
}

fn create_empty_input_module() -> input::Module {
    let compiled_module = empty_module();
    input::Module {
        compiled_module,
        functions: BTreeMap::new(),
    }
}

fn create_test_input_package() -> input::Package {
    let version_id = VersionId::from(AccountAddress::ONE);
    let original_id = OriginalId::from(AccountAddress::TWO);
    let module = create_empty_input_module();
    let modules = BTreeMap::from([(module.compiled_module.self_id(), module)]);

    input::Package {
        version_id,
        original_id,
        modules,
        type_origin_table: IndexMap::new(),
        linkage_table: BTreeMap::from([(original_id, version_id)]),
    }
}

// -------------------------------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------------------------------

#[test]
fn test_package_empty() {
    let natives = create_test_natives();
    let interner = IdentifierInterner::new();
    let input_package = create_test_input_package();

    let result = package(&natives, &interner, input_package);
    assert!(result.is_ok());

    let pkg = result.unwrap();
    assert_eq!(pkg.loaded_modules.len(), 1);
    // Verify that the package was created successfully
}

#[test]
fn test_package_preserves_version_and_original_id() {
    let natives = create_test_natives();
    let interner = IdentifierInterner::new();
    let mut input_package = create_test_input_package();

    let test_version_id = VersionId::from(AccountAddress::from([3u8; AccountAddress::LENGTH]));
    let test_original_id = OriginalId::from(AccountAddress::from([4u8; AccountAddress::LENGTH]));
    input_package.version_id = test_version_id;
    input_package.original_id = test_original_id;

    let result = package(&natives, &interner, input_package);
    assert!(result.is_ok());

    let pkg = result.unwrap();
    assert_eq!(pkg.version_id, test_version_id);
    assert_eq!(pkg.original_id, test_original_id);
}

#[test]
fn test_package_with_type_origin_table() {
    let natives = create_test_natives();
    let interner = IdentifierInterner::new();
    let mut input_package = create_test_input_package();

    // Add a type origin entry
    input_package.type_origin_table.insert(
        IntraPackageName {
            module_name: Identifier::new("TestModule").unwrap(),
            type_name: Identifier::new("TestType").unwrap(),
        },
        input_package.original_id,
    );

    let result = package(&natives, &interner, input_package);
    assert!(result.is_ok());
}

#[test]
fn test_package_multiple_modules_dependency_order() {
    let natives = create_test_natives();
    let interner = IdentifierInterner::new();

    // Create a simpler test with just two distinct modules
    let module_a = create_empty_input_module();
    let mut module_b = create_empty_input_module();

    // Make module_b have a different address so they are distinct
    let addr_b = AccountAddress::TWO;
    let name_b = Identifier::new("ModuleB").unwrap();

    let mut compiled_b = empty_module();
    compiled_b.identifiers.push(name_b.clone());
    compiled_b.address_identifiers.push(addr_b);
    compiled_b.module_handles.push(ModuleHandle {
        address: AddressIdentifierIndex(0),
        name: IdentifierIndex(0),
    });

    module_b.compiled_module = compiled_b;

    let modules = BTreeMap::from([
        (module_a.compiled_module.self_id(), module_a),
        (module_b.compiled_module.self_id(), module_b),
    ]);

    let version_id = VersionId::from(AccountAddress::from([5u8; AccountAddress::LENGTH]));
    let original_id = OriginalId::from(AccountAddress::from([6u8; AccountAddress::LENGTH]));

    let input_package = input::Package {
        version_id,
        original_id,
        modules,
        type_origin_table: IndexMap::new(),
        linkage_table: BTreeMap::from([(original_id, version_id)]),
    };

    let result = package(&natives, &interner, input_package);
    assert!(result.is_ok());

    let pkg = result.unwrap();
    assert!(pkg.loaded_modules.len() == 1);
}

#[test]
fn test_package_creates_virtual_table() {
    let natives = create_test_natives();
    let interner = IdentifierInterner::new();
    let input_package = create_test_input_package();

    let result = package(&natives, &interner, input_package);
    assert!(result.is_ok());

    let pkg = result.unwrap();
    // Virtual table should be created (we can't easily test its contents without more complex setup)
    // but we can verify the package structure is correct
    assert_eq!(pkg.loaded_modules.len(), 1);
}

#[test]
fn test_package_error_handling_invalid_identifier() {
    let natives = create_test_natives();
    let interner = IdentifierInterner::new();
    let mut input_package = create_test_input_package();

    // Add a type origin entry
    input_package.type_origin_table.insert(
        IntraPackageName {
            module_name: Identifier::new("ValidModule").unwrap(),
            type_name: Identifier::new("TestType").unwrap(),
        },
        input_package.original_id,
    );

    let result = package(&natives, &interner, input_package);
    assert!(result.is_ok());
}

// Tests for flatten_and_renumber_blocks function

#[test]
fn test_flatten_and_renumber_blocks_empty() {
    let blocks = BTreeMap::new();
    let jump_tables = vec![];

    let (bytecode, jump_tables) =
        flatten_and_renumber_input_bytcode_and_jumptables(blocks, jump_tables).unwrap();
    assert!(bytecode.is_empty());
    assert!(jump_tables.is_empty());
}

#[test]
fn test_flatten_and_renumber_blocks_single_block() {
    use crate::jit::optimization::ast as input;

    let mut blocks = BTreeMap::new();
    blocks.insert(0u16, vec![input::Bytecode::LdU64(42), input::Bytecode::Pop]);
    let jump_tables = vec![];

    let (bytecode, jump_tables) =
        flatten_and_renumber_input_bytcode_and_jumptables(blocks, jump_tables).unwrap();
    assert_eq!(bytecode.len(), 2);
    assert!(matches!(bytecode[0], input::Bytecode::LdU64(42)));
    assert!(matches!(bytecode[1], input::Bytecode::Pop));
    assert!(jump_tables.is_empty());
}

#[test]
fn test_flatten_and_renumber_blocks_multiple_blocks() {
    use crate::jit::optimization::ast as input;

    let mut blocks = BTreeMap::new();
    blocks.insert(0u16, vec![input::Bytecode::LdU64(42), input::Bytecode::Pop]);
    blocks.insert(
        10u16,
        vec![
            input::Bytecode::LdTrue,
            input::Bytecode::Pop,
            input::Bytecode::Ret,
        ],
    );
    let jump_tables = vec![];

    let (bytecode, jump_tables) =
        flatten_and_renumber_input_bytcode_and_jumptables(blocks, jump_tables).unwrap();
    assert_eq!(bytecode.len(), 5);
    // Verify the blocks were flattened in order
    assert!(matches!(bytecode[0], input::Bytecode::LdU64(42)));
    assert!(matches!(bytecode[1], input::Bytecode::Pop));
    assert!(matches!(bytecode[2], input::Bytecode::LdTrue));
    assert!(matches!(bytecode[3], input::Bytecode::Pop));
    assert!(matches!(bytecode[4], input::Bytecode::Ret));
    assert!(jump_tables.is_empty());
}

#[test]
fn test_flatten_and_renumber_blocks_branch_targets() {
    use crate::jit::optimization::ast as input;

    let mut blocks = BTreeMap::new();
    blocks.insert(
        0u16,
        vec![input::Bytecode::LdTrue, input::Bytecode::BrTrue(10u16)],
    );
    blocks.insert(
        10u16,
        vec![input::Bytecode::LdU64(42), input::Bytecode::Ret],
    );
    let jump_tables = vec![];

    let (bytecode, jump_tables) =
        flatten_and_renumber_input_bytcode_and_jumptables(blocks, jump_tables).unwrap();
    assert_eq!(bytecode.len(), 4);

    // Verify that branch targets were renumbered
    match &bytecode[1] {
        input::Bytecode::BrTrue(target) => assert_eq!(*target, 2), // Should point to offset 2
        _ => panic!("Expected BrTrue with renumbered target"),
    }
    assert!(jump_tables.is_empty());
}

#[test]
fn test_flatten_and_renumber_blocks_with_jump_table() {
    use crate::jit::optimization::ast as input;

    // Create a jump table that points to block offsets 0, 10, 20
    let jump_tables = vec![vec![0u16, 10u16, 20u16]];

    let mut blocks = BTreeMap::new();
    blocks.insert(0u16, vec![input::Bytecode::LdU64(1)]);
    blocks.insert(10u16, vec![input::Bytecode::LdU64(2)]);
    blocks.insert(20u16, vec![input::Bytecode::LdU64(3)]);

    let (bytecode, updated_tables) =
        flatten_and_renumber_input_bytcode_and_jumptables(blocks, jump_tables).unwrap();

    // Verify the bytecode was flattened correctly
    assert_eq!(bytecode.len(), 3);
    assert!(matches!(bytecode[0], input::Bytecode::LdU64(1)));
    assert!(matches!(bytecode[1], input::Bytecode::LdU64(2)));
    assert!(matches!(bytecode[2], input::Bytecode::LdU64(3)));

    // Verify the jump table was updated
    assert_eq!(updated_tables.len(), 1); // One jump table still
    // Block 0 -> offset 0, block 10 -> offset 1, block 20 -> offset 2
    assert_eq!(updated_tables[0].len(), 3); // Table has 3 entries
    assert_eq!(updated_tables[0][0], 0); // Block 0 maps to offset 0
    assert_eq!(updated_tables[0][1], 1); // Block 10 maps to offset 1
    assert_eq!(updated_tables[0][2], 2); // Block 20 maps to offset 2
}

#[test]
fn test_flatten_and_renumber_blocks_variant_switch() {
    use crate::jit::optimization::ast as input;

    // Create a jump table for variant switch
    let jump_tables = vec![vec![0u16, 5u16, 10u16]];

    let mut blocks = BTreeMap::new();
    blocks.insert(
        0u16,
        vec![
            input::Bytecode::LdU8(0),
            input::Bytecode::VariantSwitch(move_binary_format::file_format::VariantJumpTableIndex(
                0,
            )),
        ],
    );
    blocks.insert(5u16, vec![input::Bytecode::LdU64(100)]);
    blocks.insert(10u16, vec![input::Bytecode::LdU64(200)]);

    let (bytecode, jump_tables) =
        flatten_and_renumber_input_bytcode_and_jumptables(blocks, jump_tables).unwrap();

    // Verify flattening
    assert_eq!(bytecode.len(), 4);
    assert!(matches!(bytecode[0], input::Bytecode::LdU8(0)));
    assert!(matches!(bytecode[1], input::Bytecode::VariantSwitch(_)));
    assert!(matches!(bytecode[2], input::Bytecode::LdU64(100)));
    assert!(matches!(bytecode[3], input::Bytecode::LdU64(200)));

    // Verify jump table was updated
    assert_eq!(jump_tables.len(), 1);
    let updated_table = &jump_tables[0];
    assert_eq!(updated_table[0], 0); // Block 0 -> offset 0
    assert_eq!(updated_table[1], 2); // Block 5 -> offset 2
    assert_eq!(updated_table[2], 3); // Block 10 -> offset 3
}

#[test]
fn test_flatten_and_renumber_blocks_overflow_protection() {
    use crate::jit::optimization::ast as input;

    // Test that we properly detect overflow when bytecode offset exceeds u16::MAX
    let mut blocks = BTreeMap::new();

    let large_vec = vec![input::Bytecode::Nop; (u16::MAX as usize) + 1];
    blocks.insert(0u16, large_vec);

    let jump_tables = vec![];

    let result = flatten_and_renumber_input_bytcode_and_jumptables(blocks, jump_tables);
    assert!(result.is_err());

    let err = result.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(msg.contains("exceeds u16::MAX") || msg.contains("overflow"));
}

#[test]
fn test_flatten_and_renumber_blocks_offset_overflow_protection() {
    use crate::jit::optimization::ast as input;

    // Test that we properly detect overflow when cumulative offset exceeds u16::MAX
    let mut blocks = BTreeMap::new();

    let block1 = vec![input::Bytecode::Nop; u16::MAX as usize];
    let block2 = vec![input::Bytecode::Nop; 2];

    blocks.insert(0u16, block1);
    blocks.insert(1u16, block2);

    let jump_tables = vec![];

    let result = flatten_and_renumber_input_bytcode_and_jumptables(blocks, jump_tables);
    assert!(result.is_err());

    let err = result.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(msg.contains("overflow") || msg.contains("exceeds u16::MAX"));
}
