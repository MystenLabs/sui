// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::str::FromStr;

use fastcrypto::encoding::{Encoding, Hex};
use move_core_types::annotated_value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout};
use move_core_types::language_storage::StructTag;
use move_core_types::u256::U256;
use move_core_types::{account_address::AccountAddress, ident_str, identifier::Identifier};
use serde::Serialize;
use serde_json::{json, Value};
use test_fuzz::runtime::num_traits::ToPrimitive;

use sui_framework::BuiltInFramework;
use sui_move_build::BuildConfig;
use sui_types::base_types::{
    ObjectID, SuiAddress, TransactionDigest, STD_ASCII_MODULE_NAME, STD_ASCII_STRUCT_NAME,
    STD_OPTION_MODULE_NAME, STD_OPTION_STRUCT_NAME,
};
use sui_types::dynamic_field::derive_dynamic_field_id;
use sui_types::gas_coin::GasCoin;
use sui_types::object::Object;
use sui_types::{parse_sui_type_tag, MOVE_STDLIB_ADDRESS};

use crate::ResolvedCallArg;

use super::{check_valid_homogeneous, HEX_PREFIX};
use super::{resolve_move_function_args, SuiJsonValue};

// Negative test cases
#[test]
fn test_json_not_homogeneous() {
    let checks = vec![
        (json!([1, 2, 3, true, 5, 6, 7])),
        // Although we can encode numbers as strings, we do not allow mixing primitive
        // numbers and string encoded numbers
        (json!([1, 2, "4", 4, 5, 6, 7])),
        (json!([1, 2, 3, 4, "", 6, 7])),
        (json!([
            1,
            2,
            3,
            4,
            "456478542957455650244254734723567875646785024425473472356787564678463250089787",
            6,
            7
        ])),
        (json!([[], 2, 3, 5, 6, 7])),
        (json!([[[9, 53, 434], [0], [300]], [], [300, 4, 5, 6, 7]])),
        (json!([1, 2, 3, 4, 5, 6, 0.4])),
        (json!([4.2])),
        (json!(4.7)),
    ];
    // Driver
    for arg in checks {
        assert!(check_valid_homogeneous(&arg).is_err());
    }
}

// Positive test cases
#[test]
fn test_json_is_homogeneous() {
    let checks = vec![
        (json!([1, 2, 3, 4, 5, 6, 7])),
        (json!(["123", "456"])),
        (json!([
            "123",
            "456478542957455650244254734723567875646785024425473472356787564678463250089787"
        ])),
        (json!([])),
        (json!([[[9, 53, 434], [0], [300]], [], [[332], [4, 5, 6, 7]]])),
        (json!([[], [true], [false], []])),
        (json!([[[[[2]]]], [], [[]], []])),
        (json!([3])),
        (json!([])),
        (json!(1)),
    ];

    // Driver
    for arg in checks {
        assert!(check_valid_homogeneous(&arg).is_ok());
    }
}

#[test]
fn test_json_struct_homogeneous() {
    let positive = json!({"inner_vec":[1, 2, 3, 4, 5, 6, 7]});
    assert!(SuiJsonValue::new(positive).is_ok());

    let negative = json!({"inner_vec":[1, 2, 3, true, 5, 6, 7]});
    assert!(SuiJsonValue::new(negative).is_err());
}

#[test]
fn test_json_is_not_valid_sui_json() {
    let checks = vec![
        // Not homogeneous
        (json!([1, 2, 3, true, 5, 6, 7])),
        // Not homogeneous
        (json!([1, 2, 3, "123456", 5, 6, 7])),
        // Float not allowed
        (json!(1.3)),
        // Negative not allowed
        (json!(-10)),
        // Not homogeneous
        (json!([[[9, 53, 434], [0], [300]], [], [300, 4, 5, 6, 7]])),
    ];

    // Driver
    for arg in checks {
        assert!(SuiJsonValue::new(arg).is_err());
    }
}

#[test]
fn test_json_is_valid_sui_json() {
    let checks = vec![
        // Homogeneous
        (json!([1, 2, 3, 4, 5, 6, 7])),
        // String allowed
        (json!("a string")),
        // Bool allowed
        (json!(true)),
        // Uint allowed
        (json!(100)),
        (json!([])),
        // Homogeneous
        (json!([[[9, 53, 434], [0], [300]], [], [[332], [4, 5, 6, 7]]])),
    ];

    // Driver
    for arg in checks {
        assert!(SuiJsonValue::new(arg).is_ok());
    }
}

#[test]
fn test_basic_args_linter_pure_args_bad() {
    let bad_hex_val = "0x1234AB  CD";

    let checks = vec![
            // Although U256 value can be encoded as num, we enforce it must be a string
            (
                Value::from(123),
                MoveTypeLayout::U256,
            ),
             // Space not allowed
             (Value::from(" 9"), MoveTypeLayout::U8),
             // Hex must start with 0x
             (Value::from("AB"), MoveTypeLayout::U8),
             // Too large
             (Value::from("123456789"), MoveTypeLayout::U8),
             // Too large
             (Value::from("123456789123456789123456789123456789"), MoveTypeLayout::U64),
             // Too large
             (Value::from("123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789"), MoveTypeLayout::U128),
             // U64 value greater than 255 cannot be used as U8
             (Value::from(900u64), MoveTypeLayout::U8),
             // floats cannot be used as U8
             (Value::from(0.4f32), MoveTypeLayout::U8),
             // floats cannot be used as U64
             (Value::from(3.4f32), MoveTypeLayout::U64),
             // Negative cannot be used as U64
             (Value::from(-19), MoveTypeLayout::U64),
             // Negative cannot be used as Unsigned
             (Value::from(-1), MoveTypeLayout::U8),
              // u8 vector from bad hex repr
            (
                Value::from(bad_hex_val),
                MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            ),
            // u8 vector from heterogeneous array
            (
                json!([1, 2, 3, true, 5, 6, 7]),
                MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            ),
            // U64 deep nest, bad because heterogeneous array
            (
                json!([[[9, 53, 434], [0], [300]], [], [300, 4, 5, 6, 7]]),
                MoveTypeLayout::Vector(Box::new(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U64)))),
            ),
    ];

    // Driver
    for (arg, expected_type) in checks {
        let r = SuiJsonValue::new(arg);
        assert!(r.is_err() || r.unwrap().to_bcs_bytes(&expected_type).is_err());
    }
}

#[test]
fn test_basic_args_linter_pure_args_good() {
    let good_ascii_str = "123456789hdffwfof libgude ihibhdede +_))@+";
    let good_utf8_str = "enbeuf√12∫∆∂3456789hdπ˚ffwfof libgude ˚ø˙ßƒçß +_))@+";
    let good_hex_val = "0x1234ABCD";
    let u128_val = u64::MAX as u128 + 0xff;
    let u256_hex_val = "0x1234567812345678877EDA56789098ABCDEF12";
    let u256_val = U256::from_str_radix(u256_hex_val.trim_start_matches("0x"), 16).unwrap();

    let checks = vec![
        // Expected Bool match
        (
            Value::from(true),
            MoveTypeLayout::Bool,
            bcs::to_bytes(&true).unwrap(),
        ),
        // Expected U8 match
        (
            Value::from(9u8),
            MoveTypeLayout::U8,
            bcs::to_bytes(&9u8).unwrap(),
        ),
        // Expected U8 match
        (
            Value::from(u8::MAX),
            MoveTypeLayout::U8,
            bcs::to_bytes(&u8::MAX).unwrap(),
        ),
        // Expected U16 match
        (
            Value::from(9000u16),
            MoveTypeLayout::U16,
            bcs::to_bytes(&9000u16).unwrap(),
        ),
        // Expected U32 match
        (
            Value::from(1233459000u32),
            MoveTypeLayout::U32,
            bcs::to_bytes(&1233459000u32).unwrap(),
        ),
        // Expected U16 match
        (
            Value::from(u16::MAX),
            MoveTypeLayout::U16,
            bcs::to_bytes(&u16::MAX).unwrap(),
        ),
        // Expected U32 match
        (
            Value::from(u32::MAX),
            MoveTypeLayout::U32,
            bcs::to_bytes(&u32::MAX).unwrap(),
        ),
        // U64 value less than 256 can be used as U8
        (
            Value::from(9u64),
            MoveTypeLayout::U8,
            bcs::to_bytes(&9u8).unwrap(),
        ),
        // U8 value encoded as str
        (
            Value::from("89"),
            MoveTypeLayout::U8,
            bcs::to_bytes(&89u8).unwrap(),
        ),
        // U16 value encoded as str
        (
            Value::from("12389"),
            MoveTypeLayout::U16,
            bcs::to_bytes(&12389u16).unwrap(),
        ),
        // U32 value encoded as str
        (
            Value::from("123899856"),
            MoveTypeLayout::U32,
            bcs::to_bytes(&123899856u32).unwrap(),
        ),
        // U8 value encoded as str promoted to U64
        (
            Value::from("89"),
            MoveTypeLayout::U64,
            bcs::to_bytes(&89u64).unwrap(),
        ),
        // U64 value encoded as str
        (
            Value::from("890"),
            MoveTypeLayout::U64,
            bcs::to_bytes(&890u64).unwrap(),
        ),
        // U64 value encoded as str
        (
            Value::from(format!("{}", u64::MAX)),
            MoveTypeLayout::U64,
            bcs::to_bytes(&u64::MAX).unwrap(),
        ),
        // U128 value encoded as str
        (
            Value::from(format!("{u128_val}")),
            MoveTypeLayout::U128,
            bcs::to_bytes(&u128_val).unwrap(),
        ),
        // U128 value encoded as str
        (
            Value::from(format!("{}", u128::MAX)),
            MoveTypeLayout::U128,
            bcs::to_bytes(&u128::MAX).unwrap(),
        ),
        // U256 value encoded as str
        (
            Value::from(format!("{u256_val}")),
            MoveTypeLayout::U256,
            bcs::to_bytes(&u256_val).unwrap(),
        ),
        // U8 value encoded as hex str
        (
            Value::from("0x12"),
            MoveTypeLayout::U8,
            bcs::to_bytes(&0x12u8).unwrap(),
        ),
        // U8 value encoded as hex str promoted to U64
        (
            Value::from("0x12"),
            MoveTypeLayout::U64,
            bcs::to_bytes(&0x12u64).unwrap(),
        ),
        // U64 value encoded as hex str
        (
            Value::from("0x890"),
            MoveTypeLayout::U64,
            bcs::to_bytes(&0x890u64).unwrap(),
        ),
        // U128 value encoded as hex str
        (
            Value::from(format!("0x{:02x}", u128_val)),
            MoveTypeLayout::U128,
            bcs::to_bytes(&u128_val).unwrap(),
        ),
        // U256 value encoded as hex str
        (
            Value::from(u256_hex_val.to_string()),
            MoveTypeLayout::U256,
            bcs::to_bytes(&u256_val).unwrap(),
        ),
        // U256 value encoded as hex str
        (
            Value::from(format!("0x{:02x}", U256::max_value())),
            MoveTypeLayout::U256,
            bcs::to_bytes(&U256::max_value()).unwrap(),
        ),
        // u8 vector can be gotten from string
        (
            Value::from(good_ascii_str),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            bcs::to_bytes(&good_ascii_str.as_bytes()).unwrap(),
        ),
        // u8 vector from bad string
        (
            Value::from(good_utf8_str),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            bcs::to_bytes(&good_utf8_str.as_bytes()).unwrap(),
        ),
        // u8 vector from hex repr
        (
            Value::from(good_hex_val),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            bcs::to_bytes(&Hex::decode(good_hex_val.trim_start_matches(HEX_PREFIX)).unwrap())
                .unwrap(),
        ),
        // u8 vector from u8 array
        (
            json!([1, 2, 3, 4, 5, 6, 7]),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            bcs::to_bytes(&vec![1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8]).unwrap(),
        ),
        // Vector of vector of u8s
        (
            json!([[1, 2, 3], [], [3, 4, 5, 6, 7]]),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::Vector(Box::new(
                MoveTypeLayout::U8,
            )))),
            bcs::to_bytes(&vec![
                vec![1u8, 2u8, 3u8],
                vec![],
                vec![3u8, 4u8, 5u8, 6u8, 7u8],
            ])
            .unwrap(),
        ),
        // U64 nest
        (
            json!([["1111", "2", "3"], [], ["300", "4", "5", "6", "7"]]),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::Vector(Box::new(
                MoveTypeLayout::U64,
            )))),
            bcs::to_bytes(&vec![
                vec![1111u64, 2u64, 3u64],
                vec![],
                vec![300u64, 4u64, 5u64, 6u64, 7u64],
            ])
            .unwrap(),
        ),
        // U32 deep nest, good
        (
            json!([[[9, 53, 434], [0], [300]], [], [[332], [4, 5, 6, 7]]]),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::Vector(Box::new(
                MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U32)),
            )))),
            bcs::to_bytes(&vec![
                vec![vec![9u32, 53u32, 434u32], vec![0u32], vec![300u32]],
                vec![],
                vec![vec![332u32], vec![4u32, 5u32, 6u32, 7u32]],
            ])
            .unwrap(),
        ),
    ];

    // Driver
    for (arg, expected_type, expected_val) in checks {
        let r = SuiJsonValue::new(arg);
        // Must conform
        assert!(r.is_ok());
        // Must be serializable
        let sr = r.unwrap().to_bcs_bytes(&expected_type);
        // Must match expected serialized value
        assert_eq!(sr.unwrap(), expected_val);
    }
}

#[test]
fn test_basic_args_linter_top_level() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/move/basics");
    let compiled_modules = BuildConfig::new_for_testing()
        .build(&path)
        .unwrap()
        .into_modules();
    let example_package = Object::new_package_for_testing(
        &compiled_modules,
        TransactionDigest::genesis_marker(),
        BuiltInFramework::genesis_move_packages(),
    )
    .unwrap();
    let package = example_package.data.try_as_package().unwrap();

    let module = Identifier::new("resolve_args").unwrap();
    let function = Identifier::new("foo").unwrap();

    // Function signature:
    // foo(
    //     _foo: &mut Foo,
    //     _bar: vector<Foo>,
    //     _name: vector<u8>,
    //     _index: u64,
    //     _flag: u8,
    //     _recipient: address,
    //     _ctx: &mut TxContext,
    // )

    let foo_id = ObjectID::random();
    let bar_id = ObjectID::random();
    let baz_id = ObjectID::random();
    let recipient_addr = SuiAddress::random_for_testing_only();

    let foo = json!(foo_id.to_canonical_string(/* with_prefix */ true));
    let bar = json!([
        bar_id.to_canonical_string(/* with_prefix */ true),
        baz_id.to_canonical_string(/* with_prefix */ true),
    ]);

    let name = json!("Name");
    let index = json!("12345678");
    let flag = json!(89);
    let recipient = json!(recipient_addr.to_string());

    let args: Vec<_> = [
        foo.clone(),
        bar.clone(),
        name.clone(),
        index.clone(),
        flag,
        recipient.clone(),
    ]
    .into_iter()
    .map(|q| SuiJsonValue::new(q.clone()).unwrap())
    .collect();

    let json_args: Vec<_> =
        resolve_move_function_args(package, module.clone(), function.clone(), &[], args)
            .unwrap()
            .into_iter()
            .map(|(arg, _)| arg)
            .collect();

    use ResolvedCallArg as RCA;
    fn pure<T: Serialize>(t: &T) -> RCA {
        RCA::Pure(bcs::to_bytes(t).unwrap())
    }

    assert_eq!(
        json_args,
        vec![
            RCA::Object(foo_id),
            RCA::ObjVec(vec![bar_id, baz_id]),
            pure(&"Name"),
            pure(&12345678u64),
            pure(&89u8),
            pure(&recipient_addr),
        ],
    );

    // Flag is u8 so too large
    let args: Vec<_> = [foo, bar, name, index, json!(10000u64), recipient]
        .into_iter()
        .map(|q| SuiJsonValue::new(q.clone()).unwrap())
        .collect();

    assert!(resolve_move_function_args(package, module, function, &[], args,).is_err());
}

#[test]
fn test_convert_address_from_bcs() {
    let bcs_bytes = [
        50, 134, 111, 1, 9, 250, 27, 169, 17, 57, 45, 205, 45, 66, 96, 241, 216, 36, 49, 51, 22,
        245, 70, 122, 191, 100, 24, 123, 62, 239, 165, 85,
    ];

    let value = SuiJsonValue::from_bcs_bytes(Some(&MoveTypeLayout::Signer), &bcs_bytes).unwrap();

    assert_eq!(
        "0x32866f0109fa1ba911392dcd2d4260f1d824313316f5467abf64187b3eefa555",
        value.0.as_str().unwrap()
    );
}

#[test]
fn test_convert_number_from_bcs() {
    let bcs_bytes = [160u8, 134, 1, 0];
    let value = SuiJsonValue::from_bcs_bytes(Some(&MoveTypeLayout::U32), &bcs_bytes).unwrap();
    assert_eq!(100000, value.0.as_u64().unwrap());
}

#[test]
fn test_no_address_zero_trimming() {
    let bcs_bytes = bcs::to_bytes(
        &AccountAddress::from_str(
            "0x0000000000000000000000000000011111111111111111111111111111111111",
        )
        .unwrap(),
    )
    .unwrap();
    let value = SuiJsonValue::from_bcs_bytes(Some(&MoveTypeLayout::Address), &bcs_bytes).unwrap();
    assert_eq!(
        "0x0000000000000000000000000000011111111111111111111111111111111111",
        value.0.as_str().unwrap()
    );
}

#[test]
fn test_convert_number_array_from_bcs() {
    let bcs_bytes = [
        5, 80, 195, 0, 0, 80, 195, 0, 0, 80, 195, 0, 0, 80, 195, 0, 0, 80, 195, 0, 0,
    ];

    let value = SuiJsonValue::from_bcs_bytes(
        Some(&MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U32))),
        &bcs_bytes,
    )
    .unwrap();

    for value in value.0.as_array().unwrap() {
        assert_eq!(50000, value.as_u64().unwrap())
    }
}

#[test]
fn test_from_str() {
    // test number
    let test = SuiJsonValue::from_str("10000").unwrap();
    assert!(test.0.is_number());
    // Test array
    let test = SuiJsonValue::from_str("[10,10,10,10]").unwrap();
    assert!(test.0.is_array());
    assert_eq!(
        vec![10, 10, 10, 10],
        test.0
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap().to_u8().unwrap())
            .collect::<Vec<_>>()
    );
    // test bool
    let test = SuiJsonValue::from_str("true").unwrap();
    assert!(test.0.is_boolean());

    // test id without quotes
    let object_id = ObjectID::random().to_hex_uncompressed();
    let test = SuiJsonValue::from_str(&object_id).unwrap();
    assert!(test.0.is_string());
    assert_eq!(object_id, test.0.as_str().unwrap());

    // test id with quotes
    let test = SuiJsonValue::from_str(&format!("\"{}\"", &object_id)).unwrap();
    assert!(test.0.is_string());
    assert_eq!(object_id, test.0.as_str().unwrap());

    // test string without quotes
    let test = SuiJsonValue::from_str("Some string").unwrap();
    assert!(test.0.is_string());
    assert_eq!("Some string", test.0.as_str().unwrap());

    // test string with quotes
    let test = SuiJsonValue::from_str("\"Some string\"").unwrap();
    assert!(test.0.is_string());
    assert_eq!("Some string", test.0.as_str().unwrap());

    let test = SuiJsonValue::from_object_id(
        ObjectID::from_str("0x0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap(),
    );
    assert!(test.0.is_string());
    assert_eq!(
        "0x0000000000000000000000000000000000000000000000000000000000000001",
        test.0.as_str().unwrap()
    );
}

#[test]
fn test_sui_call_arg_string_type() {
    let arg1 = bcs::to_bytes("Some String").unwrap();

    let string_layout = Some(MoveTypeLayout::Struct(MoveStructLayout {
        type_: StructTag {
            address: MOVE_STDLIB_ADDRESS,
            module: STD_ASCII_MODULE_NAME.into(),
            name: STD_ASCII_STRUCT_NAME.into(),
            type_params: vec![],
        },
        fields: vec![MoveFieldLayout {
            name: ident_str!("bytes").into(),
            layout: MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
        }],
    }));
    let v = SuiJsonValue::from_bcs_bytes(string_layout.as_ref(), &arg1).unwrap();

    assert_eq!(json! {"Some String"}, v.to_json_value());
}

#[test]
fn test_sui_call_arg_option_type() {
    let arg1 = bcs::to_bytes(&Some("Some String")).unwrap();

    let string_layout = MoveTypeLayout::Struct(MoveStructLayout {
        type_: StructTag {
            address: MOVE_STDLIB_ADDRESS,
            module: STD_ASCII_MODULE_NAME.into(),
            name: STD_ASCII_STRUCT_NAME.into(),
            type_params: vec![],
        },
        fields: vec![MoveFieldLayout {
            name: ident_str!("bytes").into(),
            layout: MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
        }],
    });

    let option_layout = MoveTypeLayout::Struct(MoveStructLayout {
        type_: StructTag {
            address: MOVE_STDLIB_ADDRESS,
            module: STD_OPTION_MODULE_NAME.into(),
            name: STD_OPTION_STRUCT_NAME.into(),
            type_params: vec![],
        },
        fields: vec![MoveFieldLayout {
            name: ident_str!("vec").into(),
            layout: MoveTypeLayout::Vector(Box::new(string_layout.clone())),
        }],
    });

    let v = SuiJsonValue::from_bcs_bytes(Some(option_layout).as_ref(), &arg1).unwrap();

    let bytes = v
        .to_bcs_bytes(&MoveTypeLayout::Vector(Box::new(string_layout)))
        .unwrap();

    assert_eq!(json! {["Some String"]}, v.to_json_value());
    assert_eq!(arg1, bytes);

    let s = SuiJsonValue::from_str("[test, test2]").unwrap();
    println!("{s:?}");
}

#[test]
fn test_convert_struct() {
    let layout = MoveTypeLayout::Struct(GasCoin::layout());

    let value = json!({"id":"0xf1416fe18c7baa1673187375777a7606708481311cb3548509ec91a5871c6b9a", "balance": "1000000"});
    let sui_json = SuiJsonValue::new(value).unwrap();

    println!("JS: {:#?}", sui_json);

    let bcs = sui_json.to_bcs_bytes(&layout).unwrap();

    let coin: GasCoin = bcs::from_bytes(&bcs).unwrap();
    assert_eq!(
        coin.0.id.id.bytes,
        ObjectID::from_str("0xf1416fe18c7baa1673187375777a7606708481311cb3548509ec91a5871c6b9a")
            .unwrap()
    );
    assert_eq!(coin.0.balance.value(), 1000000);
}

#[test]
fn test_convert_string_vec() {
    let test_vec = vec!["0xbbb", "test_str"];
    let bcs = bcs::to_bytes(&test_vec).unwrap();
    let string_layout = MoveTypeLayout::Struct(MoveStructLayout {
        type_: StructTag {
            address: MOVE_STDLIB_ADDRESS,
            module: STD_ASCII_MODULE_NAME.into(),
            name: STD_ASCII_STRUCT_NAME.into(),
            type_params: vec![],
        },
        fields: vec![MoveFieldLayout {
            name: ident_str!("bytes").into(),
            layout: MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
        }],
    });

    let layout = MoveTypeLayout::Vector(Box::new(string_layout));

    let value = json!(test_vec);
    let sui_json = SuiJsonValue::new(value).unwrap();

    let bcs2 = sui_json.to_bcs_bytes(&layout).unwrap();

    assert_eq!(bcs, bcs2);
}

#[test]
fn test_string_vec_df_name_child_id_eq() {
    let parent_id =
        ObjectID::from_str("0x13a3ab664bfbdff0ab03cd1ce8c6fb3f31a8803f2e6e0b14b610f8e94fcb8509")
            .unwrap();
    let name = json!({
        "labels": [
            "0x0001",
            "sui"
        ]
    });

    let string_layout = MoveTypeLayout::Struct(MoveStructLayout {
        type_: StructTag {
            address: MOVE_STDLIB_ADDRESS,
            module: STD_ASCII_MODULE_NAME.into(),
            name: STD_ASCII_STRUCT_NAME.into(),
            type_params: vec![],
        },
        fields: vec![MoveFieldLayout {
            name: ident_str!("bytes").into(),
            layout: MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
        }],
    });

    let layout = MoveTypeLayout::Struct(MoveStructLayout {
        type_: StructTag {
            address: MOVE_STDLIB_ADDRESS,
            module: STD_ASCII_MODULE_NAME.into(),
            name: STD_ASCII_STRUCT_NAME.into(),
            type_params: vec![],
        },
        fields: vec![MoveFieldLayout::new(
            Identifier::from_str("labels").unwrap(),
            MoveTypeLayout::Vector(Box::new(string_layout)),
        )],
    });

    let sui_json = SuiJsonValue::new(name).unwrap();
    let bcs2 = sui_json.to_bcs_bytes(&layout).unwrap();

    let child_id = derive_dynamic_field_id(
        parent_id,
        &parse_sui_type_tag(
            "0x3278d6445c6403c96abe9e25cc1213a85de2bd627026ee57906691f9bbf2bf8a::domain::Domain",
        )
        .unwrap(),
        &bcs2,
    )
    .unwrap();

    assert_eq!(
        "0x2c2e361ee262b9f1f9a930e27e092cce5906b1e63a699ee60aec2de452ab9c70",
        child_id.to_string()
    );
}
