// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::file_format_common::{VERSION_1, VERSION_MAX};

/// Configuration for the binary format related to table size.
/// Maps to all tables in the binary format.
#[derive(Clone, Debug)]
pub struct TableConfig {
    pub module_handles: u16,
    pub datatype_handles: u16,
    pub function_handles: u16,
    pub function_instantiations: u16,
    pub signatures: u16,
    pub constant_pool: u16,
    pub identifiers: u16,
    pub address_identifiers: u16,
    pub struct_defs: u16,
    pub struct_def_instantiations: u16,
    pub function_defs: u16,
    pub field_handles: u16,
    pub field_instantiations: u16,
    pub friend_decls: u16,
    pub enum_defs: u16,
    pub enum_def_instantiations: u16,
    pub variant_handles: u16,
    pub variant_instantiation_handles: u16,
}

impl TableConfig {
    // The deserializer and other parts of the system already have limits in place,
    // this is the "legacy" configuration that is effectively the "no limits" setup.
    // This table is a noop with `u16::MAX`.
    pub fn legacy() -> Self {
        TableConfig {
            module_handles: u16::MAX,
            datatype_handles: u16::MAX,
            function_handles: u16::MAX,
            function_instantiations: u16::MAX,
            signatures: u16::MAX,
            constant_pool: u16::MAX,
            identifiers: u16::MAX,
            address_identifiers: u16::MAX,
            struct_defs: u16::MAX,
            struct_def_instantiations: u16::MAX,
            function_defs: u16::MAX,
            field_handles: u16::MAX,
            field_instantiations: u16::MAX,
            friend_decls: u16::MAX,
            // These can be any number
            enum_defs: u16::MAX,
            enum_def_instantiations: u16::MAX,
            variant_handles: 1024,
            variant_instantiation_handles: 1024,
        }
    }
}

/// Configuration information for deserializing a binary.
/// Controls multiple aspects of the deserialization process.
#[derive(Clone, Debug)]
pub struct BinaryConfig {
    pub max_binary_format_version: u32,
    pub min_binary_format_version: u32,
    pub check_no_extraneous_bytes: bool,
    pub table_config: TableConfig,
}

impl BinaryConfig {
    pub fn new(
        max_binary_format_version: u32,
        min_binary_format_version: u32,
        check_no_extraneous_bytes: bool,
        table_config: TableConfig,
    ) -> Self {
        Self {
            max_binary_format_version,
            min_binary_format_version,
            check_no_extraneous_bytes,
            table_config,
        }
    }

    // We want to make this disappear from the public API in favor of a "true" config
    pub fn legacy(
        max_binary_format_version: u32,
        min_binary_format_version: u32,
        check_no_extraneous_bytes: bool,
    ) -> Self {
        Self {
            max_binary_format_version,
            min_binary_format_version,
            check_no_extraneous_bytes,
            table_config: TableConfig::legacy(),
        }
    }

    /// Run always with the max version but with controllable "extraneous bytes check"
    pub fn with_extraneous_bytes_check(check_no_extraneous_bytes: bool) -> Self {
        Self {
            max_binary_format_version: VERSION_MAX,
            min_binary_format_version: VERSION_1,
            check_no_extraneous_bytes,
            table_config: TableConfig::legacy(),
        }
    }

    /// VERSION_MAX and check_no_extraneous_bytes = true
    /// common "standard/default" in code base now
    pub fn standard() -> Self {
        Self {
            max_binary_format_version: VERSION_MAX,
            min_binary_format_version: VERSION_1,
            check_no_extraneous_bytes: true,
            table_config: TableConfig::legacy(),
        }
    }
}
