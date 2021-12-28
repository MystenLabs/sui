// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

mod module_builder;

use fastx_verifier::id_leak_verifier::verify_module;
pub use module_builder::module_builder::ModuleBuilder;
use move_binary_format::file_format::*;

const ID_STRUCT: StructHandleIndex = StructHandleIndex(0);
const FOO_STRUCT: StructHandleIndex = StructHandleIndex(1);
const FOO_DEF: StructDefinitionIndex = StructDefinitionIndex(1);

fn make_module_with_default_struct() -> ModuleBuilder {
    /*
    We are creating FASTX_FRAMEWORK_ADDRESS::ID module that looks like this:
    struct ID has store, drop {
    }

    struct Foo has key {
        id: FASTX_FRAMEWORK_ADDRESS::ID::ID
    }
    */
    let mut module = ModuleBuilder::default();
    module.add_struct(
        module.get_self_index(),
        "ID",
        AbilitySet::EMPTY | Ability::Store | Ability::Drop,
        vec![],
    );
    let id_field = module.create_field("id", SignatureToken::Struct(ID_STRUCT));
    module.add_struct(
        module.get_self_index(),
        "Foo",
        AbilitySet::EMPTY | Ability::Key,
        vec![id_field],
    );
    module
}

#[test]
fn id_leak_through_direct_return() {
    /*
    fun foo(f: Foo): 0x1::ID::ID {
        let Foo { id: id } = f;
        return id;
    }
    */
    let mut module = make_module_with_default_struct();
    let (_, func_def) = module.add_function(
        module.get_self_index(),
        "foo",
        vec![SignatureToken::Struct(FOO_STRUCT)],
        vec![SignatureToken::Struct(ID_STRUCT)],
    );
    module.set_bytecode(
        func_def,
        vec![
            Bytecode::MoveLoc(0),
            Bytecode::Unpack(FOO_DEF),
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
    let mut module = make_module_with_default_struct();
    let (_, func_def) = module.add_function(
        module.get_self_index(),
        "foo",
        vec![SignatureToken::Struct(FOO_STRUCT)],
        vec![SignatureToken::Struct(FOO_STRUCT)],
    );
    module.set_bytecode(
        func_def,
        vec![
            Bytecode::MoveLoc(0),
            Bytecode::Unpack(FOO_DEF),
            Bytecode::Pack(FOO_DEF),
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
    fun foo(f: Foo, ref: &mut 0x1::ID::ID) {
        let Foo { id: id } = f;
        *ref = id;
    }
    */
    let mut module = make_module_with_default_struct();
    let (_, func_def) = module.add_function(
        module.get_self_index(),
        "foo",
        vec![
            SignatureToken::Struct(FOO_STRUCT),
            SignatureToken::MutableReference(Box::new(SignatureToken::Struct(ID_STRUCT))),
        ],
        vec![],
    );
    module.set_bytecode(
        func_def,
        vec![
            Bytecode::MoveLoc(0),
            Bytecode::Unpack(FOO_DEF),
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
    fun transfer(id: 0x1::ID::ID);

    fun foo(f: Foo) {
        let Foo { id: id } = f;
        transfer(id);
    }
    */
    let mut module = make_module_with_default_struct();
    let (transfer, _) = module.add_function(
        module.get_self_index(),
        "transfer",
        vec![SignatureToken::Struct(ID_STRUCT)],
        vec![],
    );
    let (_, func_def) = module.add_function(
        module.get_self_index(),
        "foo",
        vec![SignatureToken::Struct(FOO_STRUCT)],
        vec![],
    );
    module.set_bytecode(
        func_def,
        vec![
            Bytecode::MoveLoc(0),
            Bytecode::Unpack(FOO_DEF),
            Bytecode::Call(transfer),
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
    let mut module = make_module_with_default_struct();
    let (transfer, _) = module.add_function(
        module.get_self_index(),
        "transfer",
        vec![SignatureToken::Struct(ID_STRUCT)],
        vec![],
    );
    let (_, func_def) = module.add_function(
        module.get_self_index(),
        "foo",
        vec![SignatureToken::Struct(FOO_STRUCT)],
        vec![],
    );
    module.set_bytecode(
        func_def,
        vec![
            Bytecode::MoveLoc(0),
            Bytecode::Unpack(FOO_DEF),
            Bytecode::Pack(FOO_DEF),
            Bytecode::Call(transfer),
        ],
    );
    let result = verify_module(module.get_module());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("ID leak detected in function foo: ID leaked through function call."));
}
