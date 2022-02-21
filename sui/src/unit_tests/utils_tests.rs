// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::normalized::Type;
use move_core_types::identifier::Identifier;
use serde_json::{json, Value};
use sui_adapter::genesis::clone_genesis_data;
use sui_framework::get_sui_framework_modules;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    SUI_FRAMEWORK_ADDRESS,
};

use crate::utils::check_and_refine_pure_args;

use super::{resolve_move_function_components, HEX_PREFIX};

#[test]
fn test_basic_args_linter_pure_args() {
    let u128_val = (u64::MAX as u128) + 0xFF;
    let good_ascii_str = "123456789hdffwfof libgude ihibhdede +_))@+";
    let bad_ascii_str = "enbeuf√12∫∆∂3456789hdπ˚ffwfof libgude ˚ø˙ßƒçß +_))@+";

    let good_hex_val = "0x1234ABCD";
    let bad_hex_val = "0x1234AB CD";

    let checks = vec![
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
        // Try to use u128 string as U128
        (
            Value::from(format!("{}", u128_val)),
            Type::U128,
            Some(bcs::to_bytes(&u128_val).unwrap()),
        ),
        // Try to u64 upgrade to u128
        (
            Value::from(1234u64),
            Type::U128,
            Some(bcs::to_bytes(&1234u128).unwrap()),
        ),
        // Try to use negative string as U128
        (Value::from(format!("-{}", u128_val)), Type::U128, None),
        // u8 vector from string
        (
            Value::from(good_ascii_str),
            Type::Vector(Box::new(Type::U8)),
            Some(bcs::to_bytes(&good_ascii_str.as_bytes()).unwrap()),
        ),
        // u8 vector from bad string
        (
            Value::from(bad_ascii_str),
            Type::Vector(Box::new(Type::U8)),
            None,
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
        // u128 vector from u8 array
        // Its okay to encode u128 as strings
        (
            json!(["1", "2", 3, 4, 5, 6, 7]),
            Type::Vector(Box::new(Type::U128)),
            Some(bcs::to_bytes(&vec![1u128, 2u128, 3u128, 4u128, 5u128, 6u128, 7u128]).unwrap()),
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
        // Vector of vector of u8s encoded as hex
        (
            json!(["0x010203", [], [3, 4, 5, 6, 7]]),
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
        // Vector of vector of u8s encoded as bytes
        (
            json!(["12345", [], [3, 4, 5, 6, 7]]),
            Type::Vector(Box::new(Type::Vector(Box::new(Type::U8)))),
            Some(
                bcs::to_bytes(&vec![
                    vec![b'1', b'2', b'3', b'4', b'5'],
                    vec![],
                    vec![3u8, 4u8, 5u8, 6u8, 7u8],
                ])
                .unwrap(),
            ),
        ),
        // Vector of vector of u8s encoded as hex
        (
            json!(["0xABCD1234", [], [3, 4, 5, 6, 7]]),
            Type::Vector(Box::new(Type::Vector(Box::new(Type::U8)))),
            Some(
                bcs::to_bytes(&vec![
                    vec![0xAB, 0xCD, 0x12, 0x34],
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
        // U128 nest
        (
            json!([
                [1111, 2, format!("{}", u128_val)],
                [],
                [300, 4, 5, format!("{}", u128_val), 7]
            ]),
            Type::Vector(Box::new(Type::Vector(Box::new(Type::U128)))),
            Some(
                bcs::to_bytes(&vec![
                    vec![1111u128, 2u128, u128_val],
                    vec![],
                    vec![300u128, 4u128, 5u128, u128_val, 7u128],
                ])
                .unwrap(),
            ),
        ),
        // U64 deep nest, bad because heterogenous
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
        let res = check_and_refine_pure_args(&arg, &expected_type);
        match expected_val {
            Some(q) => assert_eq!(q, res.unwrap()),
            None => assert!(res.is_err()),
        }
    }
}

#[test]
fn test_basic_args_linter_top_level() {
    let (genesis_objs, _) = clone_genesis_data();
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
    ];

    let components =
        resolve_move_function_components(framework_pkg, module.clone(), function.clone(), args)
            .unwrap();

    assert!(components.object_args.is_empty());
    assert!(components.type_args.is_empty());

    assert_eq!(
        components.pure_args_serialized[0],
        bcs::to_bytes(&monster_name_raw.as_bytes().to_vec()).unwrap()
    );
    assert_eq!(
        components.pure_args_serialized[1],
        bcs::to_bytes(&(monster_img_id_raw as u64)).unwrap()
    );
    assert_eq!(
        components.pure_args_serialized[2],
        bcs::to_bytes(&(breed_raw as u8)).unwrap()
    );
    assert_eq!(
        components.pure_args_serialized[3],
        bcs::to_bytes(&(monster_affinity_raw as u8)).unwrap()
    );
    assert_eq!(
        components.pure_args_serialized[4],
        bcs::to_bytes(&monster_description_raw.as_bytes().to_vec()).unwrap()
    );

    // Breed is u8 so too large
    let args = vec![
        monster_name,
        monster_img_id,
        json!(10000),
        monster_affinity,
        monster_description,
    ];
    assert!(resolve_move_function_components(framework_pkg, module, function, args).is_err());

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
    let args = vec![value, addr];

    let components =
        resolve_move_function_components(framework_pkg, module, function, args).unwrap();

    assert!(components.object_args.is_empty());
    assert!(components.type_args.is_empty());
    assert_eq!(
        components.pure_args_serialized[0],
        bcs::to_bytes(&(value_raw as u64)).unwrap()
    );
    assert_eq!(
        components.pure_args_serialized[1],
        bcs::to_bytes(&address).unwrap()
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
    let args = vec![object_id, addr];

    let components =
        resolve_move_function_components(framework_pkg, module, function, args).unwrap();

    assert!(!components.object_args.is_empty());
    assert!(components.type_args.is_empty());
    assert_eq!(
        components.object_args[0],
        ObjectID::from_hex_literal(&format!("0x{:02x}", object_id_raw)).unwrap()
    );
    assert_eq!(
        components.pure_args_serialized[0],
        bcs::to_bytes(&address).unwrap()
    );
}
