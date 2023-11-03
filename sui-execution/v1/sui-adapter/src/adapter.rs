// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
mod checked {
    use std::path::PathBuf;
    use std::{collections::BTreeMap, sync::Arc};

    use anyhow::Result;
    use move_binary_format::{access::ModuleAccess, file_format::CompiledModule};
    use move_bytecode_verifier::meter::Meter;
    use move_bytecode_verifier::verify_module_with_config_metered;
    use move_core_types::account_address::AccountAddress;
    use move_vm_config::runtime::VMProfilerConfig;
    use move_vm_config::{
        runtime::{VMConfig, VMRuntimeLimitsConfig, DEFAULT_PROFILE_OUTPUT_PATH},
        verifier::VerifierConfig,
    };
    use move_vm_runtime::{
        move_vm::MoveVM, native_extensions::NativeContextExtensions,
        native_functions::NativeFunctionTable,
    };
    use sui_move_natives::object_runtime;
    use sui_types::metrics::BytecodeVerifierMetrics;
    use sui_verifier::check_for_verifier_timeout;
    use tracing::instrument;

    use sui_move_natives::{object_runtime::ObjectRuntime, NativesCostTable};
    use sui_protocol_config::ProtocolConfig;
    use sui_types::{
        base_types::*,
        error::ExecutionError,
        error::{ExecutionErrorKind, SuiError},
        metrics::LimitsMetrics,
        storage::ChildObjectResolver,
    };
    use sui_verifier::verifier::sui_verify_module_metered_check_timeout_only;

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
                protocol_config.max_generic_instantiation_length() as usize,
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
        }
    }

    pub fn new_move_vm(
        natives: NativeFunctionTable,
        protocol_config: &ProtocolConfig,
        paranoid_type_checks: bool,
        enable_profiler: Option<PathBuf>,
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
                    max_value_nest_depth: protocol_config.max_move_value_depth_as_option(),
                },
                enable_invariant_violation_check_in_swap_loc: !protocol_config
                    .disable_invariant_violation_check_in_swap_loc(),
                check_no_extraneous_bytes_during_deserialization: protocol_config
                    .no_extraneous_module_bytes(),
                #[cfg(feature = "gas-profiler")]
                profiler_config: VMProfilerConfig {
                    enabled: enable_profiler.is_some(),
                    base_path: (*match enable_profiler {
                        Some(ref p) => p.clone().to_path_buf(),
                        None => std::path::PathBuf::from("."),
                    })
                    .to_owned(),
                    full_path: enable_profiler.filter(|p| {
                        !matches!(
                            p.partial_cmp(&*DEFAULT_PROFILE_OUTPUT_PATH),
                            Some(std::cmp::Ordering::Equal)
                        )
                    }),
                    track_bytecode_instructions: false,
                    use_long_function_name: false,
                },
                // Don't augment errors with execution state on-chain
                error_execution_state: false,
            },
        )
        .map_err(|_| SuiError::ExecutionInvariantViolation)
    }

    pub fn new_native_extensions<'r>(
        child_resolver: &'r dyn ChildObjectResolver,
        input_objects: BTreeMap<ObjectID, object_runtime::InputObject>,
        is_metered: bool,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        current_epoch_id: EpochId,
    ) -> NativeContextExtensions<'r> {
        let mut extensions = NativeContextExtensions::default();
        extensions.add(ObjectRuntime::new(
            child_resolver,
            input_objects,
            is_metered,
            protocol_config,
            metrics,
            current_epoch_id,
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
    ///
    /// This function only fails if the verification does not complete within the limit.  If the
    /// modules fail to verify but verification completes within the meter limit, the function
    /// succeeds.
    #[instrument(level = "trace", skip_all)]
    pub fn run_metered_move_bytecode_verifier(
        modules: &[CompiledModule],
        verifier_config: &VerifierConfig,
        meter: &mut impl Meter,
        metrics: &Arc<BytecodeVerifierMetrics>,
    ) -> Result<(), SuiError> {
        // run the Move verifier
        for module in modules.iter() {
            let per_module_meter_verifier_timer = metrics
                .verifier_runtime_per_module_success_latency
                .start_timer();

            if let Err(e) = verify_module_with_config_metered(verifier_config, module, meter) {
                // Check that the status indicates mtering timeout
                if check_for_verifier_timeout(&e.major_status()) {
                    // Discard success timer, but record timeout/failure timer
                    metrics
                        .verifier_runtime_per_module_timeout_latency
                        .observe(per_module_meter_verifier_timer.stop_and_discard());
                    metrics
                        .verifier_timeout_metrics
                        .with_label_values(&[
                            BytecodeVerifierMetrics::MOVE_VERIFIER_TAG,
                            BytecodeVerifierMetrics::TIMEOUT_TAG,
                        ])
                        .inc();
                    return Err(SuiError::ModuleVerificationFailure {
                        error: format!("Verification timedout: {}", e),
                    });
                };
            } else if let Err(err) =
                sui_verify_module_metered_check_timeout_only(module, &BTreeMap::new(), meter)
            {
                // We only checked that the failure was due to timeout
                // Discard success timer, but record timeout/failure timer
                metrics
                    .verifier_runtime_per_module_timeout_latency
                    .observe(per_module_meter_verifier_timer.stop_and_discard());
                metrics
                    .verifier_timeout_metrics
                    .with_label_values(&[
                        BytecodeVerifierMetrics::SUI_VERIFIER_TAG,
                        BytecodeVerifierMetrics::TIMEOUT_TAG,
                    ])
                    .inc();
                return Err(err.into());
            }
            // Save the success timer
            per_module_meter_verifier_timer.stop_and_record();
            metrics
                .verifier_timeout_metrics
                .with_label_values(&[
                    BytecodeVerifierMetrics::OVERALL_TAG,
                    BytecodeVerifierMetrics::SUCCESS_TAG,
                ])
                .inc();
        }
        Ok(())
    }
}
