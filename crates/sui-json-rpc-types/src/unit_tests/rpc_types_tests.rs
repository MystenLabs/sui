// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use move_core_types::ident_str;
use move_core_types::language_storage::StructTag;
use move_core_types::value::{MoveStruct, MoveValue};

use crate::{SuiMoveStruct, SuiMoveValue};
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::gas_coin::GasCoin;
use sui_types::object::MoveObject;
use sui_types::sui_serde::Base64;
use sui_types::SUI_FRAMEWORK_ADDRESS;

#[test]
fn test_move_value_to_sui_bytearray() {
    let move_value = MoveValue::Vector(vec![
        MoveValue::U8(0),
        MoveValue::U8(1),
        MoveValue::U8(2),
        MoveValue::U8(3),
        MoveValue::U8(4),
    ]);
    let sui_value = SuiMoveValue::from(move_value);
    let bytes_base64 = Base64::from_bytes(&[0, 1, 2, 3, 4]);
    assert!(matches!(sui_value, SuiMoveValue::Bytearray(bytes) if bytes == bytes_base64))
}

#[test]
fn test_move_value_to_sui_coin() {
    let id = ObjectID::random();
    let value = 10000;
    let coin = GasCoin::new(id, SequenceNumber::new(), value);
    let bcs = coin.to_bcs_bytes();

    let move_object = MoveObject::new_gas_coin(bcs);
    let layout = GasCoin::layout();

    let move_struct = move_object.to_move_struct(&layout).unwrap();
    let sui_struct = SuiMoveStruct::from(move_struct);
    let gas_coin = GasCoin::try_from(&sui_struct).unwrap();
    assert_eq!(coin.value(), gas_coin.value());
    assert_eq!(coin.id(), gas_coin.id());
}

#[test]
fn test_move_value_to_string() {
    let test_string = "Some test string";
    let bytes = test_string.as_bytes();
    let values = bytes
        .iter()
        .map(|u8| MoveValue::U8(*u8))
        .collect::<Vec<_>>();

    let move_value = MoveValue::Struct(MoveStruct::WithTypes {
        type_: StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ident_str!("utf8").to_owned(),
            name: ident_str!("String").to_owned(),
            type_params: vec![],
        },
        fields: vec![(ident_str!("bytes").to_owned(), MoveValue::Vector(values))],
    });

    let sui_value = SuiMoveValue::from(move_value);

    assert!(matches!(sui_value, SuiMoveValue::String(s) if s == test_string));
}

#[test]
fn test_move_value_to_url() {
    let test_url = "http://testing.com";
    let bytes = test_url.as_bytes();
    let values = bytes
        .iter()
        .map(|u8| MoveValue::U8(*u8))
        .collect::<Vec<_>>();

    let string_move_value = MoveValue::Struct(MoveStruct::WithTypes {
        type_: StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ident_str!("utf8").to_owned(),
            name: ident_str!("String").to_owned(),
            type_params: vec![],
        },
        fields: vec![(ident_str!("bytes").to_owned(), MoveValue::Vector(values))],
    });

    let url_move_value = MoveValue::Struct(MoveStruct::WithTypes {
        type_: StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ident_str!("url").to_owned(),
            name: ident_str!("Url").to_owned(),
            type_params: vec![],
        },
        fields: vec![(ident_str!("url").to_owned(), string_move_value)],
    });

    let sui_value = SuiMoveValue::from(url_move_value);

    assert!(matches!(sui_value, SuiMoveValue::String(s) if s == test_url));
}

#[test]
fn test_serde() {
    let test_values = [
        SuiMoveValue::Number(u64::MAX),
        SuiMoveValue::VersionedID {
            id: ObjectID::random(),
            version: u64::MAX,
        },
        SuiMoveValue::String("some test string".to_string()),
        SuiMoveValue::Address(SuiAddress::random_for_testing_only()),
        SuiMoveValue::Bool(true),
        SuiMoveValue::Option(Box::new(None)),
        SuiMoveValue::Bytearray(Base64::from_bytes(&[10u8; 20])),
    ];

    for value in test_values {
        let json = serde_json::to_string(&value).unwrap();
        let serde_value: SuiMoveValue = serde_json::from_str(&json)
            .map_err(|e| anyhow!("Serde failed for [{:?}], Error msg : {}", value, e))
            .unwrap();
        assert_eq!(
            value, serde_value,
            "Error converting {:?} [{json}], got {:?}",
            value, serde_value
        )
    }
}
