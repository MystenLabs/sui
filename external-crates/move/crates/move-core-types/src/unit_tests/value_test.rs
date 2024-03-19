// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_address::AccountAddress,
    annotated_value as A, ident_str,
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
    runtime_value as R,
};
use serde_json::json;

#[test]
fn struct_deserialization() {
    let struct_type = StructTag {
        address: AccountAddress::ZERO,
        name: ident_str!("MyStruct").to_owned(),
        module: ident_str!("MyModule").to_owned(),
        type_params: vec![],
    };
    let values = vec![R::MoveValue::U64(7), R::MoveValue::Bool(true)];
    let avalues = vec![A::MoveValue::U64(7), A::MoveValue::Bool(true)];
    let fields = vec![ident_str!("f").to_owned(), ident_str!("g").to_owned()];
    let field_values: Vec<(Identifier, A::MoveValue)> =
        fields.into_iter().zip(avalues.clone()).collect();

    // test each deserialization scheme
    let runtime_value = R::MoveStruct(values);
    assert_eq!(
        serde_json::to_value(runtime_value).unwrap(),
        json!([7, true])
    );

    let typed_value = A::MoveStruct::new(struct_type, field_values);
    assert_eq!(
        serde_json::to_value(&typed_value).unwrap(),
        json!({
                "fields": { "f": 7, "g": true },
                "type": "0x0::MyModule::MyStruct"
            }
        )
    );
}

#[test]
fn struct_formatted_display() {
    let struct_type = StructTag {
        address: AccountAddress::ZERO,
        name: ident_str!("MyStruct").to_owned(),
        module: ident_str!("MyModule").to_owned(),
        type_params: vec![],
    };
    let values = vec![R::MoveValue::U64(7), R::MoveValue::Bool(true)];
    let avalues = vec![A::MoveValue::U64(7), A::MoveValue::Bool(true)];
    let fields = vec![ident_str!("f").to_owned(), ident_str!("g").to_owned()];
    let field_values: Vec<(Identifier, A::MoveValue)> =
        fields.into_iter().zip(avalues.clone()).collect();

    // test each deserialization scheme
    let runtime_value = R::MoveStruct(values);
    assert_eq!(
        serde_json::to_value(runtime_value).unwrap(),
        json!([7, true])
    );

    let typed_value = A::MoveStruct::new(struct_type, field_values);
    assert_eq!(
        format!("{:#}", typed_value),
        r#"0x0::MyModule::MyStruct {
    f: 7u64,
    g: true,
}"#
    );
}

/// A test which verifies that the BCS representation of
/// a struct with a single field is equivalent to the BCS
/// of the value in this field. It also tests
/// that BCS serialization of utf8 strings is equivalent
/// to the BCS serialization of vector<u8> of the bytes of
/// the string.
#[test]
fn struct_one_field_equiv_value() {
    let val = R::MoveValue::Vector(vec![
        R::MoveValue::U8(1),
        R::MoveValue::U8(22),
        R::MoveValue::U8(13),
        R::MoveValue::U8(99),
    ]);
    let s1 = R::MoveValue::Struct(R::MoveStruct(vec![val.clone()]))
        .simple_serialize()
        .unwrap();
    let s2 = val.simple_serialize().unwrap();
    assert_eq!(s1, s2);

    let utf8_str = "çå∞≠¢õß∂ƒ∫";
    let vec_u8 = R::MoveValue::Vector(
        utf8_str
            .as_bytes()
            .iter()
            .map(|c| R::MoveValue::U8(*c))
            .collect(),
    );
    assert_eq!(
        bcs::to_bytes(utf8_str).unwrap(),
        vec_u8.simple_serialize().unwrap()
    )
}

#[test]
fn nested_typed_struct_deserialization() {
    let struct_type = StructTag {
        address: AccountAddress::ZERO,
        name: ident_str!("MyStruct").to_owned(),
        module: ident_str!("MyModule").to_owned(),
        type_params: vec![],
    };
    let nested_struct_type = StructTag {
        address: AccountAddress::ZERO,
        name: ident_str!("NestedStruct").to_owned(),
        module: ident_str!("NestedModule").to_owned(),
        type_params: vec![TypeTag::U8],
    };

    // test each deserialization scheme
    let nested_runtime_struct = R::MoveValue::Struct(R::MoveStruct(vec![R::MoveValue::U64(7)]));
    let runtime_value = R::MoveStruct(vec![nested_runtime_struct]);
    assert_eq!(serde_json::to_value(runtime_value).unwrap(), json!([[7]]));

    let nested_typed_struct = A::MoveValue::Struct(A::MoveStruct::new(
        nested_struct_type,
        vec![(ident_str!("f").to_owned(), A::MoveValue::U64(7))],
    ));
    let typed_value = A::MoveStruct::new(
        struct_type,
        vec![(ident_str!("inner").to_owned(), nested_typed_struct)],
    );
    assert_eq!(
        serde_json::to_value(&typed_value).unwrap(),
        json!({
            "fields": {
                "inner": {
                    "fields": { "f": 7},
                    "type": "0x0::NestedModule::NestedStruct<u8>",
                }
            },
            "type": "0x0::MyModule::MyStruct"
        })
    );
}

#[test]
fn nested_typed_struct_formatted_display() {
    let struct_type = StructTag {
        address: AccountAddress::ZERO,
        name: ident_str!("MyStruct").to_owned(),
        module: ident_str!("MyModule").to_owned(),
        type_params: vec![],
    };
    let nested_struct_type = StructTag {
        address: AccountAddress::ZERO,
        name: ident_str!("NestedStruct").to_owned(),
        module: ident_str!("NestedModule").to_owned(),
        type_params: vec![TypeTag::U8],
    };

    // test each deserialization scheme
    let nested_runtime_struct = R::MoveValue::Struct(R::MoveStruct(vec![R::MoveValue::U64(7)]));
    let runtime_value = R::MoveStruct(vec![nested_runtime_struct]);
    assert_eq!(serde_json::to_value(runtime_value).unwrap(), json!([[7]]));

    let nested_typed_struct = A::MoveValue::Struct(A::MoveStruct::new(
        nested_struct_type,
        vec![(ident_str!("f").to_owned(), A::MoveValue::U64(7))],
    ));
    let typed_value = A::MoveStruct::new(
        struct_type,
        vec![(ident_str!("inner").to_owned(), nested_typed_struct)],
    );
    assert_eq!(
        format!("{:#}", typed_value),
        r#"0x0::MyModule::MyStruct {
    inner: 0x0::NestedModule::NestedStruct<u8> {
        f: 7u64,
    },
}"#
    );
}
