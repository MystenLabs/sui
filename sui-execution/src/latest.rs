// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_trace_format::format::MoveTraceBuilder;
use move_vm_config::verifier::{MeterConfig, VerifierConfig};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use sui_protocol_config::ProtocolConfig;
use sui_types::execution::ExecutionTiming;
use sui_types::execution_params::ExecutionOrEarlyError;
use sui_types::transaction::GasData;
use sui_types::{
    base_types::{SuiAddress, TxContext},
    committee::EpochId,
    digests::TransactionDigest,
    effects::TransactionEffects,
    error::{ExecutionError, ExecutionErrorTrait, SuiError, SuiResult},
    execution::{ExecutionResult, TypeLayoutStore},
    execution_status::ExecutionFailure,
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    layout_resolver::LayoutResolver,
    metrics::{BytecodeVerifierMetrics, ExecutionMetrics},
    transaction::{CheckedInputObjects, ProgrammableTransaction, TransactionKind},
};

use move_bytecode_verifier_meter::Meter;
use move_vm_runtime_latest::runtime::MoveRuntime;
use mysten_common::debug_fatal;
use sui_adapter_latest::adapter::{new_move_runtime, run_metered_move_bytecode_verifier};
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

pub(crate) struct Executor(
    Arc<MoveRuntime>,
    Arc<move_vm_runtime_replay_cut::runtime::MoveRuntime>,
);

pub(crate) struct Verifier<'m> {
    config: VerifierConfig,
    metrics: &'m Arc<BytecodeVerifierMetrics>,
}

impl Executor {
    pub(crate) fn new(protocol_config: &ProtocolConfig, silent: bool) -> Result<Self, SuiError> {
        let tip_runtime = Arc::new(new_move_runtime(
            all_natives(silent, protocol_config),
            protocol_config,
        )?);
        let base_runtime = Arc::new(sui_adapter_replay_cut::adapter::new_move_runtime(
            sui_move_natives_replay_cut::all_natives(silent, protocol_config),
            protocol_config,
        )?);
        tracing::warn!(
            order = "base_tip_tip_base",
            timings = "abba_outer_and_execution_loop",
            "dual-replay: tx-backtest executor enabled"
        );
        Ok(Executor(tip_runtime, base_runtime))
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
        metrics: Arc<ExecutionMetrics>,
        enable_expensive_checks: bool,
        execution_params: ExecutionOrEarlyError,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        input_objects: CheckedInputObjects,
        gas: GasData,
        gas_status: SuiGasStatus,
        transaction_kind: TransactionKind,
        rewritten_inputs: Option<Vec<bool>>,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) -> (
        InnerTemporaryStore,
        SuiGasStatus,
        TransactionEffects,
        Vec<ExecutionTiming>,
        Result<(), ExecutionFailure>,
    ) {
        // DUAL_REPLAY_INJECTED
        // Run A/B/B/A so first-run and second-run order effects can be separated from the
        // base-vs-tip comparison. A=base/replay_cut, B=tip/latest.
        let tip1_input_objects = input_objects.clone();
        let tip2_input_objects = input_objects.clone();
        let base2_input_objects = input_objects.clone();
        let tip1_gas = gas.clone();
        let tip2_gas = gas.clone();
        let base2_gas = gas.clone();
        let tip1_gas_status = gas_status.clone();
        let tip2_gas_status = gas_status.clone();
        let base2_gas_status = gas_status.clone();
        let tip1_transaction_kind = transaction_kind.clone();
        let tip2_transaction_kind = transaction_kind.clone();
        let base2_transaction_kind = transaction_kind.clone();
        let tip1_rewritten_inputs = rewritten_inputs.clone();
        let tip2_rewritten_inputs = rewritten_inputs.clone();
        let base2_rewritten_inputs = rewritten_inputs.clone();
        let tip1_metrics = metrics.clone();
        let tip2_metrics = metrics.clone();
        let base2_metrics = metrics.clone();
        let tip1_execution_params = execution_params.clone();
        let tip2_execution_params = execution_params.clone();
        let base2_execution_params = execution_params.clone();

        let base1_start = std::time::Instant::now();
        let base1 = {
            use sui_adapter_replay_cut as base_adapter;
            base_adapter::execution_engine::execute_transaction_to_effects::<
                base_adapter::execution_mode::Normal,
            >(
                store,
                input_objects,
                gas,
                gas_status,
                transaction_kind,
                rewritten_inputs,
                transaction_signer,
                transaction_digest,
                &self.1,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                metrics,
                enable_expensive_checks,
                execution_params,
                trace_builder_opt,
            )
        };
        let base1_ns = base1_start.elapsed().as_nanos() as u64;
        let base1_loop_ns =
            sui_adapter_replay_cut::execution_engine::dual_replay_take_last_execution_loop_ns();

        let tip1_start = std::time::Instant::now();
        let (tip1_store, tip1_gas_status, tip1_effects, _tip1_timings, _tip1_result) =
            execute_transaction_to_effects::<execution_mode::Normal>(
                store,
                tip1_input_objects,
                tip1_gas,
                tip1_gas_status,
                tip1_transaction_kind,
                tip1_rewritten_inputs,
                transaction_signer,
                transaction_digest,
                &self.0,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                tip1_metrics,
                enable_expensive_checks,
                tip1_execution_params,
                &mut None,
            );
        let tip1_ns = tip1_start.elapsed().as_nanos() as u64;
        let tip1_loop_ns =
            sui_adapter_latest::execution_engine::dual_replay_take_last_execution_loop_ns();

        let tip2_start = std::time::Instant::now();
        let (tip2_store, tip2_gas_status, tip2_effects, _tip2_timings, _tip2_result) =
            execute_transaction_to_effects::<execution_mode::Normal>(
                store,
                tip2_input_objects,
                tip2_gas,
                tip2_gas_status,
                tip2_transaction_kind,
                tip2_rewritten_inputs,
                transaction_signer,
                transaction_digest,
                &self.0,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                tip2_metrics,
                enable_expensive_checks,
                tip2_execution_params,
                &mut None,
            );
        let tip2_ns = tip2_start.elapsed().as_nanos() as u64;
        let tip2_loop_ns =
            sui_adapter_latest::execution_engine::dual_replay_take_last_execution_loop_ns();

        let base2_start = std::time::Instant::now();
        let base2 = {
            use sui_adapter_replay_cut as base_adapter;
            base_adapter::execution_engine::execute_transaction_to_effects::<
                base_adapter::execution_mode::Normal,
            >(
                store,
                base2_input_objects,
                base2_gas,
                base2_gas_status,
                base2_transaction_kind,
                base2_rewritten_inputs,
                transaction_signer,
                transaction_digest,
                &self.1,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                base2_metrics,
                enable_expensive_checks,
                base2_execution_params,
                &mut None,
            )
        };
        let base2_ns = base2_start.elapsed().as_nanos() as u64;
        let base2_loop_ns =
            sui_adapter_replay_cut::execution_engine::dual_replay_take_last_execution_loop_ns();

        self::latest_dual_replay::compare_dual_replay_abba(
            (&base1.0, &base1.1, &base1.2),
            (&tip1_store, &tip1_gas_status, &tip1_effects),
            (&tip2_store, &tip2_gas_status, &tip2_effects),
            (&base2.0, &base2.1, &base2.2),
            transaction_digest,
            base1_ns,
            tip1_ns,
            tip2_ns,
            base2_ns,
            base1_loop_ns,
            tip1_loop_ns,
            tip2_loop_ns,
            base2_loop_ns,
        );
        if let Err(error) = &base1.4 {
            log_execution_error(transaction_digest, error);
        }
        base1
    }

    fn execute_transaction_to_effects_and_execution_error(
        &self,
        store: &dyn BackingStore,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        enable_expensive_checks: bool,
        execution_params: ExecutionOrEarlyError,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        input_objects: CheckedInputObjects,
        gas: GasData,
        gas_status: SuiGasStatus,
        transaction_kind: TransactionKind,
        rewritten_inputs: Option<Vec<bool>>,
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
        // DUAL_REPLAY_INJECTED
        // Run A/B/B/A so first-run and second-run order effects can be separated from the
        // base-vs-tip comparison. A=base/replay_cut, B=tip/latest.
        let tip1_input_objects = input_objects.clone();
        let tip2_input_objects = input_objects.clone();
        let base2_input_objects = input_objects.clone();
        let tip1_gas = gas.clone();
        let tip2_gas = gas.clone();
        let base2_gas = gas.clone();
        let tip1_gas_status = gas_status.clone();
        let tip2_gas_status = gas_status.clone();
        let base2_gas_status = gas_status.clone();
        let tip1_transaction_kind = transaction_kind.clone();
        let tip2_transaction_kind = transaction_kind.clone();
        let base2_transaction_kind = transaction_kind.clone();
        let tip1_rewritten_inputs = rewritten_inputs.clone();
        let tip2_rewritten_inputs = rewritten_inputs.clone();
        let base2_rewritten_inputs = rewritten_inputs.clone();
        let tip1_metrics = metrics.clone();
        let tip2_metrics = metrics.clone();
        let base2_metrics = metrics.clone();
        let tip1_execution_params = execution_params.clone();
        let tip2_execution_params = execution_params.clone();
        let base2_execution_params = execution_params.clone();

        let base1_start = std::time::Instant::now();
        let base1 = {
            use sui_adapter_replay_cut as base_adapter;
            base_adapter::execution_engine::execute_transaction_to_effects::<
                base_adapter::execution_mode::Normal<ExecutionError>,
            >(
                store,
                input_objects,
                gas,
                gas_status,
                transaction_kind,
                rewritten_inputs,
                transaction_signer,
                transaction_digest,
                &self.1,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                metrics,
                enable_expensive_checks,
                execution_params,
                trace_builder_opt,
            )
        };
        let base1_ns = base1_start.elapsed().as_nanos() as u64;
        let base1_loop_ns =
            sui_adapter_replay_cut::execution_engine::dual_replay_take_last_execution_loop_ns();

        let tip1_start = std::time::Instant::now();
        let (tip1_store, tip1_gas_status, tip1_effects, _tip1_timings, _tip1_result) =
            execute_transaction_to_effects::<execution_mode::Normal<ExecutionError>>(
                store,
                tip1_input_objects,
                tip1_gas,
                tip1_gas_status,
                tip1_transaction_kind,
                tip1_rewritten_inputs,
                transaction_signer,
                transaction_digest,
                &self.0,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                tip1_metrics,
                enable_expensive_checks,
                tip1_execution_params,
                &mut None,
            );
        let tip1_ns = tip1_start.elapsed().as_nanos() as u64;
        let tip1_loop_ns =
            sui_adapter_latest::execution_engine::dual_replay_take_last_execution_loop_ns();

        let tip2_start = std::time::Instant::now();
        let (tip2_store, tip2_gas_status, tip2_effects, _tip2_timings, _tip2_result) =
            execute_transaction_to_effects::<execution_mode::Normal<ExecutionError>>(
                store,
                tip2_input_objects,
                tip2_gas,
                tip2_gas_status,
                tip2_transaction_kind,
                tip2_rewritten_inputs,
                transaction_signer,
                transaction_digest,
                &self.0,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                tip2_metrics,
                enable_expensive_checks,
                tip2_execution_params,
                &mut None,
            );
        let tip2_ns = tip2_start.elapsed().as_nanos() as u64;
        let tip2_loop_ns =
            sui_adapter_latest::execution_engine::dual_replay_take_last_execution_loop_ns();

        let base2_start = std::time::Instant::now();
        let base2 = {
            use sui_adapter_replay_cut as base_adapter;
            base_adapter::execution_engine::execute_transaction_to_effects::<
                base_adapter::execution_mode::Normal<ExecutionError>,
            >(
                store,
                base2_input_objects,
                base2_gas,
                base2_gas_status,
                base2_transaction_kind,
                base2_rewritten_inputs,
                transaction_signer,
                transaction_digest,
                &self.1,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                base2_metrics,
                enable_expensive_checks,
                base2_execution_params,
                &mut None,
            )
        };
        let base2_ns = base2_start.elapsed().as_nanos() as u64;
        let base2_loop_ns =
            sui_adapter_replay_cut::execution_engine::dual_replay_take_last_execution_loop_ns();

        self::latest_dual_replay::compare_dual_replay_abba(
            (&base1.0, &base1.1, &base1.2),
            (&tip1_store, &tip1_gas_status, &tip1_effects),
            (&tip2_store, &tip2_gas_status, &tip2_effects),
            (&base2.0, &base2.1, &base2.2),
            transaction_digest,
            base1_ns,
            tip1_ns,
            tip2_ns,
            base2_ns,
            base1_loop_ns,
            tip1_loop_ns,
            tip2_loop_ns,
            base2_loop_ns,
        );
        if let Err(error) = &base1.4 {
            log_execution_error(transaction_digest, error);
        }
        base1
    }

    fn dev_inspect_transaction(
        &self,
        store: &dyn BackingStore,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        enable_expensive_checks: bool,
        execution_params: ExecutionOrEarlyError,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        input_objects: CheckedInputObjects,
        gas: GasData,
        gas_status: SuiGasStatus,
        transaction_kind: TransactionKind,
        rewritten_inputs: Option<Vec<bool>>,
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
                rewritten_inputs,
                transaction_signer,
                transaction_digest,
                &self.0,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                metrics,
                enable_expensive_checks,
                execution_params,
                &mut None,
            )
        } else {
            execute_transaction_to_effects::<execution_mode::DevInspect<false>>(
                store,
                input_objects,
                gas,
                gas_status,
                transaction_kind,
                rewritten_inputs,
                transaction_signer,
                transaction_digest,
                &self.0,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                metrics,
                enable_expensive_checks,
                execution_params,
                &mut None,
            )
        };
        if let Err(error) = &result {
            log_execution_error(transaction_digest, error);
        }
        (inner_temp_store, gas_status, effects, result)
    }

    fn update_genesis_state(
        &self,
        store: &dyn BackingStore,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
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
            1,
            1_000_000,
            None,
            protocol_config,
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
        protocol_config: &'vm ProtocolConfig,
        store: Box<dyn TypeLayoutStore + 'store>,
    ) -> Box<dyn LayoutResolver + 'r> {
        Box::new(TypeLayoutResolver::new(&self.0, protocol_config, store))
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

fn log_execution_error<E>(transaction_digest: TransactionDigest, error: &E)
where
    E: ExecutionErrorTrait + std::error::Error,
{
    use sui_types::execution_status::ExecutionErrorKind as K;

    match error.kind() {
        K::InvariantViolation | K::VMInvariantViolation => {
            debug_fatal!(
                "INVARIANT VIOLATION! Txn Digest: {}, Source: {:?}",
                transaction_digest,
                std::error::Error::source(error)
            );
        }
        K::SuiMoveVerificationError | K::VMVerificationOrDeserializationError => {
            tracing::debug!(
                kind = ?error.kind(),
                tx_digest = ?transaction_digest,
                "Verification Error. Source: {:?}",
                std::error::Error::source(error),
            );
        }
        K::PublishUpgradeMissingDependency | K::PublishUpgradeDependencyDowngrade => {
            tracing::debug!(
                kind = ?error.kind(),
                tx_digest = ?transaction_digest,
                "Publish/Upgrade Error. Source: {:?}",
                std::error::Error::source(error),
            );
        }
        _ => (),
    }
}

// // DUAL_REPLAY_INJECTED
mod latest_dual_replay {
    use std::fs::{File, OpenOptions};
    use std::io::{BufWriter, Write};
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};
    use sui_types::digests::TransactionDigest;
    use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
    use sui_types::execution_status::ExecutionStatus;
    use sui_types::gas::{SuiGasStatus, SuiGasStatusAPI};
    use sui_types::inner_temporary_store::InnerTemporaryStore;

    const OUTPUT_DIR: &str = "/opt/sui/replay-output/ccd970cfed2bb648c4c14941642c927bb84d6cb7";
    const GAS_TOLERANCE_PCT: f64 = 0.0_f64;
    const TIMINGS_FILE: &str =
        "/opt/sui/replay-output/ccd970cfed2bb648c4c14941642c927bb84d6cb7/timings.csv";
    const TIMINGS_FLUSH_EVERY: usize = 500;

    type View<'a> = (
        &'a InnerTemporaryStore,
        &'a SuiGasStatus,
        &'a TransactionEffects,
    );

    pub(super) fn compare_dual_replay_abba(
        base1: View<'_>,
        tip1: View<'_>,
        tip2: View<'_>,
        base2: View<'_>,
        digest: TransactionDigest,
        base1_ns: u64,
        tip1_ns: u64,
        tip2_ns: u64,
        base2_ns: u64,
        base1_loop_ns: u64,
        tip1_loop_ns: u64,
        tip2_loop_ns: u64,
        base2_loop_ns: u64,
    ) {
        let (_, base1_gas, base1_effects) = base1;
        let (_, tip1_gas, tip1_effects) = tip1;
        let (_, tip2_gas, tip2_effects) = tip2;
        let (_, base2_gas, base2_effects) = base2;
        let base1_gas_used = base1_gas.gas_used();
        let tip1_gas_used = tip1_gas.gas_used();
        let tip2_gas_used = tip2_gas.gas_used();
        let base2_gas_used = base2_gas.gas_used();
        let status_match_1 = same_status_shape(base1_effects.status(), tip1_effects.status());
        let status_match_2 = same_status_shape(base2_effects.status(), tip2_effects.status());
        record_timing_abba(
            digest,
            base1_ns,
            tip1_ns,
            tip2_ns,
            base2_ns,
            base1_loop_ns,
            tip1_loop_ns,
            tip2_loop_ns,
            base2_loop_ns,
            base1_gas_used,
            tip1_gas_used,
            tip2_gas_used,
            base2_gas_used,
            status_match_1,
            status_match_2,
        );
        if should_log_completed_paths() {
            let outer_tip_avg_ns = average_ns(tip1_ns, tip2_ns);
            let outer_base_avg_ns = average_ns(base1_ns, base2_ns);
            let loop_tip_avg_ns = average_ns(tip1_loop_ns, tip2_loop_ns);
            let loop_base_avg_ns = average_ns(base1_loop_ns, base2_loop_ns);
            tracing::info!(
                %digest,
                base1_ns,
                tip1_ns,
                tip2_ns,
                base2_ns,
                base1_loop_ns,
                tip1_loop_ns,
                tip2_loop_ns,
                base2_loop_ns,
                outer_tip_avg_ns,
                outer_base_avg_ns,
                loop_tip_avg_ns,
                loop_base_avg_ns,
                base1_gas_used,
                tip1_gas_used,
                tip2_gas_used,
                base2_gas_used,
                status_match_1,
                status_match_2,
                "dual-replay: A/B/B/A execution paths completed"
            );
        }
        let first_differs = effects_differ(base1_effects, tip1_effects, base1_gas, tip1_gas);
        let second_differs = effects_differ(base2_effects, tip2_effects, base2_gas, tip2_gas);
        if first_differs || second_differs {
            report_diff(base1_effects, tip1_effects, digest);
        }
    }

    fn average_ns(left: u64, right: u64) -> u64 {
        ((left as u128 + right as u128) / 2) as u64
    }

    fn same_status_shape(base: &ExecutionStatus, tip: &ExecutionStatus) -> bool {
        matches!(
            (base, tip),
            (ExecutionStatus::Success, ExecutionStatus::Success)
                | (
                    ExecutionStatus::Failure { .. },
                    ExecutionStatus::Failure { .. }
                )
        )
    }

    fn effects_differ(
        base_effects: &TransactionEffects,
        tip_effects: &TransactionEffects,
        base_gas: &SuiGasStatus,
        tip_gas: &SuiGasStatus,
    ) -> bool {
        let status_differs = base_effects.status() != tip_effects.status();
        let gas_differs = !gas_within_tolerance(base_gas.gas_used(), tip_gas.gas_used());
        let shape_differs = base_effects != tip_effects;
        status_differs || gas_differs || shape_differs
    }

    struct TimingsSink {
        writer: BufWriter<File>,
        pending: usize,
    }

    static TIMINGS: OnceLock<Option<Mutex<TimingsSink>>> = OnceLock::new();
    static COMPLETED_PATHS_LOG_COUNT: AtomicU64 = AtomicU64::new(0);

    fn should_log_completed_paths() -> bool {
        let count = COMPLETED_PATHS_LOG_COUNT.fetch_add(1, Ordering::Relaxed);
        count < 5 || count % 100_000 == 0
    }

    fn timings() -> Option<&'static Mutex<TimingsSink>> {
        TIMINGS
            .get_or_init(|| {
                let path = Path::new(TIMINGS_FILE);
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        if let Err(err) = std::fs::create_dir_all(parent) {
                            tracing::error!(%err, "dual-replay: failed to create timings dir");
                            return None;
                        }
                    }
                }
                let is_new = !path.exists();
                match OpenOptions::new().create(true).append(true).open(path) {
                    Ok(mut f) => {
                        if is_new {
                            let _ = f.write_all(
                                b"digest,base1_ns,tip1_ns,tip2_ns,base2_ns,base1_loop_ns,tip1_loop_ns,tip2_loop_ns,base2_loop_ns,base1_gas,tip1_gas,tip2_gas,base2_gas,status_match_1,status_match_2
",
                            );
                        }
                        Some(Mutex::new(TimingsSink {
                            writer: BufWriter::new(f),
                            pending: 0,
                        }))
                    }
                    Err(err) => {
                        tracing::error!(%err, "dual-replay: failed to open timings file");
                        None
                    }
                }
            })
            .as_ref()
    }

    fn record_timing_abba(
        digest: TransactionDigest,
        base1_ns: u64,
        tip1_ns: u64,
        tip2_ns: u64,
        base2_ns: u64,
        base1_loop_ns: u64,
        tip1_loop_ns: u64,
        tip2_loop_ns: u64,
        base2_loop_ns: u64,
        base1_gas: u64,
        tip1_gas: u64,
        tip2_gas: u64,
        base2_gas: u64,
        status_match_1: bool,
        status_match_2: bool,
    ) {
        let Some(sink) = timings() else { return };
        let Ok(mut guard) = sink.lock() else { return };
        if writeln!(
            guard.writer,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            digest,
            base1_ns,
            tip1_ns,
            tip2_ns,
            base2_ns,
            base1_loop_ns,
            tip1_loop_ns,
            tip2_loop_ns,
            base2_loop_ns,
            base1_gas,
            tip1_gas,
            tip2_gas,
            base2_gas,
            status_match_1 as u8,
            status_match_2 as u8,
        )
        .is_ok()
        {
            guard.pending += 1;
            if guard.pending >= TIMINGS_FLUSH_EVERY {
                let _ = guard.writer.flush();
                guard.pending = 0;
            }
        }
    }

    fn gas_within_tolerance(base_gas: u64, tip_gas: u64) -> bool {
        if GAS_TOLERANCE_PCT <= 0.0 {
            return base_gas == tip_gas;
        }
        let base = base_gas as f64;
        let tip = tip_gas as f64;
        let delta = (base - tip).abs();
        let denom = base.max(1.0);
        (delta / denom) * 100.0 <= GAS_TOLERANCE_PCT
    }

    fn report_diff(
        base_effects: &TransactionEffects,
        tip_effects: &TransactionEffects,
        digest: TransactionDigest,
    ) {
        tracing::warn!(%digest, "dual-replay: effects differ");
        if let Err(err) = std::fs::create_dir_all(Path::new(OUTPUT_DIR)) {
            tracing::error!(%digest, %err, "dual-replay: failed to create output dir");
            return;
        }
        let base_path = format!("{}/{}.base.json", OUTPUT_DIR, digest);
        let tip_path = format!("{}/{}.tip.json", OUTPUT_DIR, digest);
        let base_json = serde_json::to_string_pretty(base_effects)
            .unwrap_or_else(|_| String::from("<failed to serialize base effects>"));
        let tip_json = serde_json::to_string_pretty(tip_effects)
            .unwrap_or_else(|_| String::from("<failed to serialize tip effects>"));
        if let Err(err) = std::fs::write(&base_path, base_json) {
            tracing::error!(%digest, %err, "dual-replay: failed to write base effects");
        }
        if let Err(err) = std::fs::write(&tip_path, tip_json) {
            tracing::error!(%digest, %err, "dual-replay: failed to write tip effects");
        }
    }
}
