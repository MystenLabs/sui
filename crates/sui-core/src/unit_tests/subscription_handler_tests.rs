// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;

use move_core_types::{
    annotated_value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
    ident_str,
    language_storage::StructTag,
};

use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use sui_json_rpc_types::SuiMoveStruct;

use sui_types::base_types::ObjectID;
use sui_types::gas_coin::GasCoin;
use sui_types::object::bounded_visitor::BoundedVisitor;
use sui_types::{MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS};

#[test]
fn test_to_json_value() {
    let move_event = TestEvent {
        creator: AccountAddress::random(),
        name: "test_event".into(),
        data: vec![100, 200, 300],
        coins: vec![
            GasCoin::new(ObjectID::random(), 1000000),
            GasCoin::new(ObjectID::random(), 2000000),
            GasCoin::new(ObjectID::random(), 3000000),
        ],
    };
    let event_bytes = bcs::to_bytes(&move_event).unwrap();
    let sui_move_struct: SuiMoveStruct =
        BoundedVisitor::deserialize_struct(&event_bytes, &TestEvent::layout())
            .unwrap()
            .into();
    let json_value = sui_move_struct.to_json_value();
    assert_eq!(
        Some(&json!("1000000")),
        json_value.pointer("/coins/0/balance")
    );
    assert_eq!(
        Some(&json!("2000000")),
        json_value.pointer("/coins/1/balance")
    );
    assert_eq!(
        Some(&json!("3000000")),
        json_value.pointer("/coins/2/balance")
    );
    assert_eq!(
        Some(&json!(move_event.coins[0].id().to_string())),
        json_value.pointer("/coins/0/id/id")
    );
    assert_eq!(
        Some(&json!(format!("{:#x}", move_event.creator))),
        json_value.pointer("/creator")
    );
    assert_eq!(Some(&json!("100")), json_value.pointer("/data/0"));
    assert_eq!(Some(&json!("200")), json_value.pointer("/data/1"));
    assert_eq!(Some(&json!("300")), json_value.pointer("/data/2"));
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
        MoveStructLayout {
            type_: Self::type_(),
            fields: vec![
                MoveFieldLayout::new(ident_str!("creator").to_owned(), MoveTypeLayout::Address),
                MoveFieldLayout::new(
                    ident_str!("name").to_owned(),
                    MoveTypeLayout::Struct(Box::new(UTF8String::layout())),
                ),
                MoveFieldLayout::new(
                    ident_str!("data").to_owned(),
                    MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U64)),
                ),
                MoveFieldLayout::new(
                    ident_str!("coins").to_owned(),
                    MoveTypeLayout::Vector(Box::new(MoveTypeLayout::Struct(Box::new(
                        GasCoin::layout(),
                    )))),
                ),
            ],
        }
    }
}

// Rust version of the Move std::string::String type
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
            address: MOVE_STDLIB_ADDRESS,
            name: Identifier::new("String").unwrap(),
            module: Identifier::new("string").unwrap(),
            type_params: vec![],
        }
    }
    fn layout() -> MoveStructLayout {
        MoveStructLayout {
            type_: Self::type_(),
            fields: vec![MoveFieldLayout::new(
                ident_str!("bytes").to_owned(),
                MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            )],
        }
    }
}
