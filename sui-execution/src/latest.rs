// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_trace_format::format::MoveTraceBuilder;
use move_vm_config::verifier::{MeterConfig, VerifierConfig};
use similar::TextDiff;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use sui_protocol_config::ProtocolConfig;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::execution::ExecutionTiming;
use sui_types::execution_params::ExecutionOrEarlyError;
use sui_types::execution_status::{ExecutionFailureStatus, ExecutionStatus};
use sui_types::gas::SuiGasStatusAPI;
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
use move_vm_runtime_latest::runtime::MoveRuntime;
use sui_adapter_latest::adapter::{new_move_runtime, run_metered_move_bytecode_verifier};
use sui_adapter_latest::execution_engine::execute_transaction_to_effects;
use sui_adapter_latest::type_layout_resolver::TypeLayoutResolver;
use sui_move_natives_latest::all_natives;
use sui_types::storage::BackingStore;
use sui_verifier_latest::meter::SuiVerifierMeter;

use crate::executor;
use crate::verifier;
use sui_adapter_latest::execution_mode;

pub(crate) struct Executor {
    bella_ciao_vm: Arc<MoveRuntime>,
    current_main_runtime: Arc<move_vm_runtime_replay_cut::move_vm::MoveVM>,
}

pub(crate) struct Verifier<'m> {
    config: VerifierConfig,
    metrics: &'m Arc<BytecodeVerifierMetrics>,
}

impl Executor {
    pub(crate) fn new(protocol_config: &ProtocolConfig, silent: bool) -> Result<Self, SuiError> {
        let bella_ciao_vm = Arc::new(new_move_runtime(
            all_natives(silent, protocol_config),
            protocol_config,
        )?);
        let current_main_runtime = Arc::new(sui_adapter_replay_cut::adapter::new_move_vm(
            sui_move_natives_replay_cut::all_natives(silent, protocol_config),
            protocol_config,
        )?);
        Ok(Executor {
            bella_ciao_vm,
            current_main_runtime,
        })
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
        execution_params: ExecutionOrEarlyError,
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
        use sui_adapter_replay_cut as replay_cut;
        // old vm + old adapter
        let current_main = replay_cut::execution_engine::execute_transaction_to_effects::<
            replay_cut::execution_mode::Normal,
        >(
            store,
            input_objects.clone(),
            gas.clone(),
            gas_status.clone(),
            transaction_kind.clone(),
            transaction_signer,
            transaction_digest,
            &self.current_main_runtime,
            epoch_id,
            epoch_timestamp_ms,
            protocol_config,
            metrics.clone(),
            enable_expensive_checks,
            execution_params.clone(),
            trace_builder_opt,
        );

        let mut ptb_v2_protocol_config = protocol_config.clone();
        ptb_v2_protocol_config.set_enable_ptb_execution_v2_for_testing(true);

        // old vm + new adapter
        let (m_inner_temporary_store, m_sui_gas_status, m_transaction_effects, _, _) =
            replay_cut::execution_engine::execute_transaction_to_effects::<
                replay_cut::execution_mode::Normal,
            >(
                store,
                input_objects.clone(),
                gas.clone(),
                gas_status.clone(),
                transaction_kind.clone(),
                transaction_signer,
                transaction_digest,
                &self.current_main_runtime,
                epoch_id,
                epoch_timestamp_ms,
                &ptb_v2_protocol_config,
                metrics.clone(),
                enable_expensive_checks,
                execution_params.clone(),
                trace_builder_opt,
            );

        let (b_inner_temporary_store, b_sui_gas_status, b_transaction_effects, _, _) =
            execute_transaction_to_effects::<execution_mode::Normal>(
                store,
                input_objects,
                gas,
                gas_status,
                transaction_kind,
                transaction_signer,
                transaction_digest,
                &self.bella_ciao_vm,
                epoch_id,
                epoch_timestamp_ms,
                &ptb_v2_protocol_config,
                metrics,
                enable_expensive_checks,
                execution_params,
                trace_builder_opt,
            );

        tracing::debug!("Executed transaction in three configurations");

        compare_effects(
            &(
                m_inner_temporary_store,
                m_sui_gas_status,
                m_transaction_effects,
            ),
            &(
                b_inner_temporary_store,
                b_sui_gas_status,
                b_transaction_effects,
            ),
        );

        current_main
    }

    fn dev_inspect_transaction(
        &self,
        store: &dyn BackingStore,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        execution_params: ExecutionOrEarlyError,
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
        use sui_adapter_replay_cut as replay_cut;
        let (inner_temp_store, gas_status, effects, _timings, result) = if skip_all_checks {
            replay_cut::execution_engine::execute_transaction_to_effects::<
                replay_cut::execution_mode::DevInspect<true>,
            >(
                store,
                input_objects,
                gas,
                gas_status,
                transaction_kind,
                transaction_signer,
                transaction_digest,
                &self.current_main_runtime,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                metrics,
                enable_expensive_checks,
                execution_params,
                &mut None,
            )
        } else {
            replay_cut::execution_engine::execute_transaction_to_effects::<
                replay_cut::execution_mode::DevInspect<false>,
            >(
                store,
                input_objects,
                gas,
                gas_status,
                transaction_kind,
                transaction_signer,
                transaction_digest,
                &self.current_main_runtime,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                metrics,
                enable_expensive_checks,
                execution_params,
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
        use sui_adapter_replay_cut as replay_cut;
        let tx_context = TxContext::new_from_components(
            &SuiAddress::default(),
            transaction_digest,
            &epoch_id,
            epoch_timestamp_ms,
            // genesis transaction: RGP: 1, budget: 1M, sponsor: None
            1,
            1,
            1_000_000,
            None,
            protocol_config,
        );
        let tx_context = Rc::new(RefCell::new(tx_context));
        replay_cut::execution_engine::execute_genesis_state_update(
            store,
            protocol_config,
            metrics,
            &self.current_main_runtime,
            tx_context,
            input_objects,
            pt,
        )
    }

    fn type_layout_resolver<'r, 'vm: 'r, 'store: 'r>(
        &'vm self,
        store: Box<dyn TypeLayoutStore + 'store>,
    ) -> Box<dyn LayoutResolver + 'r> {
        Box::new(TypeLayoutResolver::new(&self.bella_ciao_vm, store))
    }
}

impl verifier::Verifier for Verifier<'_> {
    fn meter(&self, config: MeterConfig) -> Box<dyn Meter> {
        Box::new(SuiVerifierMeter::new(config))
    }

    fn override_deprecate_global_storage_ops_during_deserialization(&self) -> Option<bool> {
        Some(true)
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

#[cfg(msim)]
pub fn init_vm_for_msim() {
    use move_vm_runtime_latest::cache::identifier_interner;
    identifier_interner::init_interner();
}

fn compare_effects(
    normal_effects: &(InnerTemporaryStore, SuiGasStatus, TransactionEffects),
    new_effects: &(InnerTemporaryStore, SuiGasStatus, TransactionEffects),
) {
    let ok = match (normal_effects.2.status(), new_effects.2.status()) {
        // success => success
        (ExecutionStatus::Success, ExecutionStatus::Success) => true,
        // Invariant violation in new
        (
            _,
            ExecutionStatus::Failure {
                error: ExecutionFailureStatus::InvariantViolation,
                ..
            },
        ) => false,
        // failure => failure
        (
            ExecutionStatus::Failure { error: _, .. },
            ExecutionStatus::Failure {
                error: _other_error,
                ..
            },
        ) => true,
        // Ran out of gas in the new one
        (
            _,
            ExecutionStatus::Failure {
                error: ExecutionFailureStatus::InsufficientGas,
                ..
            },
        ) => true,
        _ => false,
    };

    // If you want to log gas usage differences, uncomment this line
    // and add the gas row writing function from the other replay branch here: https://github.com/MystenLabs/sui/pull/24042/files#diff-2e9d962a08321605940b5a657135052fbcef87b5e360662bb527c96d9a615542
    // write_gas_row
    //     normal_effects.2.transaction_digest().to_string(),
    //     &new_effects.1.gas_usage_report(),
    //     &normal_effects.1.gas_usage_report(),
    // );

    // Probably want to only log this when they differ, but set to always log for now just for you
    // to play with.
    // if !ok {
    if true {
        tracing::warn!(
            "{} TransactionEffects differ",
            normal_effects.2.transaction_digest()
        );
        let t1 = format!("{:#?}", normal_effects.2);
        let t2 = format!("{:#?}", new_effects.2);
        let s = TextDiff::from_lines(&t1, &t2).unified_diff().to_string();
        let data = format!(
            "---\nDIGEST: {}\n>>\n{}\n<<<\n{:#?}\n{:#?}\n",
            normal_effects.2.transaction_digest(),
            s,
            normal_effects.1.gas_usage_report(),
            new_effects.1.gas_usage_report(),
        );
        let output_file = format!("outputs/{}", normal_effects.2.transaction_digest());

        std::fs::write(&output_file, &data).expect("Failed to write output file");
    } else {
        tracing::info!(
            "{} TransactionEffects are the same for both executions",
            normal_effects.2.transaction_digest()
        );
    }
}
