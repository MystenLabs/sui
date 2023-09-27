// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::{collections::HashSet, sync::Arc};

use move_binary_format::CompiledModule;
use move_vm_config::verifier::VerifierConfig;
use sui_move_natives_v0::{all_natives as all_natives_impl, NativesCostTable};
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
    move_package::FnInfoMap,
    storage::{BackingStore, ChildObjectResolver},
    transaction::{InputObjects, ProgrammableTransaction, TransactionKind},
    type_resolver::LayoutResolver,
};

use move_bytecode_verifier_v0::meter::Scope;
use move_vm_runtime_v0::move_vm::MoveVM;

use move_vm_types::natives::{
    native_extensions::NativeContextExtensions, native_functions::NativeFunctionTable,
};

use sui_adapter_v0::adapter::{
    default_verifier_config, new_move_vm, run_metered_move_bytecode_verifier,
};
use sui_adapter_v0::execution_engine::{
    execute_genesis_state_update, execute_transaction_to_effects,
};
use sui_adapter_v0::type_layout_resolver::TypeLayoutResolver;
use sui_move_natives_v0::object_runtime::ObjectRuntime;
use sui_verifier_v0::meter::SuiVerifierMeter;

use crate::executor;
use crate::verifier;
use crate::verifier::{VerifierMeteredValues, VerifierOverrides};

pub(crate) struct Executor(Arc<MoveVM>);

pub(crate) struct Verifier<'m> {
    config: VerifierConfig,
    metrics: &'m Arc<BytecodeVerifierMetrics>,
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

impl<'m> Verifier<'m> {
    pub(crate) fn new(
        protocol_config: &ProtocolConfig,
        is_metered: bool,
        metrics: &'m Arc<BytecodeVerifierMetrics>,
    ) -> Self {
        let config = default_verifier_config(protocol_config, is_metered);
        let meter = SuiVerifierMeter::new(&config);
        Verifier {
            config,
            metrics,
            meter,
        }
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
        input_objects: InputObjects,
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
        input_objects: InputObjects,
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
        input_objects: InputObjects,
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

    fn add_extensions<'a>(
        &self,
        ext: &mut NativeContextExtensions<'a>,
        object_resolver: &'a dyn ChildObjectResolver,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
    ) -> Result<(), ExecutionError> {
        ext.add(ObjectRuntime::new(
            object_resolver,
            BTreeMap::new(),
            false,
            protocol_config,
            metrics,
        ));
        ext.add(NativesCostTable::from_protocol_config(protocol_config));
        Ok(())
    }
}

impl<'m> verifier::Verifier for Verifier<'m> {
    fn meter_compiled_modules(
        &mut self,
        protocol_config: &ProtocolConfig,
        modules: &[CompiledModule],
    ) -> SuiResult<()> {
        run_metered_move_bytecode_verifier(
            modules,
            protocol_config,
            &self.config,
            &mut self.meter,
            self.metrics,
        )
    }

    fn meter_compiled_modules_with_overrides(
        &mut self,
        modules: &[CompiledModule],
        protocol_config: &ProtocolConfig,
        config_overrides: &VerifierOverrides,
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
            self.metrics,
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
}

pub(crate) struct UnmeteredVerifier {
    #[allow(unused)]
    config: VerifierConfig,
}

impl UnmeteredVerifier {
    pub(crate) fn new() -> Self {
        let config = VerifierConfig {
            max_back_edges_per_function: None,
            max_back_edges_per_module: None,
            max_basic_blocks_in_script: None,
            max_per_fun_meter_units: None,
            max_per_mod_meter_units: None,
            ..VerifierConfig::default()
        };
        Self { config }
    }
}

impl verifier::UnmeteredVerifier for UnmeteredVerifier {
    fn verify_module(&self, module: &CompiledModule, fn_info_map: &FnInfoMap) -> SuiResult<()> {
        move_bytecode_verifier_v0::verify_module_unmetered(module).map_err(|err| {
            SuiError::ModuleVerificationFailure {
                error: err.to_string(),
            }
        })?;
        sui_verifier_v0::verifier::sui_verify_module_unmetered(
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

pub fn all_natives(silent: bool) -> NativeFunctionTable {
    all_natives_impl(silent)
}
