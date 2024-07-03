// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    binary_config::{BinaryConfig, TableConfig},
    file_format_common::VERSION_1,
};
use sui_protocol_config::ProtocolConfig;

/// Build a `BinaryConfig` from a `ProtocolConfig`
pub fn to_binary_config(protocol_config: &ProtocolConfig) -> BinaryConfig {
    BinaryConfig::new(
        protocol_config.move_binary_format_version(),
        protocol_config
            .min_move_binary_format_version_as_option()
            .unwrap_or(VERSION_1),
        protocol_config.no_extraneous_module_bytes(),
        TableConfig {
            module_handles: protocol_config
                .binary_module_handles_as_option()
                .unwrap_or(u16::MAX),
            datatype_handles: protocol_config
                .binary_struct_handles_as_option()
                .unwrap_or(u16::MAX),
            function_handles: protocol_config
                .binary_function_handles_as_option()
                .unwrap_or(u16::MAX),
            function_instantiations: protocol_config
                .binary_function_instantiations_as_option()
                .unwrap_or(u16::MAX),
            signatures: protocol_config
                .binary_signatures_as_option()
                .unwrap_or(u16::MAX),
            constant_pool: protocol_config
                .binary_constant_pool_as_option()
                .unwrap_or(u16::MAX),
            identifiers: protocol_config
                .binary_identifiers_as_option()
                .unwrap_or(u16::MAX),
            address_identifiers: protocol_config
                .binary_address_identifiers_as_option()
                .unwrap_or(u16::MAX),
            struct_defs: protocol_config
                .binary_struct_defs_as_option()
                .unwrap_or(u16::MAX),
            struct_def_instantiations: protocol_config
                .binary_struct_def_instantiations_as_option()
                .unwrap_or(u16::MAX),
            function_defs: protocol_config
                .binary_function_defs_as_option()
                .unwrap_or(u16::MAX),
            field_handles: protocol_config
                .binary_field_handles_as_option()
                .unwrap_or(u16::MAX),
            field_instantiations: protocol_config
                .binary_field_instantiations_as_option()
                .unwrap_or(u16::MAX),
            friend_decls: protocol_config
                .binary_friend_decls_as_option()
                .unwrap_or(u16::MAX),
            enum_defs: protocol_config
                .binary_enum_defs_as_option()
                .unwrap_or(u16::MAX),
            enum_def_instantiations: protocol_config
                .binary_enum_def_instantiations_as_option()
                .unwrap_or(u16::MAX),
            variant_handles: protocol_config
                .binary_variant_handles_as_option()
                .unwrap_or(u16::MAX),
            variant_instantiation_handles: protocol_config
                .binary_variant_instantiation_handles_as_option()
                .unwrap_or(u16::MAX),
        },
    )
}
