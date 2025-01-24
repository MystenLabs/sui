// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use base_types::{SequenceNumber, SuiAddress};
use move_binary_format::file_format::{AbilitySet, SignatureToken};
use move_binary_format::CompiledModule;
use move_bytecode_utils::resolve_struct;
use move_core_types::language_storage::ModuleId;
use move_core_types::{account_address::AccountAddress, language_storage::StructTag};
pub use move_core_types::{identifier::Identifier, language_storage::TypeTag};
use object::OBJECT_START_VERSION;

use base_types::ObjectID;

pub use mysten_network::multiaddr;

use crate::base_types::{RESOLVED_ASCII_STR, RESOLVED_UTF8_STR};
use crate::{base_types::RESOLVED_STD_OPTION, id::RESOLVED_SUI_ID};

#[macro_use]
pub mod error;

pub mod accumulator;
pub mod authenticator_state;
pub mod balance;
pub mod base_types;
pub mod bridge;
pub mod clock;
pub mod coin;
pub mod collection_types;
pub mod committee;
pub mod config;
pub mod crypto;
pub mod deny_list_v1;
pub mod deny_list_v2;
pub mod digests;
pub mod display;
pub mod dynamic_field;
pub mod effects;
pub mod epoch_data;
pub mod event;
pub mod executable_transaction;
pub mod execution;
pub mod execution_config_utils;
pub mod execution_status;
pub mod full_checkpoint_content;
pub mod gas;
pub mod gas_coin;
pub mod gas_model;
pub mod governance;
pub mod id;
pub mod in_memory_storage;
pub mod inner_temporary_store;
pub mod layout_resolver;
pub mod message_envelope;
pub mod messages_checkpoint;
pub mod messages_consensus;
pub mod messages_grpc;
pub mod messages_safe_client;
pub mod metrics;
pub mod mock_checkpoint_builder;
pub mod move_package;
pub mod multisig;
pub mod multisig_legacy;
pub mod nitro_attestation;
pub mod object;
pub mod passkey_authenticator;
pub mod programmable_transaction_builder;
pub mod quorum_driver_types;
pub mod randomness_state;
pub mod signature;
pub mod signature_verification;
pub mod storage;
pub mod sui_sdk_types_conversions;
pub mod sui_serde;
pub mod sui_system_state;
pub mod supported_protocol_versions;
pub mod test_checkpoint_data_builder;
pub mod traffic_control;
pub mod transaction;
pub mod transaction_executor;
pub mod transfer;
pub mod type_input;
pub mod versioned;
pub mod zk_login_authenticator;
pub mod zk_login_util;

#[path = "./unit_tests/utils.rs"]
pub mod utils;

macro_rules! built_in_ids {
    ($($addr:ident / $id:ident = $init:expr);* $(;)?) => {
        $(
            pub const $addr: AccountAddress = AccountAddress::from_suffix($init);
            pub const $id: ObjectID = ObjectID::from_address($addr);
        )*
    }
}

macro_rules! built_in_pkgs {
    ($($addr:ident / $id:ident = $init:expr);* $(;)?) => {
        built_in_ids! { $($addr / $id = $init;)* }
        pub const SYSTEM_PACKAGE_ADDRESSES: &[AccountAddress] = &[$($addr),*];
        pub fn is_system_package(addr: impl Into<AccountAddress>) -> bool {
            matches!(addr.into(), $($addr)|*)
        }
    }
}

built_in_pkgs! {
    MOVE_STDLIB_ADDRESS / MOVE_STDLIB_PACKAGE_ID = 0x1;
    SUI_FRAMEWORK_ADDRESS / SUI_FRAMEWORK_PACKAGE_ID = 0x2;
    SUI_SYSTEM_ADDRESS / SUI_SYSTEM_PACKAGE_ID = 0x3;
    BRIDGE_ADDRESS / BRIDGE_PACKAGE_ID = 0xb;
    DEEPBOOK_ADDRESS / DEEPBOOK_PACKAGE_ID = 0xdee9;
}

built_in_ids! {
    SUI_SYSTEM_STATE_ADDRESS / SUI_SYSTEM_STATE_OBJECT_ID = 0x5;
    SUI_CLOCK_ADDRESS / SUI_CLOCK_OBJECT_ID = 0x6;
    SUI_AUTHENTICATOR_STATE_ADDRESS / SUI_AUTHENTICATOR_STATE_OBJECT_ID = 0x7;
    SUI_RANDOMNESS_STATE_ADDRESS / SUI_RANDOMNESS_STATE_OBJECT_ID = 0x8;
    SUI_BRIDGE_ADDRESS / SUI_BRIDGE_OBJECT_ID = 0x9;
    SUI_DENY_LIST_ADDRESS / SUI_DENY_LIST_OBJECT_ID = 0x403;
}

pub const SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION: SequenceNumber = OBJECT_START_VERSION;
pub const SUI_CLOCK_OBJECT_SHARED_VERSION: SequenceNumber = OBJECT_START_VERSION;
pub const SUI_AUTHENTICATOR_STATE_OBJECT_SHARED_VERSION: SequenceNumber = OBJECT_START_VERSION;

pub fn sui_framework_address_concat_string(suffix: &str) -> String {
    format!("{}{suffix}", SUI_FRAMEWORK_ADDRESS.to_hex_literal())
}

/// Parses `s` as an address. Valid formats for addresses are:
///
/// - A 256bit number, encoded in decimal, or hexadecimal with a leading "0x" prefix.
/// - One of a number of pre-defined named addresses: std, sui, sui_system, deepbook.
///
/// Parsing succeeds if and only if `s` matches one of these formats exactly, with no remaining
/// suffix. This function is intended for use within the authority codebases.
pub fn parse_sui_address(s: &str) -> anyhow::Result<SuiAddress> {
    use move_core_types::parsing::address::ParsedAddress;
    Ok(ParsedAddress::parse(s)?
        .into_account_address(&resolve_address)?
        .into())
}

/// Parse `s` as a Module ID: An address (see `parse_sui_address`), followed by `::`, and then a
/// module name (an identifier). Parsing succeeds if and only if `s` matches this format exactly,
/// with no remaining input. This function is intended for use within the authority codebases.
pub fn parse_sui_module_id(s: &str) -> anyhow::Result<ModuleId> {
    use move_core_types::parsing::types::ParsedModuleId;
    ParsedModuleId::parse(s)?.into_module_id(&resolve_address)
}

/// Parse `s` as a fully-qualified name: A Module ID (see `parse_sui_module_id`), followed by `::`,
/// and then an identifier (for the module member). Parsing succeeds if and only if `s` matches this
/// format exactly, with no remaining input. This function is intended for use within the authority
/// codebases.
pub fn parse_sui_fq_name(s: &str) -> anyhow::Result<(ModuleId, String)> {
    use move_core_types::parsing::types::ParsedFqName;
    ParsedFqName::parse(s)?.into_fq_name(&resolve_address)
}

/// Parse `s` as a struct type: A fully-qualified name, optionally followed by a list of type
/// parameters (types -- see `parse_sui_type_tag`, separated by commas, surrounded by angle
/// brackets). Parsing succeeds if and only if `s` matches this format exactly, with no remaining
/// input. This function is intended for use within the authority codebase.
pub fn parse_sui_struct_tag(s: &str) -> anyhow::Result<StructTag> {
    use move_core_types::parsing::types::ParsedStructType;
    ParsedStructType::parse(s)?.into_struct_tag(&resolve_address)
}

/// Parse `s` as a type: Either a struct type (see `parse_sui_struct_tag`), a primitive type, or a
/// vector with a type parameter. Parsing succeeds if and only if `s` matches this format exactly,
/// with no remaining input. This function is intended for use within the authority codebase.
pub fn parse_sui_type_tag(s: &str) -> anyhow::Result<TypeTag> {
    use move_core_types::parsing::types::ParsedType;
    ParsedType::parse(s)?.into_type_tag(&resolve_address)
}

/// Resolve well-known named addresses into numeric addresses.
pub fn resolve_address(addr: &str) -> Option<AccountAddress> {
    match addr {
        "deepbook" => Some(DEEPBOOK_ADDRESS),
        "std" => Some(MOVE_STDLIB_ADDRESS),
        "sui" => Some(SUI_FRAMEWORK_ADDRESS),
        "sui_system" => Some(SUI_SYSTEM_ADDRESS),
        "bridge" => Some(BRIDGE_ADDRESS),
        _ => None,
    }
}

pub trait MoveTypeTagTrait {
    fn get_type_tag() -> TypeTag;
}

impl MoveTypeTagTrait for u8 {
    fn get_type_tag() -> TypeTag {
        TypeTag::U8
    }
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

impl<T: MoveTypeTagTrait> MoveTypeTagTrait for Vec<T> {
    fn get_type_tag() -> TypeTag {
        TypeTag::Vector(Box::new(T::get_type_tag()))
    }
}

pub fn is_primitive(
    view: &CompiledModule,
    function_type_args: &[AbilitySet],
    s: &SignatureToken,
) -> bool {
    use SignatureToken as S;
    match s {
        S::Bool | S::U8 | S::U16 | S::U32 | S::U64 | S::U128 | S::U256 | S::Address => true,
        S::Signer => false,
        // optimistic, but no primitive has key
        S::TypeParameter(idx) => !function_type_args[*idx as usize].has_key(),

        S::Datatype(idx) => [RESOLVED_SUI_ID, RESOLVED_ASCII_STR, RESOLVED_UTF8_STR]
            .contains(&resolve_struct(view, *idx)),

        S::DatatypeInstantiation(inst) => {
            let (idx, targs) = &**inst;
            let resolved_struct = resolve_struct(view, *idx);
            // option is a primitive
            resolved_struct == RESOLVED_STD_OPTION
                && targs.len() == 1
                && is_primitive(view, function_type_args, &targs[0])
        }

        S::Vector(inner) => is_primitive(view, function_type_args, inner),
        S::Reference(_) | S::MutableReference(_) => false,
    }
}

pub fn is_object(
    view: &CompiledModule,
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
    view: &CompiledModule,
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
    view: &CompiledModule,
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
        S::Datatype(_) | S::DatatypeInstantiation(_) => {
            let abilities = view
                .abilities(s, function_type_args)
                .map_err(|vm_err| vm_err.to_string())?;
            Ok(abilities.has_key())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn test_parse_sui_numeric_address() {
        let result = parse_sui_address("0x2").expect("should not error");

        let expected =
            expect!["0x0000000000000000000000000000000000000000000000000000000000000002"];
        expected.assert_eq(&result.to_string());
    }

    #[test]
    fn test_parse_sui_named_address() {
        let result = parse_sui_address("sui").expect("should not error");

        let expected =
            expect!["0x0000000000000000000000000000000000000000000000000000000000000002"];
        expected.assert_eq(&result.to_string());
    }

    #[test]
    fn test_parse_sui_module_id() {
        let result = parse_sui_module_id("0x2::sui").expect("should not error");
        let expected =
            expect!["0x0000000000000000000000000000000000000000000000000000000000000002::sui"];
        expected.assert_eq(&result.to_canonical_string(/* with_prefix */ true));
    }

    #[test]
    fn test_parse_sui_fq_name() {
        let (module, name) = parse_sui_fq_name("0x2::object::new").expect("should not error");
        let expected = expect![
            "0x0000000000000000000000000000000000000000000000000000000000000002::object::new"
        ];
        expected.assert_eq(&format!(
            "{}::{name}",
            module.to_canonical_display(/* with_prefix */ true)
        ));
    }

    #[test]
    fn test_parse_sui_struct_tag_short_account_addr() {
        let result = parse_sui_struct_tag("0x2::sui::SUI").expect("should not error");

        let expected = expect!["0x2::sui::SUI"];
        expected.assert_eq(&result.to_string());

        let expected =
            expect!["0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI"];
        expected.assert_eq(&result.to_canonical_string(/* with_prefix */ true));
    }

    #[test]
    fn test_parse_sui_struct_tag_long_account_addr() {
        let result = parse_sui_struct_tag(
            "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI",
        )
        .expect("should not error");

        let expected = expect!["0x2::sui::SUI"];
        expected.assert_eq(&result.to_string());

        let expected =
            expect!["0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI"];
        expected.assert_eq(&result.to_canonical_string(/* with_prefix */ true));
    }

    #[test]
    fn test_parse_sui_struct_with_type_param_short_addr() {
        let result =
            parse_sui_struct_tag("0x2::coin::COIN<0x2::sui::SUI>").expect("should not error");

        let expected = expect!["0x2::coin::COIN<0x2::sui::SUI>"];
        expected.assert_eq(&result.to_string());

        let expected = expect!["0x0000000000000000000000000000000000000000000000000000000000000002::coin::COIN<0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>"];
        expected.assert_eq(&result.to_canonical_string(/* with_prefix */ true));
    }

    #[test]
    fn test_parse_sui_struct_with_type_param_long_addr() {
        let result = parse_sui_struct_tag("0x0000000000000000000000000000000000000000000000000000000000000002::coin::COIN<0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>")
            .expect("should not error");

        let expected = expect!["0x2::coin::COIN<0x2::sui::SUI>"];
        expected.assert_eq(&result.to_string());

        let expected = expect!["0x0000000000000000000000000000000000000000000000000000000000000002::coin::COIN<0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>"];
        expected.assert_eq(&result.to_canonical_string(/* with_prefix */ true));
    }

    #[test]
    fn test_complex_struct_tag_with_short_addr() {
        let result =
            parse_sui_struct_tag("0xe7::vec_coin::VecCoin<vector<0x2::coin::Coin<0x2::sui::SUI>>>")
                .expect("should not error");

        let expected = expect!["0xe7::vec_coin::VecCoin<vector<0x2::coin::Coin<0x2::sui::SUI>>>"];
        expected.assert_eq(&result.to_string());

        let expected = expect!["0x00000000000000000000000000000000000000000000000000000000000000e7::vec_coin::VecCoin<vector<0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>>>"];
        expected.assert_eq(&result.to_canonical_string(/* with_prefix */ true));
    }

    #[test]
    fn test_complex_struct_tag_with_long_addr() {
        let result = parse_sui_struct_tag("0x00000000000000000000000000000000000000000000000000000000000000e7::vec_coin::VecCoin<vector<0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>>>")
            .expect("should not error");

        let expected = expect!["0xe7::vec_coin::VecCoin<vector<0x2::coin::Coin<0x2::sui::SUI>>>"];
        expected.assert_eq(&result.to_string());

        let expected = expect!["0x00000000000000000000000000000000000000000000000000000000000000e7::vec_coin::VecCoin<vector<0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>>>"];
        expected.assert_eq(&result.to_canonical_string(/* with_prefix */ true));
    }

    #[test]
    fn test_dynamic_field_short_addr() {
        let result = parse_sui_struct_tag(
            "0x2::dynamic_field::Field<address, 0xdee9::custodian_v2::Account<0x234::coin::COIN>>",
        )
        .expect("should not error");

        let expected = expect![
            "0x2::dynamic_field::Field<address, 0xdee9::custodian_v2::Account<0x234::coin::COIN>>"
        ];
        expected.assert_eq(&result.to_string());

        let expected = expect!["0x0000000000000000000000000000000000000000000000000000000000000002::dynamic_field::Field<address,0x000000000000000000000000000000000000000000000000000000000000dee9::custodian_v2::Account<0x0000000000000000000000000000000000000000000000000000000000000234::coin::COIN>>"];
        expected.assert_eq(&result.to_canonical_string(/* with_prefix */ true));
    }

    #[test]
    fn test_dynamic_field_long_addr() {
        let result = parse_sui_struct_tag(
            "0x2::dynamic_field::Field<address, 0xdee9::custodian_v2::Account<0x234::coin::COIN>>",
        )
        .expect("should not error");

        let expected = expect![
            "0x2::dynamic_field::Field<address, 0xdee9::custodian_v2::Account<0x234::coin::COIN>>"
        ];
        expected.assert_eq(&result.to_string());

        let expected = expect!["0x0000000000000000000000000000000000000000000000000000000000000002::dynamic_field::Field<address,0x000000000000000000000000000000000000000000000000000000000000dee9::custodian_v2::Account<0x0000000000000000000000000000000000000000000000000000000000000234::coin::COIN>>"];
        expected.assert_eq(&result.to_canonical_string(/* with_prefix */ true));
    }
}
