// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use move_core_types::ident_str;
use move_core_types::language_storage::StructTag;
use move_core_types::value::{MoveStruct, MoveValue};

use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::coin::Coin;
use sui_types::gas_coin::GasCoin;
use sui_types::id::VersionedID;
use sui_types::object::MoveObject;
use sui_types::sui_serde::Base64;
use sui_types::SUI_FRAMEWORK_ADDRESS;

use crate::gateway_types::SuiMoveValue;

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
    assert!(matches!(sui_value, SuiMoveValue::ByteArray(bytes) if bytes == bytes_base64))
}

#[test]
fn test_move_value_to_sui_coin() {
    let id = ObjectID::random();
    let version = SequenceNumber::new();
    let value = 10000;
    let coin = GasCoin::new(id, SequenceNumber::new(), value);
    let bcs = coin.to_bcs_bytes();

    let move_object = MoveObject::new(GasCoin::type_(), bcs);
    let layout = GasCoin::layout();

    let move_value = move_object.to_move_value(&layout).unwrap();
    let sui_value = SuiMoveValue::from(move_value);
    assert!(
        matches!(sui_value, SuiMoveValue::Coin(coin) if coin.id.version == version.value() && coin.id.id.id.bytes == id && coin.value() == value)
    )
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
            name: ident_str!("String").to_owned(),
            module: ident_str!("UTF8").to_owned(),
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
            name: ident_str!("String").to_owned(),
            module: ident_str!("UTF8").to_owned(),
            type_params: vec![],
        },
        fields: vec![(ident_str!("bytes").to_owned(), MoveValue::Vector(values))],
    });

    let url_move_value = MoveValue::Struct(MoveStruct::WithTypes {
        type_: StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: ident_str!("Url").to_owned(),
            module: ident_str!("Url").to_owned(),
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
        SuiMoveValue::Coin(Coin::new(
            VersionedID::new(ObjectID::random(), SequenceNumber::new()),
            10,
        )),
        SuiMoveValue::U8(u8::MAX),
        SuiMoveValue::U64(u64::MAX),
        SuiMoveValue::U128(u128::MAX),
        SuiMoveValue::VersionedID(VersionedID::new(ObjectID::random(), SequenceNumber::MAX)),
        SuiMoveValue::String("some test string".to_string()),
        SuiMoveValue::Address(ObjectID::random()),
        SuiMoveValue::ByteArray(Base64::from_bytes(&[0u8; 20])),
        SuiMoveValue::Bool(true),
        SuiMoveValue::Signer(SuiAddress::random_for_testing_only()),
        SuiMoveValue::Option(Box::new(None)),
        SuiMoveValue::Option(Box::new(Some(SuiMoveValue::U8(u8::MAX)))),
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
