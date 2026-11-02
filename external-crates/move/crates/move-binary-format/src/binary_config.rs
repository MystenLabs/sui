// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::file_format_common::*;
use serde::{Deserialize, Serialize};

macro_rules! binary_format_limits {
    (
        $($field:ident),* $(,)?;
        $(
            profile $profile_name:ident {
                $($config_field:ident: $config_value:expr),* $(,)?
            }
        )*
    ) => {
        /// Limits for ULEB128-encoded binary format values.
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct BinaryFormatLimits {
            $($field: u64,)*
        }

        impl BinaryFormatLimits {
            $(
                pub const $profile_name: Self = Self {
                    $($config_field: $config_value,)*
                };
            )*

            $(pub fn $field(&self) -> u64 {
                self.$field
            })*
        }

        #[allow(non_camel_case_types)]
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub enum BinaryFormatLimitsProfile {
            $($profile_name,)*
        }

        impl BinaryFormatLimitsProfile {
            pub const fn limits(self) -> &'static BinaryFormatLimits {
                match self {
                    $(Self::$profile_name => &BinaryFormatLimits::$profile_name,)*
                }
            }
        }
    };
}

binary_format_limits! {
    signature_index_max,
    module_handle_index_max,
    identifier_index_max,
    datatype_handle_index_max,
    address_index_max,
    struct_def_index_max,
    enum_def_index_max,
    variant_handle_index_max,
    variant_instantiation_handle_index_max,
    function_handle_index_max,
    field_handle_index_max,
    field_inst_index_max,
    function_inst_index_max,
    struct_def_inst_index_max,
    enum_def_inst_index_max,
    constant_index_max,
    bytecode_count_max,
    bytecode_index_max,
    acquires_count_max,
    field_count_max,
    variant_tag_max_value,
    variant_count_max,
    jump_table_index_max,
    type_parameter_count_max,
    signature_size_max,
    constant_size_max,
    metadata_key_size_max,
    metadata_value_size_max,
    identifier_size_max,
    type_parameter_index_max,
    field_offset_max,
    table_count_max,
    table_offset_max,
    table_size_max,
    local_index_max;

    profile LEGACY {
        signature_index_max: SIGNATURE_INDEX_MAX,
        module_handle_index_max: MODULE_HANDLE_INDEX_MAX,
        identifier_index_max: IDENTIFIER_INDEX_MAX,
        datatype_handle_index_max: DATATYPE_HANDLE_INDEX_MAX,
        address_index_max: ADDRESS_INDEX_MAX,
        struct_def_index_max: STRUCT_DEF_INDEX_MAX,
        enum_def_index_max: ENUM_DEF_INDEX_MAX,
        variant_handle_index_max: VARIANT_HANDLE_INDEX_MAX,
        variant_instantiation_handle_index_max: VARIANT_INSTANTIATION_HANDLE_INDEX_MAX,
        function_handle_index_max: FUNCTION_HANDLE_INDEX_MAX,
        field_handle_index_max: FIELD_HANDLE_INDEX_MAX,
        field_inst_index_max: FIELD_INST_INDEX_MAX,
        function_inst_index_max: FUNCTION_INST_INDEX_MAX,
        struct_def_inst_index_max: STRUCT_DEF_INST_INDEX_MAX,
        enum_def_inst_index_max: ENUM_DEF_INST_INDEX_MAX,
        constant_index_max: CONSTANT_INDEX_MAX,
        bytecode_count_max: BYTECODE_COUNT_MAX,
        bytecode_index_max: BYTECODE_INDEX_MAX,
        acquires_count_max: ACQUIRES_COUNT_MAX,
        field_count_max: FIELD_COUNT_MAX,
        variant_tag_max_value: VARIANT_TAG_MAX_VALUE,
        variant_count_max: VARIANT_COUNT_MAX,
        jump_table_index_max: JUMP_TABLE_INDEX_MAX,
        type_parameter_count_max: TYPE_PARAMETER_COUNT_MAX,
        signature_size_max: SIGNATURE_SIZE_MAX,
        constant_size_max: CONSTANT_SIZE_MAX,
        metadata_key_size_max: METADATA_KEY_SIZE_MAX,
        metadata_value_size_max: METADATA_VALUE_SIZE_MAX,
        identifier_size_max: IDENTIFIER_SIZE_MAX,
        type_parameter_index_max: LEGACY_TYPE_PARAMETER_INDEX_MAX,
        field_offset_max: FIELD_OFFSET_MAX,
        table_count_max: TABLE_COUNT_MAX,
        table_offset_max: TABLE_OFFSET_MAX,
        table_size_max: TABLE_SIZE_MAX,
        local_index_max: LOCAL_INDEX_MAX,
    }

    profile V1 {
        signature_index_max: SIGNATURE_INDEX_MAX,
        module_handle_index_max: MODULE_HANDLE_INDEX_MAX,
        identifier_index_max: IDENTIFIER_INDEX_MAX,
        datatype_handle_index_max: DATATYPE_HANDLE_INDEX_MAX,
        address_index_max: ADDRESS_INDEX_MAX,
        struct_def_index_max: STRUCT_DEF_INDEX_MAX,
        enum_def_index_max: ENUM_DEF_INDEX_MAX,
        variant_handle_index_max: VARIANT_HANDLE_INDEX_MAX,
        variant_instantiation_handle_index_max: VARIANT_INSTANTIATION_HANDLE_INDEX_MAX,
        function_handle_index_max: FUNCTION_HANDLE_INDEX_MAX,
        field_handle_index_max: FIELD_HANDLE_INDEX_MAX,
        field_inst_index_max: FIELD_INST_INDEX_MAX,
        function_inst_index_max: FUNCTION_INST_INDEX_MAX,
        struct_def_inst_index_max: STRUCT_DEF_INST_INDEX_MAX,
        enum_def_inst_index_max: ENUM_DEF_INST_INDEX_MAX,
        constant_index_max: CONSTANT_INDEX_MAX,
        bytecode_count_max: BYTECODE_COUNT_MAX,
        bytecode_index_max: BYTECODE_INDEX_MAX,
        acquires_count_max: ACQUIRES_COUNT_MAX,
        field_count_max: FIELD_COUNT_MAX,
        variant_tag_max_value: VARIANT_TAG_MAX_VALUE,
        variant_count_max: VARIANT_COUNT_MAX,
        jump_table_index_max: JUMP_TABLE_INDEX_MAX,
        type_parameter_count_max: TYPE_PARAMETER_COUNT_MAX,
        signature_size_max: SIGNATURE_SIZE_MAX,
        constant_size_max: CONSTANT_SIZE_MAX,
        metadata_key_size_max: METADATA_KEY_SIZE_MAX,
        metadata_value_size_max: METADATA_VALUE_SIZE_MAX,
        identifier_size_max: IDENTIFIER_SIZE_MAX,
        type_parameter_index_max: TYPE_PARAMETER_INDEX_MAX,
        field_offset_max: FIELD_OFFSET_MAX,
        table_count_max: TABLE_COUNT_MAX,
        table_offset_max: TABLE_OFFSET_MAX,
        table_size_max: TABLE_SIZE_MAX,
        local_index_max: LOCAL_INDEX_MAX,
    }
}

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
            variant_handles: u16::MAX,
            variant_instantiation_handles: u16::MAX,
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
    pub binary_format_limits: &'static BinaryFormatLimits,
    allow_unpublishable: bool,
}

impl BinaryConfig {
    pub fn new(
        max_binary_format_version: u32,
        min_binary_format_version: u32,
        check_no_extraneous_bytes: bool,
        deprecate_global_storage_ops: bool,
        table_config: TableConfig,
        binary_format_limits: &'static BinaryFormatLimits,
    ) -> Self {
        Self {
            max_binary_format_version,
            min_binary_format_version,
            check_no_extraneous_bytes,
            deprecate_global_storage_ops,
            table_config,
            binary_format_limits,
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
            BinaryFormatLimitsProfile::LEGACY.limits(),
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
            binary_format_limits: BinaryFormatLimitsProfile::V1.limits(),
            allow_unpublishable: true,
        }
    }

    pub fn allow_unpublishable(&self) -> bool {
        self.allow_unpublishable
    }

    /// This function exists solely for propagating a VMConfig down into the unit testing
    /// environment. It should never be called anywhere else.
    #[cfg(feature = "testing")]
    pub fn allow_unpublishable_for_move_unit_testing(&mut self) {
        self.allow_unpublishable = true;
    }
}
