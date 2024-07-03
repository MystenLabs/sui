// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;
#[sui_macros::with_checked_arithmetic]
mod checked {
    #[cfg(feature = "gas-profiler")]
    use move_vm_config::runtime::VMProfilerConfig;
    use std::path::PathBuf;
    use std::{collections::BTreeMap, sync::Arc};

    use anyhow::Result;
    use move_binary_format::file_format::CompiledModule;
    use move_bytecode_verifier::verify_module_with_config_metered;
    use move_bytecode_verifier_meter::{Meter, Scope};
    use move_core_types::account_address::AccountAddress;
    use move_vm_config::{
        runtime::{VMConfig, VMRuntimeLimitsConfig},
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
        execution_config_utils::to_binary_config,
        metrics::LimitsMetrics,
        storage::ChildObjectResolver,
    };
    use sui_verifier::verifier::sui_verify_module_metered_check_timeout_only;

    pub fn new_move_vm(
        natives: NativeFunctionTable,
        protocol_config: &ProtocolConfig,
        _enable_profiler: Option<PathBuf>,
    ) -> Result<MoveVM, SuiError> {
        #[cfg(not(feature = "gas-profiler"))]
        let vm_profiler_config = None;
        #[cfg(feature = "gas-profiler")]
        let vm_profiler_config = _enable_profiler.clone().map(|path| VMProfilerConfig {
            full_path: path,
            track_bytecode_instructions: false,
            use_long_function_name: false,
        });
        MoveVM::new_with_config(
            natives,
            VMConfig {
                verifier: protocol_config.verifier_config(/* for_signing */ false),
                max_binary_format_version: protocol_config.move_binary_format_version(),
                runtime_limits_config: VMRuntimeLimitsConfig {
                    vector_len_max: protocol_config.max_move_vector_len(),
                    max_value_nest_depth: protocol_config.max_move_value_depth_as_option(),
                    hardened_otw_check: protocol_config.hardened_otw_check(),
                },
                enable_invariant_violation_check_in_swap_loc: !protocol_config
                    .disable_invariant_violation_check_in_swap_loc(),
                check_no_extraneous_bytes_during_deserialization: protocol_config
                    .no_extraneous_module_bytes(),
                profiler_config: vm_profiler_config,
                // Don't augment errors with execution state on-chain
                error_execution_state: false,
                binary_config: to_binary_config(protocol_config),
            },
        )
        .map_err(|_| SuiError::ExecutionInvariantViolation)
    }

    pub fn new_native_extensions<'r>(
        child_resolver: &'r dyn ChildObjectResolver,
        input_objects: BTreeMap<ObjectID, object_runtime::InputObject>,
        is_metered: bool,
        protocol_config: &'r ProtocolConfig,
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
        meter: &mut (impl Meter + ?Sized),
        metrics: &Arc<BytecodeVerifierMetrics>,
    ) -> Result<(), SuiError> {
        // run the Move verifier
        for module in modules.iter() {
            let per_module_meter_verifier_timer = metrics
                .verifier_runtime_per_module_success_latency
                .start_timer();

            if let Err(e) = verify_module_timeout_only(module, verifier_config, meter) {
                // We only checked that the failure was due to timeout
                // Discard success timer, but record timeout/failure timer
                metrics
                    .verifier_runtime_per_module_timeout_latency
                    .observe(per_module_meter_verifier_timer.stop_and_discard());
                metrics
                    .verifier_timeout_metrics
                    .with_label_values(&[
                        BytecodeVerifierMetrics::OVERALL_TAG,
                        BytecodeVerifierMetrics::TIMEOUT_TAG,
                    ])
                    .inc();

                return Err(e);
            };

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

    /// Run both the Move verifier and the Sui verifier, checking just for timeouts. Returns Ok(())
    /// if the verifier completes within the module meter limit and the ticks are successfully
    /// transfered to the package limit (regardless of whether verification succeeds or not).
    fn verify_module_timeout_only(
        module: &CompiledModule,
        verifier_config: &VerifierConfig,
        meter: &mut (impl Meter + ?Sized),
    ) -> Result<(), SuiError> {
        meter.enter_scope(module.self_id().name().as_str(), Scope::Module);

        if let Err(e) = verify_module_with_config_metered(verifier_config, module, meter) {
            // Check that the status indicates metering timeout.
            if check_for_verifier_timeout(&e.major_status()) {
                return Err(SuiError::ModuleVerificationFailure {
                    error: format!("Verification timed out: {}", e),
                });
            }
        } else if let Err(err) = sui_verify_module_metered_check_timeout_only(
            module,
            &BTreeMap::new(),
            meter,
            verifier_config,
        ) {
            return Err(err.into());
        }

        if meter.transfer(Scope::Module, Scope::Package, 1.0).is_err() {
            return Err(SuiError::ModuleVerificationFailure {
                error: "Verification timed out".to_string(),
            });
        }

        Ok(())
    }
}
