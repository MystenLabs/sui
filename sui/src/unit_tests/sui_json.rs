// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::normalized::Type;
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use serde_json::{json, Value};
use sui_adapter::{self, genesis::clone_genesis_packages};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    SUI_FRAMEWORK_ADDRESS,
};

use crate::sui_json::{resolve_move_function_args, SuiJsonValue};

use super::{is_homogenous, HEX_PREFIX};

#[test]
fn test_json_is_homogenous() {
    let checks = vec![
        (json!([1, 2, 3, true, 5, 6, 7]), false),
        (json!([1, 2, 3, 4, 5, 6, 7]), true),
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
        assert_eq!(is_homogenous(&arg), expected_val);
    }
}

#[test]
fn test_json_is_valid_sui_json() {
    let checks = vec![
        // Not homogeneous
        (json!([1, 2, 3, true, 5, 6, 7]), false),
        // Homogenous
        (json!([1, 2, 3, 4, 5, 6, 7]), true),
        // String not allowed
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
        // Homogenous
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

    let checks = vec![
        // Expected Bool match
        (
            Value::from(true),
            Type::Bool,
            Some(bcs::to_bytes(&true).unwrap()),
        ),
        // Expected U8 match
        (
            Value::from(9u8),
            Type::U8,
            Some(bcs::to_bytes(&9u8).unwrap()),
        ),
        // U64 value less than 256 can be used as U8
        (
            Value::from(9u64),
            Type::U8,
            Some(bcs::to_bytes(&9u8).unwrap()),
        ),
        // U64 value greater than 255 cannot be used as U8
        (Value::from(900u64), Type::U8, None),
        // floats cannot be used as U8
        (Value::from(0.4f32), Type::U8, None),
        // floats cannot be used as U64
        (Value::from(3.4f32), Type::U64, None),
        // Negative cannot be used as Unsigned
        (Value::from(-1), Type::U8, None),
        // u8 vector can be gotten from string
        (
            Value::from(good_ascii_str),
            Type::Vector(Box::new(Type::U8)),
            Some(bcs::to_bytes(&good_ascii_str.as_bytes()).unwrap()),
        ),
        // u8 vector from bad string
        (
            Value::from(good_utf8_str),
            Type::Vector(Box::new(Type::U8)),
            Some(bcs::to_bytes(&good_utf8_str.as_bytes()).unwrap()),
        ),
        // u8 vector from hex repr
        (
            Value::from(good_hex_val),
            Type::Vector(Box::new(Type::U8)),
            Some(
                bcs::to_bytes(&hex::decode(&good_hex_val.trim_start_matches(HEX_PREFIX)).unwrap())
                    .unwrap(),
            ),
        ),
        // u8 vector from bad hex repr
        (
            Value::from(bad_hex_val),
            Type::Vector(Box::new(Type::U8)),
            None,
        ),
        // u8 vector from u8 array
        (
            json!([1, 2, 3, 4, 5, 6, 7]),
            Type::Vector(Box::new(Type::U8)),
            Some(bcs::to_bytes(&vec![1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8]).unwrap()),
        ),
        // u8 vector from heterogenous array
        (
            json!([1, 2, 3, true, 5, 6, 7]),
            Type::Vector(Box::new(Type::U8)),
            None,
        ),
        // Vector of vector of u8s
        (
            json!([[1, 2, 3], [], [3, 4, 5, 6, 7]]),
            Type::Vector(Box::new(Type::Vector(Box::new(Type::U8)))),
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
            Type::Vector(Box::new(Type::Vector(Box::new(Type::U64)))),
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
            Type::Vector(Box::new(Type::Vector(Box::new(Type::U64)))),
            None,
        ),
        // U64 deep nest, good
        (
            json!([[[9, 53, 434], [0], [300]], [], [[332], [4, 5, 6, 7]]]),
            Type::Vector(Box::new(Type::Vector(Box::new(Type::Vector(Box::new(
                Type::U64,
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
    let genesis_objs = clone_genesis_packages();
    let framework_pkg = genesis_objs
        .iter()
        .find(|q| q.id() == ObjectID::from(SUI_FRAMEWORK_ADDRESS))
        .expect("Unable to find framework object");

    let module = Identifier::new("Geniteam").unwrap();
    let function = Identifier::new("create_monster").unwrap();

    /*
    Function signature:
            public fun create_monster(
                monster_name: vector<u8>,
                monster_img_id: u64,
                breed: u8,
                monster_affinity: u8,
                monster_description: vector<u8>,
                ctx: &mut TxContext
            )
    */
    let monster_name_raw = "MonsterName";
    let monster_img_id_raw = 12345678;
    let breed_raw = 89;
    let monster_affinity_raw = 200;
    let monster_description_raw = "MonsterDescription";

    // This is okay since not starting with 0x
    let monster_name = json!(monster_name_raw);
    // Well withing U64 bounds
    let monster_img_id = json!(monster_img_id_raw);
    // Well within U8 bounds
    let breed = json!(breed_raw);
    // Well within U8 bounds
    let monster_affinity = json!(monster_affinity_raw);
    // This is okay since not starting with 0x
    let monster_description = json!(monster_description_raw);

    // They have to be ordered
    let args = vec![
        monster_name.clone(),
        monster_img_id.clone(),
        breed,
        monster_affinity.clone(),
        monster_description.clone(),
    ]
    .iter()
    .map(|q| SuiJsonValue::new(q.clone()).unwrap())
    .collect();

    let (object_args, pure_args) =
        resolve_move_function_args(framework_pkg, module.clone(), function.clone(), args).unwrap();

    assert!(object_args.is_empty());

    assert_eq!(
        pure_args[0],
        bcs::to_bytes(&monster_name_raw.as_bytes().to_vec()).unwrap()
    );
    assert_eq!(
        pure_args[1],
        bcs::to_bytes(&(monster_img_id_raw as u64)).unwrap()
    );
    assert_eq!(pure_args[2], bcs::to_bytes(&(breed_raw as u8)).unwrap());
    assert_eq!(
        pure_args[3],
        bcs::to_bytes(&(monster_affinity_raw as u8)).unwrap()
    );
    assert_eq!(
        pure_args[4],
        bcs::to_bytes(&monster_description_raw.as_bytes().to_vec()).unwrap()
    );

    // Breed is u8 so too large
    let args = vec![
        monster_name,
        monster_img_id,
        json!(10000u64),
        monster_affinity,
        monster_description,
    ]
    .iter()
    .map(|q| SuiJsonValue::new(q.clone()).unwrap())
    .collect();
    assert!(resolve_move_function_args(framework_pkg, module, function, args).is_err());

    // Test with vecu8 as address

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

    let (object_args, pure_args) =
        resolve_move_function_args(framework_pkg, module, function, args).unwrap();

    assert!(object_args.is_empty());
    assert_eq!(pure_args[0], bcs::to_bytes(&(value_raw as u64)).unwrap());

    // Need to verify this specially
    // BCS serialzes addresses like vectors so there's a length prefix, which makes the vec longer by 1
    assert_eq!(
        pure_args[1],
        bcs::to_bytes(&AccountAddress::from(address)).unwrap()
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

    let (object_args, pure_args) =
        resolve_move_function_args(framework_pkg, module, function, args).unwrap();

    assert!(!object_args.is_empty());
    assert_eq!(
        object_args[0],
        ObjectID::from_hex_literal(&format!("0x{:02x}", object_id_raw)).unwrap()
    );

    // Need to verify this specially
    // BCS serialzes addresses like vectors so there's a length prefix, which makes the vec longer by 1
    assert_eq!(
        pure_args[0],
        bcs::to_bytes(&AccountAddress::from(address)).unwrap()
    );
}
