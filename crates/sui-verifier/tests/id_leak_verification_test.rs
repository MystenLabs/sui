// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod common;

pub use common::module_builder::*;
use move_binary_format::file_format::*;
use sui_verifier::id_leak_verifier::verify_module;

fn make_module_with_default_struct() -> (ModuleBuilder, StructInfo, StructInfo) {
    /*
    Creating a module with a default struct Foo:

    struct Foo has key {
        id: SUI_FRAMEWORK_ADDRESS::ID::VersionedID
    }
    */
    let (mut module, id_struct) = ModuleBuilder::default();
    let foo_struct = module.add_struct(
        module.get_self_index(),
        "Foo",
        AbilitySet::EMPTY | Ability::Key,
        vec![("id", SignatureToken::Struct(id_struct.handle))],
    );
    (module, id_struct, foo_struct)
}

#[test]
fn id_leak_through_direct_return() {
    /*
    fun foo(f: Foo): 0x1::ID::VersionedID {
        let Foo { id: id } = f;
        return id;
    }
    */
    let (mut module, id_struct, foo_struct) = make_module_with_default_struct();
    let func = module.add_function(
        module.get_self_index(),
        "foo",
        vec![SignatureToken::Struct(foo_struct.handle)],
        vec![SignatureToken::Struct(id_struct.handle)],
    );
    module.set_bytecode(
        func.def,
        vec![
            Bytecode::MoveLoc(0),
            Bytecode::Unpack(foo_struct.def),
            Bytecode::Ret,
        ],
    );
    let result = verify_module(module.get_module());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("ID leak detected in function foo: ID leaked through function return."));
}

#[test]
fn id_leak_through_indirect_return() {
    /*
    fun foo(f: Foo): Foo {
        let Foo { id: id } = f;
        let r = Foo { id: id };
        return r;
    }
    */
    let (mut module, _, foo_struct) = make_module_with_default_struct();
    let func = module.add_function(
        module.get_self_index(),
        "foo",
        vec![SignatureToken::Struct(foo_struct.handle)],
        vec![SignatureToken::Struct(foo_struct.handle)],
    );
    module.set_bytecode(
        func.def,
        vec![
            Bytecode::MoveLoc(0),
            Bytecode::Unpack(foo_struct.def),
            Bytecode::Pack(foo_struct.def),
            Bytecode::Ret,
        ],
    );
    let result = verify_module(module.get_module());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("ID leak detected in function foo: ID leaked through function return."));
}

#[test]
fn id_leak_through_reference() {
    /*
    fun foo(f: Foo, ref: &mut 0x1::ID::VersionedID) {
        let Foo { id: id } = f;
        *ref = id;
    }
    */
    let (mut module, id_struct, foo_struct) = make_module_with_default_struct();
    let func = module.add_function(
        module.get_self_index(),
        "foo",
        vec![
            SignatureToken::Struct(foo_struct.handle),
            SignatureToken::MutableReference(Box::new(SignatureToken::Struct(id_struct.handle))),
        ],
        vec![],
    );
    module.set_bytecode(
        func.def,
        vec![
            Bytecode::MoveLoc(0),
            Bytecode::Unpack(foo_struct.def),
            Bytecode::MoveLoc(1),
            Bytecode::WriteRef,
            Bytecode::Ret,
        ],
    );
    let result = verify_module(module.get_module());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("ID leak detected in function foo: ID is leaked to a reference."));
}

#[test]
fn id_direct_leak_through_call() {
    /*
    fun transfer(id: 0x1::ID::VersionedID);

    fun foo(f: Foo) {
        let Foo { id: id } = f;
        transfer(id);
    }
    */
    let (mut module, id_struct, foo_struct) = make_module_with_default_struct();
    let transfer_func = module.add_function(
        module.get_self_index(),
        "transfer",
        vec![SignatureToken::Struct(id_struct.handle)],
        vec![],
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
            Bytecode::MoveLoc(0),
            Bytecode::Unpack(foo_struct.def),
            Bytecode::Call(transfer_func.handle),
        ],
    );
    let result = verify_module(module.get_module());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("ID leak detected in function foo: ID leaked through function call."));
}

#[test]
fn id_indirect_leak_through_call() {
    /*
    fun transfer(f: Foo);

    fun foo(f: Foo) {
        let Foo { id: id } = f;
        let newf = Foo { id: id };
        transfer(newf);
    }
    */
    let (mut module, id_struct, foo_struct) = make_module_with_default_struct();
    let transfer_func = module.add_function(
        module.get_self_index(),
        "transfer",
        vec![SignatureToken::Struct(id_struct.handle)],
        vec![],
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
            Bytecode::MoveLoc(0),
            Bytecode::Unpack(foo_struct.def),
            Bytecode::Pack(foo_struct.def),
            Bytecode::Call(transfer_func.handle),
        ],
    );
    let result = verify_module(module.get_module());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("ID leak detected in function foo: ID leaked through function call."));
}
