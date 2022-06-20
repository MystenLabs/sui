// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;

use move_core_types::{
    ident_str,
    language_storage::StructTag,
    value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
};

use serde::Deserialize;
use serde::Serialize;
use serde_json::json;

use crate::event_handler::to_json_value;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::gas_coin::GasCoin;
use sui_types::object::MoveObject;
use sui_types::SUI_FRAMEWORK_ADDRESS;

#[test]
fn test_to_json_value() {
    let move_event = TestEvent {
        creator: AccountAddress::random(),
        name: "test_event".into(),
        data: vec![100, 200, 300],
        coins: vec![
            GasCoin::new(ObjectID::random(), SequenceNumber::from_u64(10), 1000000),
            GasCoin::new(ObjectID::random(), SequenceNumber::from_u64(20), 2000000),
            GasCoin::new(ObjectID::random(), SequenceNumber::from_u64(30), 3000000),
        ],
    };
    let event_bytes = bcs::to_bytes(&move_event).unwrap();
    let move_object = MoveObject::new(TestEvent::type_(), event_bytes);
    let move_struct = move_object
        .to_move_struct(&TestEvent::layout())
        .unwrap()
        .into();
    let json_value = to_json_value(move_struct).unwrap();

    assert_eq!(
        Some(&json!(1000000)),
        json_value.pointer("/coins/0/balance")
    );
    assert_eq!(
        Some(&json!(2000000)),
        json_value.pointer("/coins/1/balance")
    );
    assert_eq!(
        Some(&json!(3000000)),
        json_value.pointer("/coins/2/balance")
    );
    assert_eq!(
        Some(&json!(move_event.coins[0].id().to_string())),
        json_value.pointer("/coins/0/id/id")
    );
    assert_eq!(Some(&json!(10)), json_value.pointer("/coins/0/id/version"));
    assert_eq!(Some(&json!(20)), json_value.pointer("/coins/1/id/version"));
    assert_eq!(Some(&json!(30)), json_value.pointer("/coins/2/id/version"));
    assert_eq!(
        Some(&json!(format!("{:#x}", move_event.creator))),
        json_value.pointer("/creator")
    );
    assert_eq!(Some(&json!(100)), json_value.pointer("/data/0"));
    assert_eq!(Some(&json!(200)), json_value.pointer("/data/1"));
    assert_eq!(Some(&json!(300)), json_value.pointer("/data/2"));
    assert_eq!(Some(&json!("test_event")), json_value.pointer("/name"));
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestEvent {
    creator: AccountAddress,
    name: UTF8String,
    data: Vec<u64>,
    coins: Vec<GasCoin>,
}

impl TestEvent {
    fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ident_str!("SUI").to_owned(),
            name: ident_str!("new_foobar").to_owned(),
            type_params: vec![],
        }
    }

    fn layout() -> MoveStructLayout {
        MoveStructLayout::WithTypes {
            type_: Self::type_(),
            fields: vec![
                MoveFieldLayout::new(ident_str!("creator").to_owned(), MoveTypeLayout::Address),
                MoveFieldLayout::new(
                    ident_str!("name").to_owned(),
                    MoveTypeLayout::Struct(UTF8String::layout()),
                ),
                MoveFieldLayout::new(
                    ident_str!("data").to_owned(),
                    MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U64)),
                ),
                MoveFieldLayout::new(
                    ident_str!("coins").to_owned(),
                    MoveTypeLayout::Vector(Box::new(MoveTypeLayout::Struct(GasCoin::layout()))),
                ),
            ],
        }
    }
}

// Rust version of the Move sui::utf8::String type
// TODO: Do we need this in the sui-types lib?
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
struct UTF8String {
    bytes: String,
}

impl From<&str> for UTF8String {
    fn from(s: &str) -> Self {
        Self {
            bytes: s.to_string(),
        }
    }
}

impl UTF8String {
    fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: Identifier::new("String").unwrap(),
            module: Identifier::new("utf8").unwrap(),
            type_params: vec![],
        }
    }
    fn layout() -> MoveStructLayout {
        MoveStructLayout::WithTypes {
            type_: Self::type_(),
            fields: vec![MoveFieldLayout::new(
                ident_str!("bytes").to_owned(),
                MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            )],
        }
    }
}
