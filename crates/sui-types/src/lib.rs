// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use base_types::{SequenceNumber, SuiAddress, RESOLVED_ASCII_STR, RESOLVED_UTF8_STR};
use messages::{CallArg, ObjectArg};
use move_binary_format::{
    binary_views::BinaryIndexedView,
    file_format::{AbilitySet, SignatureToken},
};
use move_bytecode_utils::resolve_struct;
use move_core_types::{account_address::AccountAddress, language_storage::StructTag};
pub use move_core_types::{identifier::Identifier, language_storage::TypeTag};
use object::OBJECT_START_VERSION;

use base_types::ObjectID;

pub use mysten_network::multiaddr;

use crate::{base_types::RESOLVED_STD_OPTION, id::RESOLVED_SUI_ID};

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
pub mod effects;
pub mod event;
pub mod execution_status;
pub mod gas;
pub mod gas_coin;
pub mod gas_model;
pub mod governance;
pub mod id;
pub mod in_memory_storage;
pub mod message_envelope;
pub mod messages;
pub mod messages_checkpoint;
pub mod metrics;
pub mod move_package;
pub mod multisig;
pub mod object;
pub mod programmable_transaction_builder;
pub mod quorum_driver_types;
pub mod signature;
pub mod storage;
pub mod sui_serde;
pub mod sui_system_state;
pub mod temporary_store;
pub mod versioned;

pub mod epoch_data;

#[cfg(feature = "test-utils")]
#[path = "./unit_tests/utils.rs"]
pub mod utils;

/// 0x1-- account address where Move stdlib modules are stored
/// Same as the ObjectID
pub const MOVE_STDLIB_ADDRESS: AccountAddress = AccountAddress::ONE;
pub const MOVE_STDLIB_OBJECT_ID: ObjectID = ObjectID::from_address(MOVE_STDLIB_ADDRESS);

/// 0x2-- account address where sui framework modules are stored
/// Same as the ObjectID
pub const SUI_FRAMEWORK_ADDRESS: AccountAddress = address_from_single_byte(2);
pub const SUI_FRAMEWORK_OBJECT_ID: ObjectID = ObjectID::from_address(SUI_FRAMEWORK_ADDRESS);

/// 0x3-- account address where sui system modules are stored
/// Same as the ObjectID
pub const SUI_SYSTEM_ADDRESS: AccountAddress = address_from_single_byte(3);
pub const SUI_SYSTEM_OBJECT_ID: ObjectID = ObjectID::from_address(SUI_SYSTEM_ADDRESS);

/// 0xdee9-- account address where DeepBook modules are stored
/// Same as the ObjectID
pub const DEEPBOOK_ADDRESS: AccountAddress = deepbook_addr();
pub const DEEPBOOK_OBJECT_ID: ObjectID = ObjectID::from_address(DEEPBOOK_ADDRESS);

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

/// Return `true` if `id` is a special system package that can be upgraded at epoch boundaries
/// All new system package ID's must be added here
pub fn is_system_package(id: ObjectID) -> bool {
    matches!(
        id,
        MOVE_STDLIB_OBJECT_ID | SUI_FRAMEWORK_OBJECT_ID | SUI_SYSTEM_OBJECT_ID | DEEPBOOK_OBJECT_ID
    )
}

const fn address_from_single_byte(b: u8) -> AccountAddress {
    let mut addr = [0u8; AccountAddress::LENGTH];
    addr[AccountAddress::LENGTH - 1] = b;
    AccountAddress::new(addr)
}

/// return 0x0...dee9
const fn deepbook_addr() -> AccountAddress {
    let mut addr = [0u8; AccountAddress::LENGTH];
    addr[AccountAddress::LENGTH - 2] = 0xde;
    addr[AccountAddress::LENGTH - 1] = 0xe9;
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

fn resolve_address(addr: &str) -> Option<AccountAddress> {
    match addr {
        "deepbook" => Some(DEEPBOOK_ADDRESS),
        "std" => Some(MOVE_STDLIB_ADDRESS),
        "sui" => Some(SUI_FRAMEWORK_ADDRESS),
        "sui_system" => Some(SUI_SYSTEM_ADDRESS),
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

impl MoveTypeTagTrait for ObjectID {
    fn get_type_tag() -> TypeTag {
        TypeTag::Address
    }
}

impl MoveTypeTagTrait for SuiAddress {
    fn get_type_tag() -> TypeTag {
        TypeTag::Address
    }
}

pub fn is_primitive(
    view: &BinaryIndexedView<'_>,
    function_type_args: &[AbilitySet],
    s: &SignatureToken,
) -> bool {
    use SignatureToken as S;
    match s {
        S::Bool | S::U8 | S::U16 | S::U32 | S::U64 | S::U128 | S::U256 | S::Address => true,
        S::Signer => false,
        // optimistic, but no primitive has key
        S::TypeParameter(idx) => !function_type_args[*idx as usize].has_key(),

        S::Struct(idx) => [RESOLVED_SUI_ID, RESOLVED_ASCII_STR, RESOLVED_UTF8_STR]
            .contains(&resolve_struct(view, *idx)),

        S::StructInstantiation(idx, targs) => {
            let resolved_struct = resolve_struct(view, *idx);
            // is option of a primitive
            resolved_struct == RESOLVED_STD_OPTION
                && targs.len() == 1
                && is_primitive(view, function_type_args, &targs[0])
        }

        S::Vector(inner) => is_primitive(view, function_type_args, inner),
        S::Reference(_) | S::MutableReference(_) => false,
    }
}

pub fn is_object(
    view: &BinaryIndexedView<'_>,
    function_type_args: &[AbilitySet],
    t: &SignatureToken,
) -> Result<bool, String> {
    use SignatureToken as S;
    match t {
        S::Reference(inner) | S::MutableReference(inner) => {
            is_object(view, function_type_args, inner)
        }
        _ => is_object_struct(view, function_type_args, t),
    }
}

pub fn is_object_vector(
    view: &BinaryIndexedView<'_>,
    function_type_args: &[AbilitySet],
    t: &SignatureToken,
) -> Result<bool, String> {
    use SignatureToken as S;
    match t {
        S::Vector(inner) => is_object_struct(view, function_type_args, inner),
        _ => is_object_struct(view, function_type_args, t),
    }
}

fn is_object_struct(
    view: &BinaryIndexedView<'_>,
    function_type_args: &[AbilitySet],
    s: &SignatureToken,
) -> Result<bool, String> {
    use SignatureToken as S;
    match s {
        S::Bool
        | S::U8
        | S::U16
        | S::U32
        | S::U64
        | S::U128
        | S::U256
        | S::Address
        | S::Signer
        | S::Vector(_)
        | S::Reference(_)
        | S::MutableReference(_) => Ok(false),
        S::TypeParameter(idx) => Ok(function_type_args
            .get(*idx as usize)
            .map(|abs| abs.has_key())
            .unwrap_or(false)),
        S::Struct(_) | S::StructInstantiation(_, _) => {
            let abilities = view
                .abilities(s, function_type_args)
                .map_err(|vm_err| vm_err.to_string())?;
            Ok(abilities.has_key())
        }
    }
}
