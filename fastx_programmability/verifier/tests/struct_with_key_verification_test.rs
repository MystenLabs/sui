// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

mod module_builder;

use fastx_verifier::struct_with_key_verifier::verify_module;
pub use module_builder::module_builder::ModuleBuilder;
use move_binary_format::file_format::*;
use move_core_types::account_address::AccountAddress;

const ID_STRUCT: StructHandleIndex = StructHandleIndex(0);

fn make_module_with_id_struct() -> ModuleBuilder {
    let mut module = ModuleBuilder::default();
    module.add_struct(
        module.get_self_index(),
        "ID",
        AbilitySet::EMPTY | Ability::Store | Ability::Drop,
        vec![],
    );
    module
}

#[test]
fn key_struct_with_drop() {
    let mut module = make_module_with_id_struct();
    let id_field = module.create_field("id", SignatureToken::Struct(ID_STRUCT));
    module.add_struct(
        module.get_self_index(),
        "S",
        AbilitySet::EMPTY | Ability::Key | Ability::Drop,
        vec![id_field],
    );
    assert!(verify_module(module.get_module())
        .unwrap_err()
        .to_string()
        .contains("Struct S cannot have both key and drop abilities"));
}

#[test]
fn non_key_struct_without_fields() {
    let mut module = make_module_with_id_struct();
    module.add_struct(module.get_self_index(), "S", AbilitySet::EMPTY, vec![]);
    assert!(verify_module(module.get_module()).is_ok());
}

#[test]
fn key_struct_without_fields() {
    let mut module = make_module_with_id_struct();
    module.add_struct(
        module.get_self_index(),
        "S",
        AbilitySet::EMPTY | Ability::Key,
        vec![],
    );
    assert!(verify_module(module.get_module())
        .unwrap_err()
        .to_string()
        .contains("First field of struct S must be 'id', no field found"));
}

#[test]
fn key_struct_first_field_not_id() {
    let mut module = make_module_with_id_struct();
    let foo_field = module.create_field("foo", SignatureToken::Struct(ID_STRUCT));
    module.add_struct(
        module.get_self_index(),
        "S",
        AbilitySet::EMPTY | Ability::Key,
        vec![foo_field],
    );
    assert!(verify_module(module.get_module())
        .unwrap_err()
        .to_string()
        .contains("First field of struct S must be 'id', foo found"));
}

#[test]
fn key_struct_second_field_id() {
    let mut module = make_module_with_id_struct();
    let foo_field = module.create_field("foo", SignatureToken::Struct(ID_STRUCT));
    let id_field = module.create_field("id", SignatureToken::Struct(ID_STRUCT));
    module.add_struct(
        module.get_self_index(),
        "S",
        AbilitySet::EMPTY | Ability::Key,
        vec![foo_field, id_field],
    );
    assert!(verify_module(module.get_module())
        .unwrap_err()
        .to_string()
        .contains("First field of struct S must be 'id', foo found"));
}

#[test]
fn key_struct_id_field_incorrect_type() {
    let mut module = make_module_with_id_struct();
    let id_field = module.create_field("id", SignatureToken::U64);
    module.add_struct(
        module.get_self_index(),
        "S",
        AbilitySet::EMPTY | Ability::Key,
        vec![id_field],
    );
    assert!(verify_module(module.get_module())
        .unwrap_err()
        .to_string()
        .contains("First field of struct S must be of ID type, U64 type found"));
}

#[test]
fn key_struct_id_field_incorrect_struct_address() {
    let mut module = make_module_with_id_struct();
    let new_module_idx =
        module.add_module(AccountAddress::new([1u8; AccountAddress::LENGTH]), "ID");
    let (fake_id_struct, _) = module.add_struct(
        new_module_idx,
        "ID",
        AbilitySet::EMPTY | Ability::Store | Ability::Drop,
        vec![],
    );
    let id_field = module.create_field("id", SignatureToken::Struct(fake_id_struct));
    module.add_struct(
        new_module_idx,
        "S",
        AbilitySet::EMPTY | Ability::Key,
        vec![id_field],
    );
    assert!(verify_module(module.get_module()).unwrap_err().to_string().contains("First field of struct S must be of type 00000000000000000000000000000002::ID::ID, 01010101010101010101010101010101::ID::ID type found"));
}

#[test]
fn key_struct_id_field_incorrect_struct_name() {
    let mut module = make_module_with_id_struct();
    let (fake_id_struct, _) = module.add_struct(
        module.get_self_index(),
        "FOO",
        AbilitySet::EMPTY | Ability::Store | Ability::Drop,
        vec![],
    );
    let id_field = module.create_field("id", SignatureToken::Struct(fake_id_struct));
    module.add_struct(
        module.get_self_index(),
        "S",
        AbilitySet::EMPTY | Ability::Key,
        vec![id_field],
    );
    assert!(verify_module(module.get_module()).unwrap_err().to_string().contains("First field of struct S must be of type 00000000000000000000000000000002::ID::ID, 00000000000000000000000000000002::ID::FOO type found"));
}

#[test]
fn key_struct_id_field_valid() {
    let mut module = make_module_with_id_struct();
    let id_field = module.create_field("id", SignatureToken::Struct(ID_STRUCT));
    module.add_struct(
        module.get_self_index(),
        "S",
        AbilitySet::EMPTY | Ability::Key,
        vec![id_field],
    );
    assert!(verify_module(module.get_module()).is_ok());
}
