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
    let ser = R::MoveValue::Struct(runtime_value.clone())
        .simple_serialize()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&runtime_value).unwrap(),
        json!([7, true])
    );

    let struct_type_layout = A::MoveStructLayout {
        type_: struct_type.clone(),
        fields: vec![
            A::MoveFieldLayout::new(ident_str!("f").to_owned(), A::MoveTypeLayout::U64),
            A::MoveFieldLayout::new(ident_str!("g").to_owned(), A::MoveTypeLayout::Bool),
        ]
        .into_iter()
        .collect(),
    };

    let deser_typed_value =
        A::MoveValue::simple_deserialize(&ser, &A::MoveTypeLayout::Struct(struct_type_layout))
            .unwrap();
    let typed_value = A::MoveStruct::new(struct_type, field_values);

    assert_eq!(
        serde_json::to_value(&typed_value).unwrap(),
        json!({
                "fields": { "f": 7, "g": true },
                "type": "0x0::MyModule::MyStruct"
            }
        )
    );

    assert_eq!(deser_typed_value, A::MoveValue::Struct(typed_value));
}

#[test]
fn enum_deserialization() {
    let enum_type = StructTag {
        address: AccountAddress::ZERO,
        name: ident_str!("MyEnum").to_owned(),
        module: ident_str!("MyModule").to_owned(),
        type_params: vec![],
    };

    let values1 = vec![A::MoveValue::U64(7), A::MoveValue::Bool(true)];
    let fields1 = vec![ident_str!("f").to_owned(), ident_str!("g").to_owned()];
    let field_values1: Vec<(Identifier, A::MoveValue)> =
        fields1.into_iter().zip(values1.clone()).collect();

    let values2 = vec![
        A::MoveValue::U64(8),
        A::MoveValue::Bool(false),
        A::MoveValue::U8(0),
    ];
    let fields2 = vec![
        ident_str!("f2").to_owned(),
        ident_str!("g2").to_owned(),
        ident_str!("h2").to_owned(),
    ];
    let field_values2: Vec<(Identifier, A::MoveValue)> =
        fields2.into_iter().zip(values2.clone()).collect();

    let enum_runtime_layout = {
        let variant_layout1 = vec![R::MoveTypeLayout::U64, R::MoveTypeLayout::Bool];
        let variant_layout2 = vec![
            R::MoveTypeLayout::U64,
            R::MoveTypeLayout::Bool,
            R::MoveTypeLayout::U8,
        ];
        let enum_layout = R::MoveEnumLayout(vec![variant_layout1, variant_layout2]);
        R::MoveTypeLayout::Enum(enum_layout)
    };

    // test each deserialization scheme
    let runtime_value = R::MoveVariant {
        tag: 0,
        fields: values1
            .clone()
            .into_iter()
            .map(|v| v.undecorate())
            .collect(),
    };
    let v = serde_json::to_value(&runtime_value).unwrap();
    assert_eq!(v, json!([0, [7, true]]));

    let ser = R::MoveValue::Variant(runtime_value.clone())
        .simple_serialize()
        .unwrap();
    assert_eq!(
        R::MoveValue::simple_deserialize(&ser, &enum_runtime_layout).unwrap(),
        R::MoveValue::Variant(runtime_value),
    );

    let enum_type_layout = A::MoveEnumLayout {
        type_: enum_type.clone(),
        variants: vec![
            (
                (ident_str!("Variant1").to_owned(), 0u16),
                vec![
                    A::MoveFieldLayout::new(ident_str!("f").to_owned(), A::MoveTypeLayout::U64),
                    A::MoveFieldLayout::new(ident_str!("g").to_owned(), A::MoveTypeLayout::Bool),
                ],
            ),
            (
                (ident_str!("Variant2").to_owned(), 1u16),
                vec![
                    A::MoveFieldLayout::new(ident_str!("f2").to_owned(), A::MoveTypeLayout::U64),
                    A::MoveFieldLayout::new(ident_str!("g2").to_owned(), A::MoveTypeLayout::Bool),
                    A::MoveFieldLayout::new(ident_str!("h2").to_owned(), A::MoveTypeLayout::U8),
                ],
            ),
        ]
        .into_iter()
        .collect(),
    };

    let runtime_value = R::MoveVariant {
        tag: 1,
        fields: values2
            .clone()
            .into_iter()
            .map(|v| v.undecorate())
            .collect(),
    };
    assert_eq!(
        serde_json::to_value(&runtime_value).unwrap(),
        json!([1, [8, false, 0]])
    );

    let deser_typed_value =
        A::MoveValue::simple_deserialize(&ser, &A::MoveTypeLayout::Enum(enum_type_layout.clone()))
            .unwrap();
    let typed_value = A::MoveVariant {
        type_: enum_type.clone(),
        variant_name: ident_str!("Variant1").to_owned(),
        tag: 0,
        fields: field_values1,
    };
    assert_eq!(
        serde_json::to_value(&typed_value).unwrap(),
        json!({
            "type": "0x0::MyModule::MyEnum",
            "variant_name": "Variant1",
            "variant_tag": 0,
            "fields": {
                "f": 7,
                "g": true,
            }
        })
    );
    assert_eq!(deser_typed_value, A::MoveValue::Variant(typed_value));

    let ser1 = R::MoveValue::Variant(runtime_value.clone())
        .simple_serialize()
        .unwrap();
    let deser1_typed_value =
        A::MoveValue::simple_deserialize(&ser1, &A::MoveTypeLayout::Enum(enum_type_layout))
            .unwrap();
    let typed_value = A::MoveVariant {
        type_: enum_type,
        variant_name: ident_str!("Variant2").to_owned(),
        tag: 1,
        fields: field_values2,
    };

    assert_eq!(
        serde_json::to_value(&typed_value).unwrap(),
        json!({
            "type": "0x0::MyModule::MyEnum",
            "variant_name": "Variant2",
            "variant_tag": 1,
            "fields": {
                "f2": 8,
                "g2": false,
                "h2": 0
            }
        })
    );
    assert_eq!(deser1_typed_value, A::MoveValue::Variant(typed_value));
}

#[test]
fn enum_deserialization_vec_option_runtime_layout_equiv() {
    let value = vec![R::MoveValue::U64(42)];
    let vec_option = R::MoveValue::Struct(R::MoveStruct(vec![R::MoveValue::Vector(value.clone())]));
    let enum_option = R::MoveValue::Variant(R::MoveVariant {
        tag: 1,
        fields: value,
    });

    let vec_ser = vec_option.simple_serialize().unwrap();
    let enum_ser = enum_option.simple_serialize().unwrap();

    let enum_layout = R::MoveTypeLayout::Enum(R::MoveEnumLayout(vec![
        vec![],
        vec![R::MoveTypeLayout::U64],
    ]));

    let vec_layout = R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::U64));

    // Sanity check -- we can deserialize into the original type layout
    R::MoveValue::simple_deserialize(&vec_ser, &vec_layout).unwrap();
    let enum_val = R::MoveValue::simple_deserialize(&enum_ser, &enum_layout).unwrap();
    let enum_vec_val = R::MoveValue::simple_deserialize(&vec_ser, &enum_layout).unwrap();

    assert_eq!(vec_ser, enum_ser);
    // The deserialized values should be equal
    assert_eq!(enum_val, enum_vec_val);
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
    assert_eq!(serde_json::to_value(&runtime_value).unwrap(), json!([[7]]));

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
