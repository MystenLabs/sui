// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod common;

pub use common::module_builder::ModuleBuilder;
use move_binary_format::file_format::*;
use sui_verifier::id_immutable_verifier::verify_module;

#[test]
fn mut_borrow_key_struct_id_field() {
    /*
    struct Foo has key {
        id: 0x2::ID::VersionedID
    }

    fun foo(f: Foo) {
        let ref = &mut f.id;
    }
    */
    let (mut module, id_struct) = ModuleBuilder::default();
    let foo_struct = module.add_struct(
        module.get_self_index(),
        "Foo",
        AbilitySet::EMPTY | Ability::Key,
        vec![("id", SignatureToken::Struct(id_struct.handle))],
    );
    let foo_func = module.add_function(
        module.get_self_index(),
        "foo",
        vec![SignatureToken::Struct(foo_struct.handle)],
        vec![],
    );
    module.set_bytecode(
        foo_func.def,
        vec![
            Bytecode::MoveLoc(0u8),
            Bytecode::MutBorrowField(foo_struct.fields[0]),
        ],
    );
    assert!(verify_module(module.get_module())
        .unwrap_err()
        .to_string()
        .contains(
        "In function foo: ID field of struct Foo cannot be mut borrowed because ID is immutable"
    ));
}

#[test]
fn mut_borrow_non_key_struct_id_field() {
    /*
    struct Foo {
        id: 0x2::ID::VersionedID
    }
    fun foo(f: Foo) {
        let ref = &mut f.id;
    }
    */
    let (mut module, id_struct) = ModuleBuilder::default();
    let foo_struct = module.add_struct(
        module.get_self_index(),
        "Foo",
        AbilitySet::EMPTY,
        vec![("id", SignatureToken::Struct(id_struct.handle))],
    );
    let foo_func = module.add_function(
        module.get_self_index(),
        "foo",
        vec![SignatureToken::Struct(foo_struct.handle)],
        vec![],
    );
    module.set_bytecode(
        foo_func.def,
        vec![
            Bytecode::MoveLoc(0u8),
            Bytecode::MutBorrowField(foo_struct.fields[0]),
        ],
    );
    assert!(verify_module(module.get_module()).is_ok());
}

#[test]
fn mut_borrow_key_struct_non_id_field() {
    /*
    struct Foo has key {
        id: 0x2::ID::VersionedID,
        other: 0x2::ID::VersionedID
    }
    fun foo(f: Foo) {
        let ref = &mut f.other;
    }
    */
    let (mut module, id_struct) = ModuleBuilder::default();
    let foo_struct = module.add_struct(
        module.get_self_index(),
        "Foo",
        AbilitySet::EMPTY | Ability::Key,
        vec![
            ("id", SignatureToken::Struct(id_struct.handle)),
            ("other", SignatureToken::Struct(id_struct.handle)),
        ],
    );
    let foo_func = module.add_function(
        module.get_self_index(),
        "foo",
        vec![SignatureToken::Struct(foo_struct.handle)],
        vec![],
    );
    module.set_bytecode(
        foo_func.def,
        vec![
            Bytecode::MoveLoc(0u8),
            Bytecode::MutBorrowField(foo_struct.fields[1]),
        ],
    );
    assert!(verify_module(module.get_module()).is_ok());
}

#[test]
fn mut_borrow_generic_key_struct_id_field() {
    /*
    struct Foo<T> has key {
        id: 0x2::ID::VersionedID
    }

    fun foo(f: Foo<u64>) {
        let ref = &mut f.id;
    }
    */
    let (mut module, id_struct) = ModuleBuilder::default();
    let foo_struct = module.add_struct(
        module.get_self_index(),
        "Foo",
        AbilitySet::EMPTY | Ability::Key,
        vec![("id", SignatureToken::Struct(id_struct.handle))],
    );
    let inst = module.add_field_instantiation(foo_struct.fields[0], vec![SignatureToken::U64]);
    let foo_func = module.add_function(
        module.get_self_index(),
        "foo",
        vec![SignatureToken::Struct(foo_struct.handle)],
        vec![],
    );
    module.set_bytecode(
        foo_func.def,
        vec![
            Bytecode::MoveLoc(0u8),
            Bytecode::MutBorrowFieldGeneric(inst),
        ],
    );
    assert!(verify_module(module.get_module())
        .unwrap_err()
        .to_string()
        .contains(
        "In function foo: ID field of struct Foo cannot be mut borrowed because ID is immutable"
    ));
}
