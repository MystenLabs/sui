// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_trace_format::format::MoveTraceBuilder;
use move_vm_config::verifier::{MeterConfig, VerifierConfig};
use std::{cell::RefCell, collections::HashSet, path::PathBuf, rc::Rc, sync::Arc};
use sui_protocol_config::ProtocolConfig;
use sui_types::execution::ExecutionTiming;
use sui_types::transaction::GasData;
use sui_types::{
    base_types::{SuiAddress, TxContext},
    committee::EpochId,
    digests::TransactionDigest,
    effects::TransactionEffects,
    error::{ExecutionError, SuiError, SuiResult},
    execution::{ExecutionResult, TypeLayoutStore},
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    layout_resolver::LayoutResolver,
    metrics::{BytecodeVerifierMetrics, LimitsMetrics},
    transaction::{CheckedInputObjects, ProgrammableTransaction, TransactionKind},
};

use move_bytecode_verifier_meter::Meter;
use move_vm_runtime_latest::move_vm::MoveVM;
use sui_adapter_latest::adapter::{new_move_vm, run_metered_move_bytecode_verifier};
use sui_adapter_latest::execution_engine::{
    execute_genesis_state_update, execute_transaction_to_effects,
};
use sui_adapter_latest::type_layout_resolver::TypeLayoutResolver;
use sui_move_natives_latest::all_natives;
use sui_types::storage::BackingStore;
use sui_verifier_latest::meter::SuiVerifierMeter;

use crate::executor;
use crate::verifier;
use sui_adapter_latest::execution_mode;

pub(crate) struct Executor(Arc<MoveVM>);

pub(crate) struct Verifier<'m> {
    config: VerifierConfig,
    metrics: &'m Arc<BytecodeVerifierMetrics>,
}

impl Executor {
    pub(crate) fn new(
        protocol_config: &ProtocolConfig,
        silent: bool,
        enable_profiler: Option<PathBuf>,
    ) -> Result<Self, SuiError> {
        Ok(Executor(Arc::new(new_move_vm(
            all_natives(silent, protocol_config),
            protocol_config,
            enable_profiler,
        )?)))
    }
}

impl<'m> Verifier<'m> {
    pub(crate) fn new(config: VerifierConfig, metrics: &'m Arc<BytecodeVerifierMetrics>) -> Self {
        Verifier { config, metrics }
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
        gas: GasData,
        gas_status: SuiGasStatus,
        transaction_kind: TransactionKind,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) -> (
        InnerTemporaryStore,
        SuiGasStatus,
        TransactionEffects,
        Vec<ExecutionTiming>,
        Result<(), ExecutionError>,
    ) {
        execute_transaction_to_effects::<execution_mode::Normal>(
            store,
            input_objects,
            gas,
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
            trace_builder_opt,
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
        gas: GasData,
        gas_status: SuiGasStatus,
        transaction_kind: TransactionKind,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        skip_all_checks: bool,
    ) -> (
        InnerTemporaryStore,
        SuiGasStatus,
        TransactionEffects,
        Result<Vec<ExecutionResult>, ExecutionError>,
    ) {
        let (inner_temp_store, gas_status, effects, _timings, result) = if skip_all_checks {
            execute_transaction_to_effects::<execution_mode::DevInspect<true>>(
                store,
                input_objects,
                gas,
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
                &mut None,
            )
        } else {
            execute_transaction_to_effects::<execution_mode::DevInspect<false>>(
                store,
                input_objects,
                gas,
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
                &mut None,
            )
        };
        (inner_temp_store, gas_status, effects, result)
    }

    fn update_genesis_state(
        &self,
        store: &dyn BackingStore,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        epoch_id: EpochId,
        epoch_timestamp_ms: u64,
        transaction_digest: &TransactionDigest,
        input_objects: CheckedInputObjects,
        pt: ProgrammableTransaction,
    ) -> Result<InnerTemporaryStore, ExecutionError> {
        let tx_context = TxContext::new_from_components(
            &SuiAddress::default(),
            transaction_digest,
            &epoch_id,
            epoch_timestamp_ms,
            // genesis transaction: RGP: 1, budget: 1M, sponsor: None
            1,
            1_000_000,
            None,
        );
        let tx_context = Rc::new(RefCell::new(tx_context));
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

impl verifier::Verifier for Verifier<'_> {
    fn meter(&self, config: MeterConfig) -> Box<dyn Meter> {
        Box::new(SuiVerifierMeter::new(config))
    }

    fn meter_compiled_modules(
        &mut self,
        _protocol_config: &ProtocolConfig,
        modules: &[CompiledModule],
        meter: &mut dyn Meter,
    ) -> SuiResult<()> {
        run_metered_move_bytecode_verifier(modules, &self.config, meter, self.metrics)
    }
}
