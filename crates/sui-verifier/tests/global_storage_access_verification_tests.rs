// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod common;

pub use common::module_builder::ModuleBuilder;
use move_binary_format::file_format::*;
use sui_verifier::global_storage_access_verifier::verify_module;

#[test]
fn function_with_global_access_bytecode() {
    let (mut module, _) = ModuleBuilder::default();
    let func = module.add_function(module.get_self_index(), "foo", vec![], vec![]);
    assert!(verify_module(module.get_module()).is_ok());

    // All the bytecode that could access global storage.
    let mut code = vec![
        Bytecode::Exists(StructDefinitionIndex(0)),
        Bytecode::ImmBorrowGlobal(StructDefinitionIndex(0)),
        Bytecode::ImmBorrowGlobalGeneric(StructDefInstantiationIndex(0)),
        Bytecode::MoveFrom(StructDefinitionIndex(0)),
        Bytecode::MoveFromGeneric(StructDefInstantiationIndex(0)),
        Bytecode::MoveTo(StructDefinitionIndex(0)),
        Bytecode::MoveToGeneric(StructDefInstantiationIndex(0)),
        Bytecode::MutBorrowGlobal(StructDefinitionIndex(0)),
        Bytecode::MutBorrowGlobalGeneric(StructDefInstantiationIndex(0)),
    ];
    let invalid_bytecode_str = format!("{:?}", code);
    // Add a few valid bytecode that doesn't access global storage.
    code.extend(vec![
        Bytecode::Add,
        Bytecode::ImmBorrowField(FieldHandleIndex(0)),
        Bytecode::Call(FunctionHandleIndex(0)),
    ]);
    module.set_bytecode(func.def, code);
    assert!(verify_module(module.get_module())
        .unwrap_err()
        .to_string()
        .contains(&format!(
            "Access to Move global storage is not allowed. Found in function foo: {}",
            invalid_bytecode_str
        )));
}
