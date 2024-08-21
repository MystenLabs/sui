// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format_common::VERSION_MAX;

pub const DEFAULT_MAX_CONSTANT_VECTOR_LEN: u64 = 1024 * 1024;
pub const DEFAULT_MAX_IDENTIFIER_LENGTH: u64 = 128;
pub const DEFAULT_MAX_VARIANTS: u64 = 127;

#[derive(Debug, Clone)]
pub struct VerifierConfig {
    pub max_loop_depth: Option<usize>,
    pub max_function_parameters: Option<usize>,
    pub max_generic_instantiation_length: Option<usize>,
    pub max_basic_blocks: Option<usize>,
    pub max_value_stack_size: usize,
    pub max_type_nodes: Option<usize>,
    pub max_push_size: Option<usize>,
    pub max_dependency_depth: Option<usize>,
    pub max_data_definitions: Option<usize>,
    pub max_fields_in_struct: Option<usize>,
    pub max_function_definitions: Option<usize>,
    pub max_constant_vector_len: Option<u64>,
    pub max_back_edges_per_function: Option<usize>,
    pub max_back_edges_per_module: Option<usize>,
    pub max_basic_blocks_in_script: Option<usize>,
    pub max_idenfitier_len: Option<u64>,
    pub allow_receiving_object_id: bool,
    pub reject_mutable_random_on_entry_functions: bool,
    pub bytecode_version: u32,
    pub max_variants_in_enum: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct MeterConfig {
    pub max_per_fun_meter_units: Option<u128>,
    pub max_per_mod_meter_units: Option<u128>,
    pub max_per_pkg_meter_units: Option<u128>,
}

impl Default for VerifierConfig {
    fn default() -> Self {
        Self {
            max_loop_depth: None,
            max_function_parameters: None,
            max_generic_instantiation_length: None,
            max_basic_blocks: None,
            max_type_nodes: None,
            // Max size set to 1024 to match the size limit in the interpreter.
            max_value_stack_size: 1024,
            // Max number of pushes in one function
            max_push_size: None,
            // Max depth in dependency tree for both direct and friend dependencies
            max_dependency_depth: None,
            // Max count of structs in a module
            max_data_definitions: None,
            // Max count of fields in a struct
            max_fields_in_struct: None,
            // Max count of functions in a module
            max_function_definitions: None,
            // Max size set to 10000 to restrict number of pushes in one function
            // max_push_size: Some(10000),
            // max_dependency_depth: Some(100),
            // max_data_definitions: Some(200),
            // max_fields_in_struct: Some(30),
            // max_function_definitions: Some(1000),
            max_back_edges_per_function: None,
            max_back_edges_per_module: None,
            max_basic_blocks_in_script: None,
            max_constant_vector_len: Some(DEFAULT_MAX_CONSTANT_VECTOR_LEN),
            max_idenfitier_len: Some(DEFAULT_MAX_IDENTIFIER_LENGTH),
            allow_receiving_object_id: true,
            reject_mutable_random_on_entry_functions: true,
            bytecode_version: VERSION_MAX,
            max_variants_in_enum: Some(DEFAULT_MAX_VARIANTS),
        }
    }
}

impl MeterConfig {
    pub fn old_default() -> Self {
        Self {
            max_per_fun_meter_units: Some(8_000_000),
            max_per_mod_meter_units: Some(8_000_000),
            max_per_pkg_meter_units: Some(8_000_000),
        }
    }
}

impl Default for MeterConfig {
    fn default() -> Self {
        Self {
            max_per_fun_meter_units: Some(2_200_000),
            max_per_mod_meter_units: Some(2_200_000),
            max_per_pkg_meter_units: Some(2_200_000),
        }
    }
}

impl MeterConfig {
    pub fn unbounded() -> Self {
        Self {
            max_per_fun_meter_units: None,
            max_per_mod_meter_units: None,
            max_per_pkg_meter_units: None,
        }
    }
}
