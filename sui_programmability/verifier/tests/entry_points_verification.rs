// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod common;

pub use common::module_builder::{ModuleBuilder, StructInfo};
use move_binary_format::{
    binary_views::BinaryIndexedView,
    file_format::{
        Ability, AbilitySet, Bytecode, CodeUnit, SignatureIndex, SignatureToken,
        StructTypeParameter, Visibility,
    },
    CompiledModule,
};
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Spanned;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_verifier::entry_points_verifier::verify_module;

fn add_function(
    builder: &mut ModuleBuilder,
    name: &str,
    parameters: Vec<SignatureToken>,
    ret: Vec<SignatureToken>,
    type_parameters: Vec<AbilitySet>,
) {
    builder.add_function_verbose(
        builder.get_self_index(),
        name,
        parameters,
        ret,
        type_parameters,
        Visibility::Script,
        CodeUnit {
            locals: SignatureIndex(1), // module_builder has "void" signature at 0
            code: vec![Bytecode::Ret], // need some code otherwise will be considered native
        },
    );
}

fn add_tx_context(builder: &mut ModuleBuilder) -> StructInfo {
    let addr_module_idx = builder.add_module(SUI_FRAMEWORK_ADDRESS, "Address");
    let addr = builder.add_struct(
        addr_module_idx,
        "Address",
        AbilitySet::EMPTY | Ability::Copy | Ability::Drop | Ability::Store,
        vec![(
            "bytes",
            SignatureToken::Vector(Box::new(SignatureToken::U8)),
        )],
    );
    let signer = builder.add_struct(
        addr_module_idx,
        "Signer",
        AbilitySet::EMPTY | Ability::Drop,
        vec![(
            "inner",
            SignatureToken::Vector(Box::new(SignatureToken::Struct(addr.handle))),
        )],
    );

    let tx_context_module_idx = builder.add_module(SUI_FRAMEWORK_ADDRESS, "TxContext");
    builder.add_struct(
        tx_context_module_idx,
        "TxContext",
        AbilitySet::EMPTY | Ability::Drop,
        vec![
            ("signer", SignatureToken::Struct(signer.handle)),
            (
                "tx_hash",
                SignatureToken::Vector(Box::new(SignatureToken::U8)),
            ),
            ("ids_created", SignatureToken::U64),
        ],
    )
}

fn tx_context_param(tx_context: StructInfo) -> SignatureToken {
    SignatureToken::MutableReference(Box::new(SignatureToken::Struct(tx_context.handle)))
}

fn format_assert_msg(module: &CompiledModule) -> String {
    let view = BinaryIndexedView::Module(module);
    let d = Disassembler::from_view(view, Spanned::unsafe_no_loc(()).loc);
    format!(
        "verification failed for the following module:\n{}",
        d.unwrap().disassemble().unwrap()
    )
}

#[test]
fn single_param() {
    /*
    public foo<Ty0>(loc0: Ty0, loc1: &mut TxContext): u64 {
    }

    it's a valid entry function, loc0 is assumed to be primitive
    */
    let (mut builder, _) = ModuleBuilder::default();

    let tx_context = add_tx_context(&mut builder);

    add_function(
        &mut builder,
        "foo",
        vec![
            SignatureToken::TypeParameter(0),
            tx_context_param(tx_context),
        ],
        vec![],
        vec![AbilitySet::EMPTY],
    );

    let module = builder.get_module();
    assert!(
        verify_module(module).is_ok(),
        "{}",
        format_assert_msg(module)
    );
}

#[test]
fn single_param_key() {
    /*
    public foo<Ty0: key>(loc0: Ty0, loc1: &mut TxContext) {
    }

    it's a valid entry function and verification should SUCCEED
    */
    let (mut builder, _) = ModuleBuilder::default();

    let tx_context = add_tx_context(&mut builder);

    add_function(
        &mut builder,
        "foo",
        vec![
            SignatureToken::TypeParameter(0),
            tx_context_param(tx_context),
        ],
        vec![],
        vec![AbilitySet::EMPTY | Ability::Key],
    );

    let module = builder.get_module();
    assert!(
        verify_module(module).is_ok(),
        "{}",
        format_assert_msg(module)
    );
}

#[test]
fn single_template_object_param() {
    /*
    struct ObjStruct<Ty0> has key

    public foo<Ty0>(loc0: ObjStruct<Ty0>, loc1: &mut TxContext) {
    }

    it's a valid entry function and verification should SUCCEED
    */
    let (mut builder, _) = ModuleBuilder::default();

    let tx_context = add_tx_context(&mut builder);

    let obj_struct = builder.add_struct_verbose(
        builder.get_self_index(),
        "ObjStruct",
        AbilitySet::EMPTY | Ability::Key,
        vec![],
        vec![StructTypeParameter {
            constraints: AbilitySet::EMPTY,
            is_phantom: false,
        }],
    );

    add_function(
        &mut builder,
        "foo",
        vec![
            SignatureToken::StructInstantiation(
                obj_struct.handle,
                vec![SignatureToken::TypeParameter(0)],
            ),
            tx_context_param(tx_context),
        ],
        vec![],
        vec![AbilitySet::EMPTY | Ability::Store],
    );

    let module = builder.get_module();
    assert!(
        verify_module(module).is_ok(),
        "{}",
        format_assert_msg(module)
    );
}

#[test]
fn template_and_template_object_params() {
    /*
    struct ObjStruct<Ty0> has key

    public foo<Ty0: key, Ty1: store>(loc0: Ty0, loc1: ObjStruct<Ty1>, loc2: &mut TxContext) {
    }

    it's a valid entry function and verification should SUCCEED
    */
    let (mut builder, _) = ModuleBuilder::default();

    let tx_context = add_tx_context(&mut builder);

    let obj_struct = builder.add_struct_verbose(
        builder.get_self_index(),
        "ObjStruct",
        AbilitySet::EMPTY | Ability::Key,
        vec![],
        vec![StructTypeParameter {
            constraints: AbilitySet::EMPTY,
            is_phantom: false,
        }],
    );

    add_function(
        &mut builder,
        "foo",
        vec![
            SignatureToken::TypeParameter(0),
            SignatureToken::StructInstantiation(
                obj_struct.handle,
                vec![SignatureToken::TypeParameter(1)],
            ),
            tx_context_param(tx_context),
        ],
        vec![],
        vec![
            AbilitySet::EMPTY | Ability::Key,
            AbilitySet::EMPTY | Ability::Store,
        ],
    );

    let module = builder.get_module();
    assert!(
        verify_module(module).is_ok(),
        "{}",
        format_assert_msg(module)
    );
}

#[test]
fn template_param_after_primitive() {
    /*
    struct ObjStruct has key

    public foo<Ty0>(loc0: ObjStruct, loc1: u64, loc2: Ty0, loc3: &mut TxContext) {
    }

    it is a valid entry function and verification should SUCCEED
    */
    let (mut builder, _) = ModuleBuilder::default();

    let tx_context = add_tx_context(&mut builder);

    let obj_struct = builder.add_struct(
        builder.get_self_index(),
        "ObjStruct",
        AbilitySet::EMPTY | Ability::Key,
        vec![],
    );

    add_function(
        &mut builder,
        "foo",
        vec![
            SignatureToken::Struct(obj_struct.handle),
            SignatureToken::U64,
            SignatureToken::TypeParameter(0),
            tx_context_param(tx_context),
        ],
        vec![],
        vec![AbilitySet::EMPTY],
    );

    let module = builder.get_module();
    assert!(
        verify_module(module).is_ok(),
        "{}",
        format_assert_msg(module)
    );
}

#[test]
fn single_template_vector_param() {
    /*
    public foo<Ty0>(loc0: vector<Ty0>, loc1: &mut TxContext) {
    }

    it's a valid entry function, loc0 is assumed to be primitive
    */
    let (mut builder, _) = ModuleBuilder::default();

    let tx_context = add_tx_context(&mut builder);

    add_function(
        &mut builder,
        "foo",
        vec![
            SignatureToken::Vector(Box::new(SignatureToken::TypeParameter(0))),
            tx_context_param(tx_context),
        ],
        vec![],
        vec![AbilitySet::EMPTY],
    );

    let module = builder.get_module();
    assert!(
        verify_module(module).is_ok(),
        "{}",
        format_assert_msg(module)
    );
}

#[test]
fn nested_template_vector_param() {
    /*
    public foo<Ty0>(loc0: vector<vector<Ty0>>, loc1: &mut TxContext) {
    }

    it's a valid entry function. It is assumed loc0 will be primitives, not objects
    */
    let (mut builder, _) = ModuleBuilder::default();

    let tx_context = add_tx_context(&mut builder);

    add_function(
        &mut builder,
        "foo",
        vec![
            SignatureToken::Vector(Box::new(SignatureToken::Vector(Box::new(
                SignatureToken::TypeParameter(0),
            )))),
            tx_context_param(tx_context),
        ],
        vec![],
        vec![AbilitySet::EMPTY],
    );

    let module = builder.get_module();
    assert!(
        verify_module(module).is_ok(),
        "{}",
        format_assert_msg(module)
    );
}

#[test]
fn single_template_vector_param_key() {
    /*
    public foo<Ty0: key>(loc0: vector<Ty0>, loc1: &mut TxContext) {
    }

    it's a valid entry function and verification should FAIL due to
    missing Key ability on the vector's generic type
    */
    let (mut builder, _) = ModuleBuilder::default();

    let tx_context = add_tx_context(&mut builder);

    add_function(
        &mut builder,
        "foo",
        vec![
            SignatureToken::Vector(Box::new(SignatureToken::TypeParameter(0))),
            tx_context_param(tx_context),
        ],
        vec![],
        vec![AbilitySet::EMPTY | Ability::Key],
    );

    let module = builder.get_module();
    assert!(
        verify_module(module).is_ok(),
        "{}",
        format_assert_msg(module)
    );
}

#[test]
fn nested_template_vector_param_key() {
    /*
    public foo<Ty0: key>(loc0: vector<vector<Ty0>>, loc1: &mut TxContext) {
    }

    it's a valid entry function and verification should SUCCEED
    */
    let (mut builder, _) = ModuleBuilder::default();

    let tx_context = add_tx_context(&mut builder);

    add_function(
        &mut builder,
        "foo",
        vec![
            SignatureToken::Vector(Box::new(SignatureToken::Vector(Box::new(
                SignatureToken::TypeParameter(0),
            )))),
            tx_context_param(tx_context),
        ],
        vec![],
        vec![AbilitySet::EMPTY | Ability::Key],
    );

    let module = builder.get_module();
    assert!(
        verify_module(module).is_ok(),
        "{}",
        format_assert_msg(module)
    );
}

#[test]
fn return_values() {
    let (mut builder, _) = ModuleBuilder::default();

    add_function(
        &mut builder,
        "foo",
        vec![],
        vec![SignatureToken::U64],
        vec![],
    );
    let module = builder.get_module();
    assert!(
        verify_module(module).is_err(),
        "{}",
        format_assert_msg(module)
    );

    add_function(
        &mut builder,
        "foo",
        vec![],
        vec![SignatureToken::U64, SignatureToken::U8],
        vec![],
    );
    let module = builder.get_module();
    assert!(
        verify_module(module).is_err(),
        "{}",
        format_assert_msg(module)
    );

    add_function(
        &mut builder,
        "foo",
        vec![],
        vec![SignatureToken::Vector(Box::new(SignatureToken::U8))],
        vec![],
    );
    let module = builder.get_module();
    assert!(
        verify_module(module).is_err(),
        "{}",
        format_assert_msg(module)
    );

    add_function(
        &mut builder,
        "foo",
        vec![],
        vec![SignatureToken::Reference(Box::new(SignatureToken::U8))],
        vec![],
    );
    let module = builder.get_module();
    assert!(
        verify_module(module).is_err(),
        "{}",
        format_assert_msg(module)
    );
}
