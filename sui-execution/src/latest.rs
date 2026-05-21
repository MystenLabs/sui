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
        tracing::info!(%transaction_digest, "tx-backtest dual-replay: running tip execute_transaction");
        let tip_start = std::time::Instant::now();
        let (tip_store, tip_gas_status, tip_effects, _tip_timings, _tip_result) =
            execute_transaction_to_effects::<execution_mode::Normal>(
                store,
                input_objects.clone(),
                gas.clone(),
                gas_status.clone(),
                transaction_kind.clone(),
                rewritten_inputs.clone(),
                transaction_signer,
                transaction_digest,
                &self.0,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                metrics.clone(),
                enable_expensive_checks,
                execution_params.clone(),
                &mut None,
            );
        let tip_ns = tip_start.elapsed().as_nanos() as u64;
        tracing::info!(%transaction_digest, "tx-backtest dual-replay: running base/regular execute_transaction");
        let base_start = std::time::Instant::now();
        let base = {
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
        let base_ns = base_start.elapsed().as_nanos() as u64;
        self::latest_dual_replay::compare_dual_replay(
            (&base.0, &base.1, &base.2),
            (&tip_store, &tip_gas_status, &tip_effects),
            transaction_digest,
            base_ns,
            tip_ns,
        );
        if let Err(error) = &base.4 {
            log_execution_error(transaction_digest, error);
        }
        base
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
        tracing::info!(%transaction_digest, "tx-backtest dual-replay: running tip execute_transaction_with_error");
        let tip_start = std::time::Instant::now();
        let (tip_store, tip_gas_status, tip_effects, _tip_timings, _tip_result) =
            execute_transaction_to_effects::<execution_mode::Normal<ExecutionError>>(
                store,
                input_objects.clone(),
                gas.clone(),
                gas_status.clone(),
                transaction_kind.clone(),
                rewritten_inputs.clone(),
                transaction_signer,
                transaction_digest,
                &self.0,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                metrics.clone(),
                enable_expensive_checks,
                execution_params.clone(),
                &mut None,
            );
        let tip_ns = tip_start.elapsed().as_nanos() as u64;
        tracing::info!(%transaction_digest, "tx-backtest dual-replay: running base/regular execute_transaction_with_error");
        let base_start = std::time::Instant::now();
        let base = {
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
        let base_ns = base_start.elapsed().as_nanos() as u64;
        self::latest_dual_replay::compare_dual_replay(
            (&base.0, &base.1, &base.2),
            (&tip_store, &tip_gas_status, &tip_effects),
            transaction_digest,
            base_ns,
            tip_ns,
        );
        if let Err(error) = &base.4 {
            log_execution_error(transaction_digest, error);
        }
        base
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
    use std::sync::{Mutex, OnceLock};
    use sui_types::digests::TransactionDigest;
    use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
    use sui_types::execution_status::ExecutionStatus;
    use sui_types::gas::{SuiGasStatus, SuiGasStatusAPI};
    use sui_types::inner_temporary_store::InnerTemporaryStore;

    const OUTPUT_DIR: &str = "/opt/sui/replay-output/93a2eec772f3659092ea65ca313bf0d1c4b0e832";
    const GAS_TOLERANCE_PCT: f64 = 0.0_f64;
    const TIMINGS_FILE: &str =
        "/opt/sui/replay-output/93a2eec772f3659092ea65ca313bf0d1c4b0e832/timings.csv";
    const TIMINGS_FLUSH_EVERY: usize = 500;

    type View<'a> = (
        &'a InnerTemporaryStore,
        &'a SuiGasStatus,
        &'a TransactionEffects,
    );

    pub(super) fn compare_dual_replay(
        base: View<'_>,
        tip: View<'_>,
        digest: TransactionDigest,
        base_ns: u64,
        tip_ns: u64,
    ) {
        let (_, base_gas, base_effects) = base;
        let (_, tip_gas, tip_effects) = tip;
        let base_gas_used = base_gas.gas_used();
        let tip_gas_used = tip_gas.gas_used();
        let status_match = matches!(
            (base_effects.status(), tip_effects.status()),
            (ExecutionStatus::Success, ExecutionStatus::Success)
                | (
                    ExecutionStatus::Failure { .. },
                    ExecutionStatus::Failure { .. }
                )
        );
        record_timing(
            digest,
            base_ns,
            tip_ns,
            base_gas_used,
            tip_gas_used,
            status_match,
        );
        let differs = {
            let status_differs = base_effects.status() != tip_effects.status();
            let gas_differs = !gas_within_tolerance(base_gas.gas_used(), tip_gas.gas_used());
            let shape_differs = base_effects != tip_effects;
            status_differs || gas_differs || shape_differs
        };
        if differs {
            report_diff(base_effects, tip_effects, digest);
        }
    }

    struct TimingsSink {
        writer: BufWriter<File>,
        pending: usize,
    }

    static TIMINGS: OnceLock<Mutex<TimingsSink>> = OnceLock::new();

    fn timings() -> &'static Mutex<TimingsSink> {
        TIMINGS.get_or_init(|| {
            let path = Path::new(TIMINGS_FILE);
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .expect("dual-replay: failed to create timings dir");
                }
            }
            let is_new = !path.exists();
            let mut f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .expect("dual-replay: failed to open timings file");
            if is_new {
                f.write_all(
                    b"digest,base_ns,tip_ns,base_gas,tip_gas,status_match
",
                )
                .expect("dual-replay: failed to write timings header");
            }
            Mutex::new(TimingsSink {
                writer: BufWriter::new(f),
                pending: 0,
            })
        })
    }

    fn record_timing(
        digest: TransactionDigest,
        base_ns: u64,
        tip_ns: u64,
        base_gas: u64,
        tip_gas: u64,
        status_match: bool,
    ) {
        let mut guard = timings()
            .lock()
            .expect("dual-replay: timings mutex poisoned");
        writeln!(
            guard.writer,
            "{},{},{},{},{},{}",
            digest, base_ns, tip_ns, base_gas, tip_gas, status_match as u8,
        )
        .expect("dual-replay: failed to write timings row");
        guard.pending += 1;
        if guard.pending >= TIMINGS_FLUSH_EVERY {
            guard
                .writer
                .flush()
                .expect("dual-replay: failed to flush timings");
            guard.pending = 0;
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
        std::fs::create_dir_all(Path::new(OUTPUT_DIR))
            .expect("dual-replay: failed to create output dir");
        let base_path = format!("{}/{}.base.json", OUTPUT_DIR, digest);
        let tip_path = format!("{}/{}.tip.json", OUTPUT_DIR, digest);
        let base_json = serde_json::to_string_pretty(base_effects)
            .expect("dual-replay: failed to serialize base effects");
        let tip_json = serde_json::to_string_pretty(tip_effects)
            .expect("dual-replay: failed to serialize tip effects");
        std::fs::write(&base_path, base_json).expect("dual-replay: failed to write base effects");
        std::fs::write(&tip_path, tip_json).expect("dual-replay: failed to write tip effects");
    }
}
