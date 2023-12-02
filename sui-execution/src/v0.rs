// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc};

use move_binary_format::CompiledModule;
use move_vm_config::verifier::VerifierConfig;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::{
    base_types::{ObjectRef, SuiAddress, TxContext},
    committee::EpochId,
    digests::TransactionDigest,
    effects::TransactionEffects,
    error::{ExecutionError, SuiError, SuiResult},
    execution::TypeLayoutStore,
    execution_mode::{self, ExecutionResult},
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    metrics::{BytecodeVerifierMetrics, LimitsMetrics},
    transaction::{CheckedInputObjects, ProgrammableTransaction, TransactionKind},
    type_resolver::LayoutResolver,
};

use move_bytecode_verifier_v0::{meter::Scope, verify_module_unmetered};
use move_vm_runtime_v0::move_vm::MoveVM;
use sui_adapter_v0::adapter::{
    default_verifier_config, new_move_vm, run_metered_move_bytecode_verifier,
};
use sui_adapter_v0::execution_engine::{
    execute_genesis_state_update, execute_transaction_to_effects,
};
use sui_adapter_v0::type_layout_resolver::TypeLayoutResolver;
use sui_move_natives_v0::all_natives;
use sui_types::{move_package::FnInfoMap, storage::BackingStore};
use sui_verifier_v0::{meter::SuiVerifierMeter, verifier::sui_verify_module_unmetered};

use crate::executor;
use crate::verifier;
use crate::verifier::{VerifierMeteredValues, VerifierOverrides};

pub(crate) struct Executor(Arc<MoveVM>);

pub(crate) struct Verifier {
    config: VerifierConfig,
    meter: SuiVerifierMeter,
}

impl Executor {
    pub(crate) fn new(
        protocol_config: &ProtocolConfig,
        paranoid_type_checks: bool,
        silent: bool,
    ) -> Result<Self, SuiError> {
        Ok(Executor(Arc::new(new_move_vm(
            all_natives(silent),
            protocol_config,
            paranoid_type_checks,
        )?)))
    }
}

impl Verifier {
    pub(crate) fn new(config: VerifierConfig) -> Self {
        let meter = SuiVerifierMeter::new(&config);
        Verifier { config, meter }
    }

    pub(crate) fn verifier_config(
        protocol_config: &ProtocolConfig,
        is_metered: bool,
    ) -> VerifierConfig {
        default_verifier_config(protocol_config, is_metered)
    }
}

impl executor::Executor for Executor {
    fn execute_transaction_to_effects(
        &self,
        store: &dyn BackingStore,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        certificate_deny_set: &HashSet<TransactionDigest>,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        input_objects: CheckedInputObjects,
        gas_coins: Vec<ObjectRef>,
        gas_status: SuiGasStatus,
        transaction_kind: TransactionKind,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
    ) -> (
        InnerTemporaryStore,
        TransactionEffects,
        Result<(), ExecutionError>,
    ) {
        execute_transaction_to_effects::<execution_mode::Normal>(
            store,
            input_objects,
            gas_coins,
            gas_status,
            transaction_kind,
            transaction_signer,
            transaction_digest,
            &self.0,
            epoch_id,
            epoch_timestamp_ms,
            protocol_config,
            metrics,
            enable_expensive_checks,
            certificate_deny_set,
        )
    }

    fn dev_inspect_transaction(
        &self,
        store: &dyn BackingStore,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        certificate_deny_set: &HashSet<TransactionDigest>,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        input_objects: CheckedInputObjects,
        gas_coins: Vec<ObjectRef>,
        gas_status: SuiGasStatus,
        transaction_kind: TransactionKind,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
    ) -> (
        InnerTemporaryStore,
        TransactionEffects,
        Result<Vec<ExecutionResult>, ExecutionError>,
    ) {
        execute_transaction_to_effects::<execution_mode::DevInspect>(
            store,
            input_objects,
            gas_coins,
            gas_status,
            transaction_kind,
            transaction_signer,
            transaction_digest,
            &self.0,
            epoch_id,
            epoch_timestamp_ms,
            protocol_config,
            metrics,
            enable_expensive_checks,
            certificate_deny_set,
        )
    }

    fn update_genesis_state(
        &self,
        store: &dyn BackingStore,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        tx_context: &mut TxContext,
        input_objects: CheckedInputObjects,
        pt: ProgrammableTransaction,
    ) -> Result<InnerTemporaryStore, ExecutionError> {
        execute_genesis_state_update(
            store,
            protocol_config,
            metrics,
            &self.0,
            tx_context,
            input_objects,
            pt,
        )
    }

    fn type_layout_resolver<'r, 'vm: 'r, 'store: 'r>(
        &'vm self,
        store: Box<dyn TypeLayoutStore + 'store>,
    ) -> Box<dyn LayoutResolver + 'r> {
        Box::new(TypeLayoutResolver::new(&self.0, store))
    }
}

impl verifier::Verifier for Verifier {
    fn meter_compiled_modules(
        &mut self,
        protocol_config: &ProtocolConfig,
        modules: &[CompiledModule],
        metrics: &Arc<BytecodeVerifierMetrics>,
    ) -> SuiResult<()> {
        run_metered_move_bytecode_verifier(
            modules,
            protocol_config,
            &self.config,
            &mut self.meter,
            metrics,
        )
    }

    fn meter_compiled_modules_with_overrides(
        &mut self,
        modules: &[CompiledModule],
        protocol_config: &ProtocolConfig,
        config_overrides: &VerifierOverrides,
        metrics: &Arc<BytecodeVerifierMetrics>,
    ) -> SuiResult<VerifierMeteredValues> {
        let mut config = self.config.clone();
        let max_per_fun_meter_current = config.max_per_fun_meter_units;
        let max_per_mod_meter_current = config.max_per_mod_meter_units;
        config.max_per_fun_meter_units = config_overrides.max_per_fun_meter_units;
        config.max_per_mod_meter_units = config_overrides.max_per_mod_meter_units;
        run_metered_move_bytecode_verifier(
            modules,
            protocol_config,
            &config,
            &mut self.meter,
            metrics,
        )?;
        let fun_meter_units_result = self.meter.get_usage(Scope::Function);
        let mod_meter_units_result = self.meter.get_usage(Scope::Function);
        Ok(VerifierMeteredValues::new(
            max_per_fun_meter_current,
            max_per_mod_meter_current,
            fun_meter_units_result,
            mod_meter_units_result,
        ))
    }

    fn verify_module_unmetered(
        &self,
        module: &CompiledModule,
        fn_info_map: &FnInfoMap,
    ) -> SuiResult<()> {
        verify_module_unmetered(module).map_err(|err| SuiError::ModuleVerificationFailure {
            error: err.to_string(),
        })?;
        sui_verify_module_unmetered(
            // at ProtocolVersion(18) execution moved to v1, so there is no point in
            // getting anything above for v0
            &ProtocolConfig::get_for_version(ProtocolVersion::new(17), Chain::Mainnet),
            module,
            fn_info_map,
        )
        .map_err(|err| SuiError::ModuleVerificationFailure {
            error: err.to_string(),
        })
    }
}
