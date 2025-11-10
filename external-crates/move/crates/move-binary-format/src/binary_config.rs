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
    pub deprecate_global_storage_ops: bool,
    pub table_config: TableConfig,
    allow_unpublishable: bool,
}

impl BinaryConfig {
    pub fn new(
        max_binary_format_version: u32,
        min_binary_format_version: u32,
        check_no_extraneous_bytes: bool,
        deprecate_global_storage_ops: bool,
        table_config: TableConfig,
    ) -> Self {
        Self {
            max_binary_format_version,
            min_binary_format_version,
            check_no_extraneous_bytes,
            deprecate_global_storage_ops,
            table_config,
            allow_unpublishable: false,
        }
    }

    /// Creates a legacy configuration using the legacy table config.
    pub fn legacy(
        max_binary_format_version: u32,
        min_binary_format_version: u32,
        check_no_extraneous_bytes: bool,
        deprecate_global_storage_ops: bool,
    ) -> Self {
        Self::new(
            max_binary_format_version,
            min_binary_format_version,
            check_no_extraneous_bytes,
            deprecate_global_storage_ops,
            TableConfig::legacy(),
        )
    }

    /// Creates a configuration with max version, legacy table config,
    /// and controllable extraneous bytes check and deprecate_global_storage_ops flag.
    pub fn legacy_with_flags(
        check_no_extraneous_bytes: bool,
        deprecate_global_storage_ops: bool,
    ) -> Self {
        Self::legacy(
            VERSION_MAX,
            VERSION_1,
            check_no_extraneous_bytes,
            deprecate_global_storage_ops,
        )
    }

    /// Standard configuration: VERSION_MAX and check_no_extraneous_bytes = true
    pub fn standard() -> Self {
        Self::legacy_with_flags(
            /* check_no_extraneous_bytes */ true, /* deprecate_global_storage_ops */ true,
        )
    }

    pub fn new_unpublishable() -> Self {
        Self {
            max_binary_format_version: VERSION_MAX,
            min_binary_format_version: VERSION_1,
            check_no_extraneous_bytes: true,
            deprecate_global_storage_ops: true,
            table_config: TableConfig::legacy(),
            allow_unpublishable: true,
        }
    }

    pub fn allow_unpublishable(&self) -> bool {
        self.allow_unpublishable
    }
}
