// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format_common::VERSION_MAX;
use move_vm_config::verifier::{
    MeterConfig, VerifierConfig, DEFAULT_MAX_CONSTANT_VECTOR_LEN, DEFAULT_MAX_IDENTIFIER_LENGTH,
    DEFAULT_MAX_VARIANTS,
};

pub mod binary_samples;
pub mod bounds_tests;
pub mod code_unit_tests;
pub mod constants_tests;
pub mod control_flow_tests;
pub mod duplication_tests;
pub mod generic_ops_tests;
pub mod large_type_test;
pub mod limit_tests;
pub mod locals;
pub mod loop_summary_tests;
pub mod many_back_edges;
pub mod negative_stack_size_tests;
pub mod reference_safety_tests;
pub mod signature_tests;
pub mod vec_pack_tests;

/// Configuration used in production.
pub(crate) fn production_config() -> (VerifierConfig, MeterConfig) {
    (
        VerifierConfig {
            max_loop_depth: Some(5),
            max_generic_instantiation_length: Some(32),
            max_function_parameters: Some(128),
            max_basic_blocks: Some(1024),
            max_basic_blocks_in_script: Some(1024),
            max_value_stack_size: 1024,
            max_type_nodes: Some(256),
            max_push_size: Some(10000),
            max_dependency_depth: Some(100),
            max_data_definitions: Some(200),
            max_fields_in_struct: Some(30),
            max_function_definitions: Some(1000),

            // Do not use back edge constraints as they are superseded by metering
            max_back_edges_per_function: None,
            max_back_edges_per_module: None,

            max_constant_vector_len: Some(DEFAULT_MAX_CONSTANT_VECTOR_LEN),
            max_idenfitier_len: Some(DEFAULT_MAX_IDENTIFIER_LENGTH),
            allow_receiving_object_id: true,
            reject_mutable_random_on_entry_functions: true,
            bytecode_version: VERSION_MAX,
            max_variants_in_enum: Some(DEFAULT_MAX_VARIANTS),
        },
        MeterConfig::old_default(),
    )
}
