// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, value::MoveTypeLayout,
};
use serde_json::{json, Value};
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::object::Object;
use sui_types::SUI_FRAMEWORK_ADDRESS;

use crate::sui_json::{resolve_move_function_args, SuiJsonCallArg, SuiJsonValue};

use super::{is_homogeneous, HEX_PREFIX};

#[test]
fn test_json_is_homogeneous() {
    let checks = vec![
        (json!([1, 2, 3, true, 5, 6, 7]), false),
        (json!([1, 2, 3, 4, 5, 6, 7]), true),
        // Although we can encode numbers as strings, we do not allow mixing primitive
        // numbers and string encoded numbers
        (json!([1, 2, "4", 4, 5, 6, 7]), false),
        (json!([1, 2, 3, 4, "", 6, 7]), false),
        (json!([]), true),
        (json!([[], 2, 3, 5, 6, 7]), false),
        (
            json!([[[9, 53, 434], [0], [300]], [], [300, 4, 5, 6, 7]]),
            false,
        ),
        (
            json!([[[9, 53, 434], [0], [300]], [], [[332], [4, 5, 6, 7]]]),
            true,
        ),
        (json!([[], [true], [false], []]), true),
        (json!([[[[[2]]]], [], [[]], []]), true),
        (json!([3]), true),
        (json!([]), true),
        (json!(1), true),
    ];

    // Driver
    for (arg, expected_val) in checks {
        assert_eq!(is_homogeneous(&arg), expected_val);
    }
}

#[test]
fn test_json_is_valid_sui_json() {
    let checks = vec![
        // Not homogeneous
        (json!([1, 2, 3, true, 5, 6, 7]), false),
        // Homogeneous
        (json!([1, 2, 3, 4, 5, 6, 7]), true),
        // String allowed
        (json!("a string"), true),
        // Float not allowed
        (json!(1.3), false),
        // Bool allowed
        (json!(true), true),
        // Negative not allowed
        (json!(-10), false),
        // Uint allowed
        (json!(100), true),
        // Not homogeneous
        (
            json!([[[9, 53, 434], [0], [300]], [], [300, 4, 5, 6, 7]]),
            false,
        ),
        (json!([]), true),
        // Homogeneous
        (
            json!([[[9, 53, 434], [0], [300]], [], [[332], [4, 5, 6, 7]]]),
            true,
        ),
    ];

    // Driver
    for (arg, expected_val) in checks {
        assert_eq!(SuiJsonValue::new(arg).is_ok(), expected_val);
    }
}

#[test]
fn test_basic_args_linter_pure_args() {
    let good_ascii_str = "123456789hdffwfof libgude ihibhdede +_))@+";
    let good_utf8_str = "enbeuf√12∫∆∂3456789hdπ˚ffwfof libgude ˚ø˙ßƒçß +_))@+";
    let good_hex_val = "0x1234ABCD";
    let bad_hex_val = "0x1234AB  CD";
    let u128_val = u64::MAX as u128 + 0xff;

    let checks = vec![
        // Expected Bool match
        (
            Value::from(true),
            MoveTypeLayout::Bool,
            Some(bcs::to_bytes(&true).unwrap()),
        ),
        // Expected U8 match
        (
            Value::from(9u8),
            MoveTypeLayout::U8,
            Some(bcs::to_bytes(&9u8).unwrap()),
        ),
        // U64 value less than 256 can be used as U8
        (
            Value::from(9u64),
            MoveTypeLayout::U8,
            Some(bcs::to_bytes(&9u8).unwrap()),
        ),
        // U8 value encoded as str
        (
            Value::from("89"),
            MoveTypeLayout::U8,
            Some(bcs::to_bytes(&89u8).unwrap()),
        ),
        // U8 value encoded as str promoted to U64
        (
            Value::from("89"),
            MoveTypeLayout::U64,
            Some(bcs::to_bytes(&89u64).unwrap()),
        ),
        // U64 value encoded as str
        (
            Value::from("890"),
            MoveTypeLayout::U64,
            Some(bcs::to_bytes(&890u64).unwrap()),
        ),
        // U128 value encoded as str
        (
            Value::from(format!("{u128_val}")),
            MoveTypeLayout::U128,
            Some(bcs::to_bytes(&u128_val).unwrap()),
        ),
        // U8 value encoded as hex str
        (
            Value::from("0x12"),
            MoveTypeLayout::U8,
            Some(bcs::to_bytes(&0x12u8).unwrap()),
        ),
        // U8 value encoded as hex str promoted to U64
        (
            Value::from("0x12"),
            MoveTypeLayout::U64,
            Some(bcs::to_bytes(&0x12u64).unwrap()),
        ),
        // U64 value encoded as hex str
        (
            Value::from("0x890"),
            MoveTypeLayout::U64,
            Some(bcs::to_bytes(&0x890u64).unwrap()),
        ),
        // U128 value encoded as hex str
        (
            Value::from(format!("0x{:02x}", u128_val)),
            MoveTypeLayout::U128,
            Some(bcs::to_bytes(&u128_val).unwrap()),
        ),
        // Space not allowed
        (Value::from(" 9"), MoveTypeLayout::U8, None),
        // Hex must start with 0x
        (Value::from("AB"), MoveTypeLayout::U8, None),
        // Too large
        (Value::from("123456789"), MoveTypeLayout::U8, None),
        // Too large
        (Value::from("123456789123456789123456789123456789"), MoveTypeLayout::U64, None),
        // Too large
        (Value::from("123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789123456789"), MoveTypeLayout::U128, None),

        // U64 value greater than 255 cannot be used as U8
        (Value::from(900u64), MoveTypeLayout::U8, None),
        // floats cannot be used as U8
        (Value::from(0.4f32), MoveTypeLayout::U8, None),
        // floats cannot be used as U64
        (Value::from(3.4f32), MoveTypeLayout::U64, None),
        // Negative cannot be used as Unsigned
        (Value::from(-1), MoveTypeLayout::U8, None),
        // u8 vector can be gotten from string
        (
            Value::from(good_ascii_str),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            Some(bcs::to_bytes(&good_ascii_str.as_bytes()).unwrap()),
        ),
        // u8 vector from bad string
        (
            Value::from(good_utf8_str),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            Some(bcs::to_bytes(&good_utf8_str.as_bytes()).unwrap()),
        ),
        // u8 vector from hex repr
        (
            Value::from(good_hex_val),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            Some(
                bcs::to_bytes(&hex::decode(&good_hex_val.trim_start_matches(HEX_PREFIX)).unwrap())
                    .unwrap(),
            ),
        ),
        // u8 vector from bad hex repr
        (
            Value::from(bad_hex_val),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            None,
        ),
        // u8 vector from u8 array
        (
            json!([1, 2, 3, 4, 5, 6, 7]),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            Some(bcs::to_bytes(&vec![1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8]).unwrap()),
        ),
        // u8 vector from heterogenous array
        (
            json!([1, 2, 3, true, 5, 6, 7]),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            None,
        ),
        // Vector of vector of u8s
        (
            json!([[1, 2, 3], [], [3, 4, 5, 6, 7]]),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)))),
            Some(
                bcs::to_bytes(&vec![
                    vec![1u8, 2u8, 3u8],
                    vec![],
                    vec![3u8, 4u8, 5u8, 6u8, 7u8],
                ])
                .unwrap(),
            ),
        ),
        // U64 nest
        (
            json!([[1111, 2, 3], [], [300, 4, 5, 6, 7]]),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U64)))),
            Some(
                bcs::to_bytes(&vec![
                    vec![1111u64, 2u64, 3u64],
                    vec![],
                    vec![300u64, 4u64, 5u64, 6u64, 7u64],
                ])
                .unwrap(),
            ),
        ),
        // U64 deep nest, bad because heterogenous array
        (
            json!([[[9, 53, 434], [0], [300]], [], [300, 4, 5, 6, 7]]),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U64)))),
            None,
        ),
        // U64 deep nest, good
        (
            json!([[[9, 53, 434], [0], [300]], [], [[332], [4, 5, 6, 7]]]),
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::Vector(Box::new(
                MoveTypeLayout::U64,
            )))))),
            Some(
                bcs::to_bytes(&vec![
                    vec![vec![9u64, 53u64, 434u64], vec![0u64], vec![300u64]],
                    vec![],
                    vec![vec![332u64], vec![4u64, 5u64, 6u64, 7u64]],
                ])
                .unwrap(),
            ),
        ),
    ];

    // Driver
    for (arg, expected_type, expected_val) in checks {
        let r = SuiJsonValue::new(arg);

        match expected_val {
            Some(q) => {
                // Must be conform
                assert!(r.is_ok());
                // Must be serializable
                let sr = r.unwrap().to_bcs_bytes(&expected_type);
                // Must match expected serialized value
                assert_eq!(sr.unwrap(), q);
            }
            None => {
                assert!(r.is_err() || r.unwrap().to_bcs_bytes(&expected_type).is_err());
            }
        }
    }
}

#[test]
fn test_basic_args_linter_top_level() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sui_programmability/examples/nfts");
    let compiled_modules = sui_framework::build_and_verify_user_package(&path).unwrap();
    let example_package = Object::new_package(compiled_modules, TransactionDigest::genesis());

    let module = Identifier::new("Geniteam").unwrap();
    let function = Identifier::new("create_monster").unwrap();

    /*
    Function signature:
            public fun create_monster(
                _player: &mut Player,
                farm: &mut Farm,
                pet_monsters: &mut Collection,
                monster_name: vector<u8>,
                monster_img_index: u64,
                breed: u8,
                monster_affinity: u8,
                monster_description: vector<u8>,
                display: vector<u8>,
                ctx: &mut TxContext
            )
    */

    let monster_name_raw = "MonsterName";
    let monster_img_id_raw = 12345678;
    let breed_raw = 89;
    let monster_affinity_raw = 200;
    let monster_description_raw = "MonsterDescription";
    let display_raw = "DisplayUrl";

    let player_id = json!(format!("0x{:02x}", ObjectID::random()));
    let farm_id = json!(format!("0x{:02x}", ObjectID::random()));
    let pet_monsters_id = json!(format!("0x{:02x}", ObjectID::random()));
    // This is okay since not starting with 0x
    let monster_name = json!(monster_name_raw);
    // Well within U64 bounds
    let monster_img_id = json!(monster_img_id_raw);
    // Well within U8 bounds
    let breed = json!(breed_raw);
    // Well within U8 bounds
    let monster_affinity = json!(monster_affinity_raw);
    // This is okay since not starting with 0x
    let monster_description = json!(monster_description_raw);
    // This is okay since not starting with 0x
    let display = json!(display_raw);

    // They have to be ordered
    let args = vec![
        player_id,
        farm_id,
        pet_monsters_id,
        monster_name.clone(),
        monster_img_id.clone(),
        breed,
        monster_affinity.clone(),
        monster_description.clone(),
        display.clone(),
    ]
    .iter()
    .map(|q| SuiJsonValue::new(q.clone()).unwrap())
    .collect();

    let json_args =
        resolve_move_function_args(&example_package, module.clone(), function.clone(), args)
            .unwrap();

    assert!(!json_args.is_empty());

    assert_eq!(
        json_args[3],
        SuiJsonCallArg::Pure(bcs::to_bytes(&monster_name_raw.as_bytes().to_vec()).unwrap())
    );
    assert_eq!(
        json_args[4],
        SuiJsonCallArg::Pure(bcs::to_bytes(&(monster_img_id_raw as u64)).unwrap()),
    );
    assert_eq!(
        json_args[5],
        SuiJsonCallArg::Pure(bcs::to_bytes(&(breed_raw as u8)).unwrap())
    );
    assert_eq!(
        json_args[6],
        SuiJsonCallArg::Pure(bcs::to_bytes(&(monster_affinity_raw as u8)).unwrap()),
    );
    assert_eq!(
        json_args[7],
        SuiJsonCallArg::Pure(bcs::to_bytes(&monster_description_raw.as_bytes().to_vec()).unwrap()),
    );

    // Breed is u8 so too large
    let args = vec![
        monster_name,
        monster_img_id,
        json!(10000u64),
        monster_affinity,
        monster_description,
        display,
    ]
    .iter()
    .map(|q| SuiJsonValue::new(q.clone()).unwrap())
    .collect();
    assert!(resolve_move_function_args(&example_package, module, function, args).is_err());

    // Test with vecu8 as address
    let genesis_objs = sui_adapter::genesis::clone_genesis_packages();
    let framework_pkg = genesis_objs
        .iter()
        .find(|q| q.id() == ObjectID::from(SUI_FRAMEWORK_ADDRESS))
        .expect("Unable to find framework object");

    let module = Identifier::new("ObjectBasics").unwrap();
    let function = Identifier::new("create").unwrap();

    /*
    Function signature:
            public fun create(value: u64, recipient: vector<u8>, ctx: &mut TxContext)
    */
    let value_raw = 29897;
    let address = SuiAddress::random_for_testing_only();

    let value = json!(value_raw);
    // Encode as hex string
    let addr = json!(format!("0x{:02x}", address));

    // They have to be ordered
    let args = vec![value, addr]
        .iter()
        .map(|q| SuiJsonValue::new(q.clone()).unwrap())
        .collect();

    let args = resolve_move_function_args(framework_pkg, module, function, args).unwrap();

    assert_eq!(
        args[0],
        SuiJsonCallArg::Pure(bcs::to_bytes(&(value_raw as u64)).unwrap())
    );

    // Need to verify this specially
    // BCS serialzes addresses like vectors so there's a length prefix, which makes the vec longer by 1
    assert_eq!(
        args[1],
        SuiJsonCallArg::Pure(bcs::to_bytes(&AccountAddress::from(address)).unwrap()),
    );

    // Test with object args

    let module = Identifier::new("ObjectBasics").unwrap();
    let function = Identifier::new("transfer").unwrap();

    /*
    Function signature:
            public fun transfer(o: Object, recipient: vector<u8>, _ctx: &mut TxContext)
    */
    let object_id_raw = ObjectID::random();
    let address = SuiAddress::random_for_testing_only();

    let object_id = json!(format!("0x{:02x}", object_id_raw));
    // Encode as hex string
    let addr = json!(format!("0x{:02x}", address));

    // They have to be ordered
    let args = vec![object_id, addr]
        .iter()
        .map(|q| SuiJsonValue::new(q.clone()).unwrap())
        .collect();

    let args = resolve_move_function_args(framework_pkg, module, function, args).unwrap();

    assert_eq!(
        args[0],
        SuiJsonCallArg::Object(
            ObjectID::from_hex_literal(&format!("0x{:02x}", object_id_raw)).unwrap()
        )
    );

    // Need to verify this specially
    // BCS serialzes addresses like vectors so there's a length prefix, which makes the vec longer by 1
    assert_eq!(
        args[1],
        SuiJsonCallArg::Pure(bcs::to_bytes(&AccountAddress::from(address)).unwrap())
    );
}
