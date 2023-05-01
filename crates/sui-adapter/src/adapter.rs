// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use move_binary_format::{access::ModuleAccess, file_format::CompiledModule};
use move_bytecode_verifier::meter::Meter;
use move_bytecode_verifier::{verify_module_with_config_metered, VerifierConfig};
use move_core_types::{account_address::AccountAddress, vm_status::StatusCode};
pub use move_vm_runtime::move_vm::MoveVM;
use move_vm_runtime::{
    config::{VMConfig, VMRuntimeLimitsConfig},
    native_extensions::NativeContextExtensions,
    native_functions::NativeFunctionTable,
};
use tracing::instrument;

use sui_move_natives::{object_runtime::ObjectRuntime, NativesCostTable};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::*,
    error::ExecutionError,
    error::{ExecutionErrorKind, SuiError},
    metrics::LimitsMetrics,
    object::Owner,
    storage::ChildObjectResolver,
};
use sui_verifier::verifier::sui_verify_module_metered;

sui_macros::checked_arithmetic! {

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
        max_idenfitier_len: protocol_config.max_move_identifier_len_as_option() // Before protocol version 9, there was no limit
    }
}

pub fn new_move_vm(
    natives: NativeFunctionTable,
    protocol_config: &ProtocolConfig,
    paranoid_type_checks: bool,
) -> Result<MoveVM, SuiError> {
    MoveVM::new_with_config(
        natives,
        VMConfig {
            verifier: default_verifier_config(
                protocol_config,
                false, /* we do not enable metering in execution*/
            ),
            max_binary_format_version: protocol_config.move_binary_format_version(),
            paranoid_type_checks,
            runtime_limits_config: VMRuntimeLimitsConfig {
                vector_len_max: protocol_config.max_move_vector_len(),
            },
            enable_invariant_violation_check_in_swap_loc:
                !protocol_config.disable_invariant_violation_check_in_swap_loc(),
            check_no_extraneous_bytes_during_deserialization:
                protocol_config.no_extraneous_module_bytes(),
        },
    )
    .map_err(|_| SuiError::ExecutionInvariantViolation)
}

pub fn new_native_extensions<'r>(
    child_resolver: &'r impl ChildObjectResolver,
    input_objects: BTreeMap<ObjectID, Owner>,
    is_metered: bool,
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
) -> NativeContextExtensions<'r> {
    let mut extensions = NativeContextExtensions::default();
    extensions.add(ObjectRuntime::new(
        Box::new(child_resolver),
        input_objects,
        is_metered,
        protocol_config,
        metrics,
    ));
    extensions.add(NativesCostTable::from_protocol_config(protocol_config));
    extensions
}

/// Given a list of `modules` and an `object_id`, mutate each module's self ID (which must be
/// 0x0) to be `object_id`.
pub fn substitute_package_id(
    modules: &mut [CompiledModule],
    object_id: ObjectID,
) -> Result<(), ExecutionError> {
    let new_address = AccountAddress::from(object_id);

    for module in modules.iter_mut() {
        let self_handle = module.self_handle().clone();
        let self_address_idx = self_handle.address;

        let addrs = &mut module.address_identifiers;
        let Some(address_mut) = addrs.get_mut(self_address_idx.0 as usize) else {
            let name = module.identifier_at(self_handle.name);
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::PublishErrorNonZeroAddress,
                format!("Publishing module {name} with invalid address index"),
            ));
        };

        if *address_mut != AccountAddress::ZERO {
            let name = module.identifier_at(self_handle.name);
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::PublishErrorNonZeroAddress,
                format!("Publishing module {name} with non-zero address is not allowed"),
            ));
        };

        *address_mut = new_address;
    }

    Ok(())
}

pub fn missing_unwrapped_msg(id: &ObjectID) -> String {
    format!(
        "Unable to unwrap object {}. Was unable to retrieve last known version in the parent sync",
        id
    )
}

/// Run the bytecode verifier with a meter limit
/// This function only fails if the verification does not complete within the limit
#[instrument(level = "trace", skip_all)]
pub fn run_metered_move_bytecode_verifier(
    module_bytes: &[Vec<u8>],
    protocol_config: &ProtocolConfig,
    metered_verifier_config: &VerifierConfig,
    meter: &mut impl Meter
) -> Result<(), SuiError> {
    let modules_stat = module_bytes
        .iter()
        .map(|b| {
            CompiledModule::deserialize_with_config(
                b,
                protocol_config.move_binary_format_version(),
                protocol_config.no_extraneous_module_bytes(),
            )
            .map_err(|e| e.finish(move_binary_format::errors::Location::Undefined))
        })
        .collect::<move_binary_format::errors::VMResult<Vec<CompiledModule>>>();
    let modules = if let Ok(m) = modules_stat {
        m
    } else {
        // Although we failed, we dont care since it failed withing the timeout
        return Ok(());
    };

    run_metered_move_bytecode_verifier_impl(&modules, protocol_config, metered_verifier_config, meter)
}

pub fn run_metered_move_bytecode_verifier_impl(
    modules: &[CompiledModule],
    protocol_config: &ProtocolConfig,
    verifier_config: &VerifierConfig,
    meter: &mut impl Meter
) -> Result<(), SuiError> {
    // run the Move verifier
    for module in modules.iter() {
        if let Err(e) = verify_module_with_config_metered(verifier_config, module, meter) {
            // Check that the status indicates mtering timeout
            // TODO: currently the Move verifier emits `CONSTRAINT_NOT_SATISFIED` for various failures including metering timeout
            // We need to change the VM error code to be more specific when timedout for metering
            if [
                StatusCode::CONSTRAINT_NOT_SATISFIED,
                StatusCode::TOO_MANY_BACK_EDGES,
            ]
            .contains(&e.major_status())
            {
                return Err(SuiError::ModuleVerificationFailure {
                    error: "Verification timedout".to_string(),
                });
            };
            sui_verify_module_metered(protocol_config, module, &BTreeMap::new(), meter)?
        }
    }
    Ok(())
}

}
