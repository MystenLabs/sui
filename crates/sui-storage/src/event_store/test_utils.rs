// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use move_core_types::ident_str;
use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::value::MoveStruct;
use serde::{Deserialize, Serialize};
use sui_types::SUI_FRAMEWORK_ADDRESS;

use move_core_types::account_address::AccountAddress;
use sui_types::base_types::SuiAddress;
use sui_types::event::{Event, EventEnvelope, TransferType};
use sui_types::object::Owner;

#[derive(Debug, Serialize, Deserialize)]
struct TestEvent {
    creator: AccountAddress,
    name: String,
}

impl TestEvent {
    fn struct_tag(name: &'static str) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ident_str!("SUI").to_owned(),
            name: ident_str!(name).to_owned(),
            type_params: vec![TypeTag::Address, TypeTag::Vector(Box::new(TypeTag::U8))],
        }
    }

    fn move_struct(&self) -> MoveStruct {
        let move_bytes: Vec<_> = self
            .name
            .as_bytes()
            .iter()
            .map(|b| MoveValue::U8(*b))
            .collect();
        MoveStruct::WithFields(vec![
            (
                ident_str!("creator").to_owned(),
                MoveValue::Address(self.creator),
            ),
            (ident_str!("name").to_owned(), MoveValue::Vector(move_bytes)),
        ])
    }
}

pub fn new_test_publish_event(
    timestamp: u64,
    digest: TransactionDigest,
    seq_num: u64,
    sender: Option<SuiAddress>,
) -> EventEnvelope {
    EventEnvelope::new(
        timestamp,
        Some(digest),
        seq_num,
        Event::Publish {
            sender: sender.unwrap_or_else(SuiAddress::random_for_testing_only),
            package_id: ObjectID::random(),
        },
        None,
    )
}

pub fn new_test_newobj_event(
    timestamp: u64,
    digest: TransactionDigest,
    seq_num: u64,
    object_id: Option<ObjectID>,
    sender: Option<SuiAddress>,
    recipient: Option<Owner>,
) -> EventEnvelope {
    EventEnvelope::new(
        timestamp,
        Some(digest),
        seq_num,
        Event::NewObject {
            package_id: ObjectID::random(),
            transaction_module: Identifier::new("module").unwrap(),
            sender: sender.unwrap_or_else(SuiAddress::random_for_testing_only),
            recipient: recipient
                .unwrap_or_else(|| Owner::AddressOwner(SuiAddress::random_for_testing_only())),
            object_id: object_id.unwrap_or_else(ObjectID::random),
        },
        None,
    )
}

pub fn new_test_deleteobj_event(
    timestamp: u64,
    digest: TransactionDigest,
    seq_num: u64,
    object_id: Option<ObjectID>,
    sender: Option<SuiAddress>,
) -> EventEnvelope {
    EventEnvelope::new(
        timestamp,
        Some(digest),
        seq_num,
        Event::DeleteObject {
            package_id: ObjectID::random(),
            transaction_module: Identifier::new("module").unwrap(),
            sender: sender.unwrap_or_else(SuiAddress::random_for_testing_only),
            object_id: object_id.unwrap_or_else(ObjectID::random),
        },
        None,
    )
}

pub fn new_test_transfer_event(
    timestamp: u64,
    digest: TransactionDigest,
    seq_num: u64,
    object_version: u64,
    type_: TransferType,
    object_id: Option<ObjectID>,
    sender: Option<SuiAddress>,
    recipient: Option<Owner>,
) -> EventEnvelope {
    EventEnvelope::new(
        timestamp,
        Some(digest),
        seq_num,
        Event::TransferObject {
            package_id: ObjectID::random(),
            transaction_module: Identifier::new("module").unwrap(),
            sender: sender.unwrap_or_else(SuiAddress::random_for_testing_only),
            recipient: recipient
                .unwrap_or_else(|| Owner::AddressOwner(SuiAddress::random_for_testing_only())),
            object_id: object_id.unwrap_or_else(ObjectID::random),
            version: object_version.into(),
            type_,
            amount: Some(10),
        },
        None,
    )
}

pub fn new_test_move_event(
    timestamp: u64,
    digest: TransactionDigest,
    seq_num: u64,
    package_id: ObjectID,
    module_name: &str,
    event_struct_name: &'static str,
) -> EventEnvelope {
    let move_event = TestEvent {
        creator: AccountAddress::random(),
        name: "foobar_buz".to_string(),
    };
    let event_bytes = bcs::to_bytes(&move_event).unwrap();
    let (move_event, move_struct) = (
        Event::MoveEvent {
            package_id,
            transaction_module: Identifier::new(module_name).unwrap(),
            sender: SuiAddress::random_for_testing_only(),
            type_: TestEvent::struct_tag(event_struct_name),
            contents: event_bytes,
        },
        move_event.move_struct(),
    );

    let json = serde_json::to_value(&move_struct).expect("Cannot serialize move struct to JSON");
    EventEnvelope::new(timestamp, Some(digest), seq_num, move_event, Some(json))
}
