// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use base_types::SequenceNumber;
use messages::{CallArg, ObjectArg};
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{StructTag, TypeTag},
};
use object::OBJECT_START_VERSION;

use base_types::ObjectID;

pub use mysten_network::multiaddr;

#[macro_use]
pub mod error;

pub mod accumulator;
pub mod balance;
pub mod base_types;
pub mod certificate_proof;
pub mod clock;
pub mod coin;
pub mod collection_types;
pub mod committee;
pub mod crypto;
pub mod digests;
pub mod display;
pub mod dynamic_field;
pub mod event;
pub mod gas;
pub mod gas_coin;
pub mod governance;
pub mod id;
pub mod in_memory_storage;
pub mod message_envelope;
pub mod messages;
pub mod messages_checkpoint;
pub mod move_package;
pub mod multisig;
pub mod object;
pub mod programmable_transaction_builder;
pub mod query;
pub mod quorum_driver_types;
pub mod signature;
pub mod storage;
pub mod sui_serde;
pub mod sui_system_state;
pub mod temporary_store;
pub mod versioned;

pub mod epoch_data;
#[path = "./unit_tests/utils.rs"]
pub mod utils;

/// 0x1-- account address where Move stdlib modules are stored
/// Same as the ObjectID
pub const MOVE_STDLIB_ADDRESS: AccountAddress = AccountAddress::ONE;
pub const MOVE_STDLIB_OBJECT_ID: ObjectID = ObjectID::from_single_byte(1);

/// 0x2-- account address where sui framework modules are stored
/// Same as the ObjectID
pub const SUI_FRAMEWORK_ADDRESS: AccountAddress = get_hex_address_two();
pub const SUI_FRAMEWORK_OBJECT_ID: ObjectID = ObjectID::from_single_byte(2);

/// 0x5: hardcoded object ID for the singleton sui system state object.
pub const SUI_SYSTEM_STATE_OBJECT_ID: ObjectID = ObjectID::from_single_byte(5);
pub const SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION: SequenceNumber = OBJECT_START_VERSION;

pub const SUI_SYSTEM_OBJ_CALL_ARG: CallArg = CallArg::Object(ObjectArg::SharedObject {
    id: SUI_SYSTEM_STATE_OBJECT_ID,
    initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
    mutable: true,
});

/// 0x6: hardcoded object ID for the singleton clock object.
pub const SUI_CLOCK_OBJECT_ID: ObjectID = ObjectID::from_single_byte(6);
pub const SUI_CLOCK_OBJECT_SHARED_VERSION: SequenceNumber = OBJECT_START_VERSION;

const fn get_hex_address_two() -> AccountAddress {
    let mut addr = [0u8; AccountAddress::LENGTH];
    addr[AccountAddress::LENGTH - 1] = 2u8;
    AccountAddress::new(addr)
}

pub fn sui_framework_address_concat_string(suffix: &str) -> String {
    format!("{}{suffix}", SUI_FRAMEWORK_ADDRESS.to_hex_literal())
}

pub fn parse_sui_struct_tag(s: &str) -> anyhow::Result<StructTag> {
    use move_command_line_common::types::ParsedStructType;
    ParsedStructType::parse(s)?.into_struct_tag(&resolve_address)
}

pub fn parse_sui_type_tag(s: &str) -> anyhow::Result<TypeTag> {
    use move_command_line_common::types::ParsedType;
    ParsedType::parse(s)?.into_type_tag(&resolve_address)
}

pub fn is_system_package(id: ObjectID) -> bool {
    matches!(id, MOVE_STDLIB_OBJECT_ID | SUI_FRAMEWORK_OBJECT_ID)
}

fn resolve_address(addr: &str) -> Option<AccountAddress> {
    match addr {
        "std" => Some(MOVE_STDLIB_ADDRESS),
        "sui" => Some(SUI_FRAMEWORK_ADDRESS),
        _ => None,
    }
}

pub trait MoveTypeTagTrait {
    fn get_type_tag() -> TypeTag;
}

impl MoveTypeTagTrait for u64 {
    fn get_type_tag() -> TypeTag {
        TypeTag::U64
    }
}
