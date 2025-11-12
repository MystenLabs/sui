// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::verifier::{DEFAULT_MAX_CONSTANT_VECTOR_LEN, VerifierConfig};
use move_binary_format::binary_config::BinaryConfig;
use move_binary_format::file_format_common::VERSION_MAX;

pub const DEFAULT_MAX_VALUE_NEST_DEPTH: u64 = 128;

/// Dynamic config options for the Move VM.
pub struct VMConfig {
    pub verifier: VerifierConfig,
    pub max_binary_format_version: u32,
    pub runtime_limits_config: VMRuntimeLimitsConfig,
    // When this flag is set to true, MoveVM will check invariant violation in swap_loc
    pub enable_invariant_violation_check_in_swap_loc: bool,
    // When this flag is set to true, MoveVM will check that there are no trailing bytes after
    // deserializing and check for no metadata bytes
    pub check_no_extraneous_bytes_during_deserialization: bool,
    // When this flag is set to true, errors from the VM will be augmented with execution state
    // (stacktrace etc.)
    pub error_execution_state: bool,
    // configuration for binary deserialization (modules)
    pub binary_config: BinaryConfig,
    // Whether value serialization errors when generating type layouts should be rethrown or
    // converted to a different error.
    pub rethrow_serialization_type_layout_errors: bool,
    /// Maximal nodes which are allowed when converting to layout. This includes the types of
    /// fields for struct types.
    pub max_type_to_layout_nodes: Option<u64>,
    /// Count variants as nodes.
    pub variant_nodes: bool,
    /// Check for deprecated global storage operations during deserialization.
    pub deprecate_global_storage_ops_during_deserialization: bool,
}

impl Default for VMConfig {
    fn default() -> Self {
        Self {
            verifier: VerifierConfig::default(),
            max_binary_format_version: VERSION_MAX,
            runtime_limits_config: VMRuntimeLimitsConfig::default(),
            enable_invariant_violation_check_in_swap_loc: true,
            check_no_extraneous_bytes_during_deserialization: true,
            error_execution_state: true,
            binary_config: BinaryConfig::legacy_with_flags(
                /* check_no_extraneous_bytes */ true,
                /* deprecate_global_storage_ops */ false,
            ),
            rethrow_serialization_type_layout_errors: false,
            max_type_to_layout_nodes: Some(512),
            variant_nodes: true,
            deprecate_global_storage_ops_during_deserialization: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VMRuntimeLimitsConfig {
    /// Maximum number of items that can be pushed into a vec
    pub vector_len_max: u64,
    /// Maximum value nest depth for structs
    pub max_value_nest_depth: Option<u64>,
    // include hardened OTW checks
    pub hardened_otw_check: bool,
}

impl Default for VMRuntimeLimitsConfig {
    fn default() -> Self {
        Self {
            vector_len_max: DEFAULT_MAX_CONSTANT_VECTOR_LEN,
            max_value_nest_depth: Some(DEFAULT_MAX_VALUE_NEST_DEPTH),
            hardened_otw_check: true,
        }
    }
}
