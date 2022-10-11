// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::value::MoveStruct;
use serde::{Deserialize, Serialize};

use sui_types::base_types::SuiAddress;
use sui_types::event::{Event, EventEnvelope};
use sui_types::gas_coin::GAS;
use sui_types::object::Owner;
use sui_types::SUI_FRAMEWORK_ADDRESS;

use super::*;

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
    event_num: u64,
    sender: Option<SuiAddress>,
) -> EventEnvelope {
    EventEnvelope {
        timestamp,
        Some(digest),
        seq_num,
        event_num,
        Event::Publish {
            sender: sender.unwrap_or_else(SuiAddress::random_for_testing_only),
            package_id: ObjectID::random(),
        },
        move_struct_json_value: None,
    }
}

pub fn new_test_newobj_event(
    timestamp: u64,
    digest: TransactionDigest,
    seq_num: u64,
    event_num: u64,
    object_id: Option<ObjectID>,
    sender: Option<SuiAddress>,
    recipient: Option<Owner>,
) -> EventEnvelope {
    EventEnvelope {
        timestamp,
        tx_digest: Some(digest),
        tx_seq_num: seq_num,
        event_num,
        event: Event::NewObject {
            package_id: ObjectID::random(),
            transaction_module: Identifier::new("module").unwrap(),
            sender: sender.unwrap_or_else(SuiAddress::random_for_testing_only),
            recipient: recipient
                .unwrap_or_else(|| Owner::AddressOwner(SuiAddress::random_for_testing_only())),
            object_type: "0x2::test:NewObject".to_string(),
            object_id: object_id.unwrap_or_else(ObjectID::random),
            version: Default::default(),
        },
        move_struct_json_value: None,
    }
}

pub fn new_test_balance_change_event(
    timestamp: u64,
    seq_num: u64,
    event_num: u64,
    coin_object_id: Option<ObjectID>,
    sender: Option<SuiAddress>,
    owner: Option<Owner>,
) -> EventEnvelope {
    EventEnvelope::new(
        timestamp,
        Some(TransactionDigest::random()),
        seq_num,
        event_num,
        Event::CoinBalanceChange {
            package_id: ObjectID::random(),
            transaction_module: Identifier::new("module").unwrap(),
            sender: sender.unwrap_or_else(SuiAddress::random_for_testing_only),
            change_type: BalanceChangeType::Gas,
            owner: owner
                .unwrap_or_else(|| Owner::AddressOwner(SuiAddress::random_for_testing_only())),
            coin_type: GAS::type_().to_string(),
            coin_object_id: coin_object_id.unwrap_or_else(ObjectID::random),
            version: Default::default(),
            amount: -10000,
        },
        None,
    )
}
pub fn new_test_deleteobj_event(
    timestamp: u64,
    digest: TransactionDigest,
    seq_num: u64,
    event_num: u64,
    object_id: Option<ObjectID>,
    sender: Option<SuiAddress>,
) -> EventEnvelope {
    EventEnvelope {
        timestamp,
        tx_digest: Some(digest),
        tx_seq_num: seq_num,
        event_num,
        event: Event::DeleteObject {
            package_id: ObjectID::random(),
            transaction_module: Identifier::new("module").unwrap(),
            sender: sender.unwrap_or_else(SuiAddress::random_for_testing_only),
            object_id: object_id.unwrap_or_else(ObjectID::random),
            version: Default::default(),
        },
        move_struct_json_value: None,
    }
}

pub fn new_test_transfer_event(
    timestamp: u64,
    digest: TransactionDigest,
    seq_num: u64,
    event_num: u64,
    object_version: u64,
    object_type: &str,
    object_id: Option<ObjectID>,
    sender: Option<SuiAddress>,
    recipient: Option<Owner>,
) -> EventEnvelope {
    EventEnvelope {
        timestamp,
        tx_digest: Some(digest),
        tx_seq_num: seq_num,
        event_num,
        event: Event::TransferObject {
            package_id: ObjectID::random(),
            transaction_module: Identifier::new("module").unwrap(),
            sender: sender.unwrap_or_else(SuiAddress::random_for_testing_only),
            recipient: recipient
                .unwrap_or_else(|| Owner::AddressOwner(SuiAddress::random_for_testing_only())),
            object_type: object_type.to_string(),
            object_id: object_id.unwrap_or_else(ObjectID::random),
            version: object_version.into(),
        },
        None,
    )
}

pub fn new_test_mutate_event(
    timestamp: u64,
    seq_num: u64,
    event_num: u64,
    object_version: u64,
    object_type: &str,
    object_id: Option<ObjectID>,
    sender: Option<SuiAddress>,
) -> EventEnvelope {
    EventEnvelope::new(
        timestamp,
        Some(TransactionDigest::random()),
        seq_num,
        event_num,
        Event::MutateObject {
            package_id: ObjectID::random(),
            transaction_module: Identifier::new("module").unwrap(),
            sender: sender.unwrap_or_else(SuiAddress::random_for_testing_only),
            object_type: object_type.to_string(),
            object_id: object_id.unwrap_or_else(ObjectID::random),
            version: object_version.into(),
        },
        move_struct_json_value: None,
    }
}

pub fn new_test_move_event(
    timestamp: u64,
    digest: TransactionDigest,
    seq_num: u64,
    event_num: u64,
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
    EventEnvelope {
        timestamp,
        tx_digest: Some(digest),
        tx_seq_num: seq_num,
        event_num,
        event: move_event,
        move_struct_json_value: Some(json),
    }
}
