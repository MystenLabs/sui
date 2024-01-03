// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod verifier;

pub mod entry_points_verifier;
pub mod global_storage_access_verifier;
pub mod id_leak_verifier;
pub mod meter;
pub mod one_time_witness_verifier;
pub mod private_generics;
pub mod struct_with_key_verifier;

use move_core_types::{ident_str, identifier::IdentStr, vm_status::StatusCode};
use move_vm_config::verifier::VerifierConfig;
use sui_protocol_config::ProtocolConfig;
use sui_types::error::{ExecutionError, ExecutionErrorKind};

pub const INIT_FN_NAME: &IdentStr = ident_str!("init");
pub const TEST_SCENARIO_MODULE_NAME: &str = "test_scenario";

fn verification_failure(error: String) -> ExecutionError {
    ExecutionError::new_with_source(ExecutionErrorKind::SuiMoveVerificationError, error)
}

fn to_verification_timeout_error(error: String) -> ExecutionError {
    ExecutionError::new_with_source(ExecutionErrorKind::SuiMoveVerificationTimedout, error)
}

/// Runs the Move verifier and checks if the error counts as a Move verifier timeout
/// NOTE: this function only check if the verifier error is a timeout
/// All other errors are ignored
pub fn check_for_verifier_timeout(major_status_code: &StatusCode) -> bool {
    [
        StatusCode::PROGRAM_TOO_COMPLEX,
        // Do we want to make this a substatus of `PROGRAM_TOO_COMPLEX`?
        StatusCode::TOO_MANY_BACK_EDGES,
    ]
    .contains(major_status_code)
}

pub fn default_verifier_config(
    protocol_config: &ProtocolConfig,
    is_metered: bool,
) -> VerifierConfig {
    let (
        max_back_edges_per_function,
        max_back_edges_per_module,
        max_per_fun_meter_units,
        max_per_mod_meter_units,
    ) = if is_metered {
        (
            Some(protocol_config.max_back_edges_per_function() as usize),
            Some(protocol_config.max_back_edges_per_module() as usize),
            Some(protocol_config.max_verifier_meter_ticks_per_function() as u128),
            Some(protocol_config.max_meter_ticks_per_module() as u128),
        )
    } else {
        (None, None, None, None)
    };

    VerifierConfig {
        max_loop_depth: Some(protocol_config.max_loop_depth() as usize),
        max_generic_instantiation_length: Some(
            protocol_config.max_generic_instantiation_length() as usize
        ),
        max_function_parameters: Some(protocol_config.max_function_parameters() as usize),
        max_basic_blocks: Some(protocol_config.max_basic_blocks() as usize),
        max_value_stack_size: protocol_config.max_value_stack_size() as usize,
        max_type_nodes: Some(protocol_config.max_type_nodes() as usize),
        max_push_size: Some(protocol_config.max_push_size() as usize),
        max_dependency_depth: Some(protocol_config.max_dependency_depth() as usize),
        max_fields_in_struct: Some(protocol_config.max_fields_in_struct() as usize),
        max_function_definitions: Some(protocol_config.max_function_definitions() as usize),
        max_struct_definitions: Some(protocol_config.max_struct_definitions() as usize),
        max_constant_vector_len: Some(protocol_config.max_move_vector_len()),
        max_back_edges_per_function,
        max_back_edges_per_module,
        max_basic_blocks_in_script: None,
        max_per_fun_meter_units,
        max_per_mod_meter_units,
        max_idenfitier_len: protocol_config.max_move_identifier_len_as_option(), // Before protocol version 9, there was no limit
        allow_receiving_object_id: protocol_config.allow_receiving_object_id(),
    }
}
