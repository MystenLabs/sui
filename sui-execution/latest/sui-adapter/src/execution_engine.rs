// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
pub(crate) mod checked {

    use crate::adapter::new_move_runtime;
    use crate::execution_mode::{self, ExecutionMode};
    use crate::gas_charger::{PaymentKind, PaymentMethod};
    use move_binary_format::CompiledModule;
    use move_trace_format::format::MoveTraceBuilder;
    use move_vm_runtime::runtime::MoveRuntime;
    use mysten_common::{assert_reachable, debug_fatal_with_metric};
    use std::collections::{BTreeMap, BTreeSet};
    use std::{cell::RefCell, rc::Rc, sync::Arc};
    use sui_types::accumulator_root::{
        ACCUMULATOR_ROOT_CREATE_FUNC, ACCUMULATOR_ROOT_MODULE, UnsettledObjectFundsRead,
    };
    use sui_types::balance::{
        BALANCE_CREATE_REWARDS_FUNCTION_NAME, BALANCE_DESTROY_REBATES_FUNCTION_NAME,
        BALANCE_MODULE_NAME,
    };
    use sui_types::coin_reservation::ParsedDigest;
    use sui_types::execution_params::ExecutionOrEarlyError;
    use sui_types::gas_coin::GAS;
    use sui_types::messages_checkpoint::CheckpointTimestamp;
    use sui_types::metrics::ExecutionMetrics;
    use sui_types::object::OBJECT_START_VERSION;
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::randomness_state::{
        RANDOMNESS_MODULE_NAME, RANDOMNESS_STATE_CREATE_FUNCTION_NAME,
        RANDOMNESS_STATE_UPDATE_FUNCTION_NAME,
    };
    use sui_types::{BRIDGE_ADDRESS, SUI_BRIDGE_OBJECT_ID, SUI_RANDOMNESS_STATE_OBJECT_ID};
    use tracing::{info, instrument, trace, warn};

    use crate::static_programmable_transactions as SPT;
    use crate::sui_types::gas::SuiGasStatusAPI;
    use crate::{gas_charger::GasCharger, temporary_store::TemporaryStore};
    use move_core_types::ident_str;
    use sui_move_natives::all_natives;
    use sui_protocol_config::{
        LimitThresholdCrossed, PerObjectCongestionControlMode, ProtocolConfig, check_limit_by_meter,
    };
    use sui_types::authenticator_state::{
        AUTHENTICATOR_STATE_CREATE_FUNCTION_NAME, AUTHENTICATOR_STATE_EXPIRE_JWKS_FUNCTION_NAME,
        AUTHENTICATOR_STATE_MODULE_NAME, AUTHENTICATOR_STATE_UPDATE_FUNCTION_NAME,
    };
    use sui_types::base_types::{ObjectID, SequenceNumber, SystemObjectVersions};
    use sui_types::bridge::BRIDGE_COMMITTEE_MINIMAL_VOTING_POWER;
    use sui_types::bridge::{
        BRIDGE_CREATE_FUNCTION_NAME, BRIDGE_INIT_COMMITTEE_FUNCTION_NAME, BRIDGE_MODULE_NAME,
        BridgeChainId,
    };
    use sui_types::clock::{CLOCK_MODULE_NAME, CONSENSUS_COMMIT_PROLOGUE_FUNCTION_NAME};
    use sui_types::committee::EpochId;
    use sui_types::deny_list_v1::{DENY_LIST_CREATE_FUNC, DENY_LIST_MODULE};
    use sui_types::digests::{
        ChainIdentifier, get_mainnet_chain_identifier, get_testnet_chain_identifier,
    };
    use sui_types::effects::TransactionEffects;
    use sui_types::error::{ExecutionError, ExecutionErrorTrait};
    use sui_types::execution::{ExecutionTiming, ResultWithTimings};
    use sui_types::execution_status::{ExecutionErrorKind, ExecutionStatus};
    use sui_types::gas::GasCostSummary;
    use sui_types::gas::SuiGasStatus;
    use sui_types::id::UID;
    use sui_types::inner_temporary_store::InnerTemporaryStore;
    use sui_types::storage::BackingStore;
    #[cfg(msim)]
    use sui_types::sui_system_state::advance_epoch_result_injection::maybe_modify_result_for;
    use sui_types::sui_system_state::{ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME, AdvanceEpochParams};
    use sui_types::transaction::{
        Argument, AuthenticatorStateExpire, AuthenticatorStateUpdate, CallArg, ChangeEpoch,
        Command, EndOfEpochTransactionKind, GasData, GenesisTransaction, ObjectArg,
        ProgrammableTransaction, StoredExecutionTimeObservations, TransactionKind,
        WriteAccumulatorStorageCost, is_gasless_transaction,
    };
    use sui_types::transaction::{CheckedInputObjects, RandomnessStateUpdate};
    use sui_types::{
        SUI_AUTHENTICATOR_STATE_OBJECT_ID, SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID,
        SUI_SYSTEM_PACKAGE_ID,
        base_types::{SuiAddress, TransactionDigest, TxContext},
        object::{Object, ObjectInner},
        sui_system_state::{ADVANCE_EPOCH_FUNCTION_NAME, SUI_SYSTEM_MODULE_NAME},
    };

    /// Whether `InsufficientFundsForWithdraw` appears anywhere in the early-error list.
    fn should_short_circuit_insufficient_funds(execution_params: &ExecutionOrEarlyError) -> bool {
        execution_params.early_errors().is_some_and(|errors| {
            errors
                .iter()
                .any(|e| matches!(e, ExecutionErrorKind::InsufficientFundsForWithdraw))
        })
    }

    fn payment_kind(
        gas_data: &GasData,
        transaction_kind: &TransactionKind,
    ) -> Result<PaymentKind, ExecutionError> {
        if gas_data.is_unmetered() || transaction_kind.is_system_tx() {
            return Ok(PaymentKind::unmetered());
        }
        if is_gasless_transaction(gas_data, transaction_kind) {
            return Ok(PaymentKind::gasless());
        }
        let payment_methods = if gas_data.payment.is_empty() {
            vec![PaymentMethod::AddressBalance(
                gas_data.owner,
                gas_data.budget,
            )]
        } else {
            gas_data
                .payment
                .iter()
                .map(|entry| {
                    if let Ok(parsed) = ParsedDigest::try_from(entry.2) {
                        PaymentMethod::AddressBalance(gas_data.owner, parsed.reservation_amount())
                    } else {
                        PaymentMethod::Coin(*entry)
                    }
                })
                .collect()
        };
        PaymentKind::smash(payment_methods).ok_or_else(|| {
            ExecutionError::invariant_violation(
                "unable to create a payment kind from the gas payment: \
                 duplicate gas coin or reservation overflow",
            )
        })
    }

    /// Everything `execute_transaction_to_effects` hands back to the executor layer.
    pub struct ExecutionOutput<Mode: ExecutionMode> {
        pub inner_store: InnerTemporaryStore,
        pub gas_status: SuiGasStatus,
        pub effects: TransactionEffects,
        pub timings: Vec<ExecutionTiming>,
        pub execution_result: Result<Mode::ExecutionResults, Mode::Error>,
    }

    /// Gas summary, execution result, and timings produced by `execute_transaction`.
    struct ExecutionOutcome<Mode: ExecutionMode> {
        cost_summary: GasCostSummary,
        execution_result: Result<Mode::ExecutionResults, Mode::Error>,
        timings: Vec<ExecutionTiming>,
    }
    #[instrument(name = "tx_execute_to_effects", level = "debug", skip_all)]
    pub fn execute_transaction_to_effects<Mode: ExecutionMode>(
        store: &dyn BackingStore,
        input_objects: CheckedInputObjects,
        system_object_versions: SystemObjectVersions,
        unsettled_object_funds: &dyn UnsettledObjectFundsRead,
        gas_data: GasData,
        gas_status: SuiGasStatus,
        transaction_kind: TransactionKind,
        rewritten_inputs: Option<Vec<bool>>,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        move_vm: &Arc<MoveRuntime>,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        enable_expensive_checks: bool,
        execution_params: ExecutionOrEarlyError,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) -> ExecutionOutput<Mode> {
        let input_objects = input_objects.into_inner();
        let shared_object_refs = input_objects.filter_shared_objects();
        let receiving_objects = transaction_kind.receiving_objects();
        let mut transaction_dependencies = if protocol_config.disable_effects_tx_dependencies() {
            BTreeSet::new()
        } else {
            input_objects.transaction_dependencies()
        };

        let mut temporary_store = TemporaryStore::new(
            store,
            input_objects,
            receiving_objects,
            transaction_digest,
            protocol_config,
            *epoch_id,
            system_object_versions,
            (&transaction_kind, &gas_data, transaction_signer),
            unsettled_object_funds,
        );

        let Finalized {
            cost_summary,
            gas_coin,
            gas_status,
            status,
            timings,
            execution_result,
        }: Finalized<Mode> = match execute_transaction_to_outcome::<Mode>(
            store,
            &mut temporary_store,
            gas_data,
            gas_status,
            transaction_kind,
            rewritten_inputs,
            transaction_signer,
            transaction_digest,
            move_vm,
            epoch_id,
            epoch_timestamp_ms,
            protocol_config,
            metrics.clone(),
            enable_expensive_checks,
            execution_params,
            trace_builder_opt,
        ) {
            Outcome::Proceed {
                gas_charger,
                gas_cost_summary,
                execution_result,
                timings,
            } => {
                let status = if let Err(error) = &execution_result {
                    ExecutionStatus::new_failure(error.to_execution_failure())
                } else {
                    ExecutionStatus::Success
                };
                Finalized {
                    cost_summary: gas_cost_summary,
                    gas_coin: gas_charger.gas_coin(),
                    gas_status: gas_charger.into_gas_status(),
                    status,
                    timings,
                    execution_result,
                }
            }
            Outcome::BumpOnly {
                gas_status,
                error,
                reason,
            } => {
                report_bump_only::<Mode>(reason, &transaction_digest, &error);
                // Rebuild the store from its inputs, keeping only the input version bumps.
                temporary_store = temporary_store.into_bump_only();
                Finalized {
                    cost_summary: GasCostSummary::default(),
                    gas_coin: None,
                    gas_status,
                    status: ExecutionStatus::new_failure(error.to_execution_failure()),
                    timings: vec![],
                    execution_result: Err(error),
                }
            }
        };

        // Shared infallible tail: trim the genesis dependency, build effects, telemetry.
        #[skip_checked_arithmetic]
        trace!(
            tx_digest = ?transaction_digest,
            computation_gas_cost = cost_summary.computation_cost,
            storage_gas_cost = cost_summary.storage_cost,
            storage_gas_rebate = cost_summary.storage_rebate,
            "Finished execution of transaction with status {:?}",
            status
        );
        transaction_dependencies.remove(&TransactionDigest::genesis_marker());
        let (inner, effects) = temporary_store.into_effects(
            shared_object_refs,
            &transaction_digest,
            transaction_dependencies,
            cost_summary,
            status,
            gas_coin,
            *epoch_id,
        );
        // Skip VM telemetry on simulation paths (dev-inspect / dry-run) since a new runtime is
        // spun-up each time.
        if !Mode::TRACK_EXECUTION {
            update_vm_telemetry_metrics(&metrics, move_vm);
        }
        ExecutionOutput {
            inner_store: inner,
            gas_status,
            effects,
            timings,
            execution_result,
        }
    }

    /// Post-execution consistency: SUI conservation, the expensive ownership invariants, and (on
    /// successful execution) the published-packages invariant. `Err` means an invariant was
    /// violated unrecoverably - no panic, no recovery; the caller bails to `BumpOnly` reporting
    /// the error.
    #[allow(clippy::too_many_arguments)]
    fn check_consistency<Mode: ExecutionMode>(
        temporary_store: &mut TemporaryStore<'_>,
        gas_charger: &GasCharger,
        gas_cost_summary: &GasCostSummary,
        move_vm: &Arc<MoveRuntime>,
        enable_expensive_checks: bool,
        transaction_signer: SuiAddress,
        sponsor: Option<SuiAddress>,
        is_epoch_change: bool,
        transaction_digest: TransactionDigest,
        execution_succeeded: bool,
    ) -> Result<(), (Mode::Error, BumpOnlyReason)> {
        // FIXME: we cannot fail the transaction if this is an epoch change transaction.
        run_conservation_checks::<Mode>(
            temporary_store,
            gas_charger,
            transaction_digest,
            move_vm,
            enable_expensive_checks,
            gas_cost_summary,
        )
        .map_err(|error| (error, BumpOnlyReason::Conservation))?;

        // Ownership invariants - only under expensive checks + non-arbitrary mode; a violation is a
        // real bug that should never fire.
        if enable_expensive_checks
            && !Mode::allow_arbitrary_function_calls()
            && let Err(err) = temporary_store.check_ownership_invariants(
                &transaction_signer,
                &sponsor,
                gas_charger,
                is_epoch_change,
            )
        {
            #[skip_checked_arithmetic]
            tracing::error!(
                tx_digest = ?transaction_digest,
                error = %err,
                "ownership invariants violated; falling back to the no-op exit (State 2): \
                 dropping all writes and charging nothing",
            );
            return Err((
                ExecutionError::from_kind(ExecutionErrorKind::InvariantViolation).into(),
                BumpOnlyReason::Ownership,
            ));
        }

        // Written packages must match the PTB's publish/upgrade commands; only meaningful when
        // execution succeeded (on failure the writes were dropped).
        if execution_succeeded {
            temporary_store
                .check_published_packages()
                .map_err(|error| (error.into(), BumpOnlyReason::PublishedPackages))?;
        }

        Ok(())
    }

    fn update_vm_telemetry_metrics(metrics: &ExecutionMetrics, move_vm: &MoveRuntime) {
        metrics.vm_telemetry_metrics.try_update(|vm_metrics| {
            let t = move_vm.get_telemetry_report();
            vm_metrics
                .move_vm_package_cache_count
                .set(t.package_cache_count as i64);
            vm_metrics
                .move_vm_total_arena_size_bytes
                .set(t.total_arena_size as i64);
            vm_metrics.move_vm_module_count.set(t.module_count as i64);
            vm_metrics
                .move_vm_function_count
                .set(t.function_count as i64);
            vm_metrics.move_vm_type_count.set(t.type_count as i64);
            vm_metrics.move_vm_interner_size.set(t.interner_size as i64);
            vm_metrics
                .move_vm_vtable_cache_count
                .set(t.vtable_cache_count as i64);
            vm_metrics
                .move_vm_vtable_cache_hits
                .set(t.vtable_cache_hits as i64);
            vm_metrics
                .move_vm_vtable_cache_misses
                .set(t.vtable_cache_misses as i64);
            vm_metrics
                .move_vm_load_time_ms
                .set(t.total_load_time as i64);
            vm_metrics.move_vm_load_count.set(t.load_count as i64);
            vm_metrics
                .move_vm_validation_time_ms
                .set(t.total_validation_time as i64);
            vm_metrics
                .move_vm_validation_count
                .set(t.validation_count as i64);
            vm_metrics.move_vm_jit_time_ms.set(t.total_jit_time as i64);
            vm_metrics.move_vm_jit_count.set(t.jit_count as i64);
            vm_metrics
                .move_vm_execution_time_ms
                .set(t.total_execution_time as i64);
            vm_metrics
                .move_vm_execution_count
                .set(t.execution_count as i64);
            vm_metrics
                .move_vm_interpreter_time_ms
                .set(t.total_interpreter_time as i64);
            vm_metrics
                .move_vm_interpreter_count
                .set(t.interpreter_count as i64);
            vm_metrics
                .move_vm_max_callstack_size
                .set(t.max_callstack_size as i64);
            vm_metrics
                .move_vm_max_valuestack_size
                .set(t.max_valuestack_size as i64);
            vm_metrics.move_vm_total_time_ms.set(t.total_time as i64);
            vm_metrics.move_vm_total_count.set(t.total_count as i64);
        });
    }

    pub fn execute_genesis_state_update(
        store: &dyn BackingStore,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        move_vm: &Arc<MoveRuntime>,
        tx_context: Rc<RefCell<TxContext>>,
        pt: ProgrammableTransaction,
    ) -> Result<InnerTemporaryStore, ExecutionError> {
        let mut temporary_store = TemporaryStore::new_for_genesis_state_update(
            store,
            tx_context.borrow().digest(),
            protocol_config,
        );
        let mut gas_charger =
            GasCharger::new_unmetered(tx_context.borrow().digest(), protocol_config);
        SPT::execute::<execution_mode::Genesis>(
            protocol_config,
            metrics,
            move_vm,
            &mut temporary_store,
            store,
            tx_context,
            &mut gas_charger,
            None,
            pt,
            &mut None,
        )
        .map_err(|(e, _)| e)?;
        temporary_store.update_object_version_and_prev_tx();
        Ok(temporary_store.into_inner(BTreeMap::new()))
    }

    #[allow(clippy::large_enum_variant)]
    enum Outcome<Mode: ExecutionMode> {
        Proceed {
            gas_charger: GasCharger,
            gas_cost_summary: GasCostSummary,
            execution_result: Result<Mode::ExecutionResults, Mode::Error>,
            timings: Vec<ExecutionTiming>,
        },
        BumpOnly {
            gas_status: SuiGasStatus,
            error: Mode::Error,
            reason: BumpOnlyReason,
        },
    }

    /// Which stage bailed to the `BumpOnly` exit. `InsufficientFundsForWithdraw` is
    /// the one expected reason; every other variant is an execution bug and is reported to
    /// `execution_bump_only_exits`, whose `reason` label is `Self::label`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum BumpOnlyReason {
        InsufficientFundsForWithdraw,
        GasSmash,
        WriteReset,
        Conservation,
        Ownership,
        PublishedPackages,
    }

    impl BumpOnlyReason {
        /// Metric label. Alerts select on these, so keep the values stable.
        fn label(self) -> &'static str {
            match self {
                Self::InsufficientFundsForWithdraw => "insufficient_funds_for_withdraw",
                Self::GasSmash => "gas_smash",
                Self::WriteReset => "write_reset",
                Self::Conservation => "conservation",
                Self::Ownership => "ownership",
                Self::PublishedPackages => "published_packages",
            }
        }

        fn is_expected(self) -> bool {
            matches!(self, Self::InsufficientFundsForWithdraw)
        }
    }

    struct Finalized<Mode: ExecutionMode> {
        cost_summary: GasCostSummary,
        gas_coin: Option<ObjectID>,
        gas_status: SuiGasStatus,
        status: ExecutionStatus,
        timings: Vec<ExecutionTiming>,
        execution_result: Result<Mode::ExecutionResults, Mode::Error>,
    }

    /// Report an unexpected `BumpOnly` exit: a transaction whose writes were all dropped and which
    /// was charged nothing because a stage of the pipeline failed. `debug_fatal` semantics - panics
    /// under `crash_on_debug()`, counts + logs in production.
    ///
    /// Skipped for the expected IFFW short-circuit, and for the simulation paths
    /// (`Mode::TRACK_EXECUTION`: dev-inspect / dry-run / simulate), where an arbitrary user-supplied
    /// transaction must not be able to crash a debug node or raise an alert.
    fn report_bump_only<Mode: ExecutionMode>(
        reason: BumpOnlyReason,
        transaction_digest: &TransactionDigest,
        error: &Mode::Error,
    ) {
        if reason.is_expected() || Mode::TRACK_EXECUTION {
            return;
        }
        debug_fatal_with_metric!(
            |metrics: &mysten_metrics::Metrics| {
                metrics
                    .execution_bump_only_exits
                    .with_label_values(&[reason.label()])
                    .inc();
            },
            "BumpOnly exit: all writes dropped, no gas charged. \
             reason={}, tx_digest={:?}, error={:?}",
            reason.label(),
            transaction_digest,
            error
        );
    }

    fn execute_transaction_to_outcome<Mode: ExecutionMode>(
        store: &dyn BackingStore,
        temporary_store: &mut TemporaryStore<'_>,
        gas_data: GasData,
        gas_status: SuiGasStatus,
        transaction_kind: TransactionKind,
        rewritten_inputs: Option<Vec<bool>>,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        move_vm: &Arc<MoveRuntime>,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        enable_expensive_checks: bool,
        execution_params: ExecutionOrEarlyError,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) -> Outcome<Mode> {
        // Short-circuit insufficient_funds. No execution, `Outcome::BumpOnly`
        if should_short_circuit_insufficient_funds(&execution_params) {
            assert_reachable!("IFFW short-circuit fired");
            let iffw: Mode::Error =
                ExecutionError::from_kind(ExecutionErrorKind::InsufficientFundsForWithdraw).into();
            return Outcome::BumpOnly {
                gas_status,
                error: iffw,
                reason: BumpOnlyReason::InsufficientFundsForWithdraw,
            };
        }

        let sponsor = (gas_data.owner != transaction_signer).then_some(gas_data.owner);
        let gas_price = gas_status.gas_price();
        let rgp = gas_status.reference_gas_price();
        let is_epoch_change = transaction_kind.is_end_of_epoch_tx();

        let tx_ctx = TxContext::new_from_components(
            &transaction_signer,
            &transaction_digest,
            epoch_id,
            epoch_timestamp_ms,
            rgp,
            gas_price,
            gas_data.budget,
            sponsor,
            protocol_config,
        );
        let tx_ctx = Rc::new(RefCell::new(tx_ctx));

        let payment_kind = match payment_kind(&gas_data, &transaction_kind) {
            Ok(payment_kind) => payment_kind,
            Err(error) => {
                return Outcome::BumpOnly {
                    gas_status,
                    error: error.into(),
                    reason: BumpOnlyReason::GasSmash,
                };
            }
        };
        let mut gas_charger = GasCharger::new(
            transaction_digest,
            payment_kind,
            gas_status,
            temporary_store,
        );
        let ExecutionOutcome {
            cost_summary: gas_cost_summary,
            execution_result,
            timings,
        } = match execute_transaction::<Mode>(
            store,
            temporary_store,
            transaction_kind,
            rewritten_inputs,
            &mut gas_charger,
            tx_ctx,
            move_vm,
            protocol_config,
            metrics,
            execution_params,
            trace_builder_opt,
        ) {
            Err((error, reason)) => {
                return Outcome::BumpOnly {
                    gas_status: gas_charger.into_gas_status(),
                    error,
                    reason,
                };
            }
            Ok(outcome) => outcome,
        };

        // Post-execution consistency (conservation + ownership + published packages): on violation
        // bail to `BumpOnly` with the gas_status recovered from the charger
        match check_consistency::<Mode>(
            temporary_store,
            &gas_charger,
            &gas_cost_summary,
            move_vm,
            enable_expensive_checks,
            transaction_signer,
            sponsor,
            is_epoch_change,
            transaction_digest,
            execution_result.is_ok(),
        ) {
            Ok(()) => Outcome::Proceed {
                gas_charger,
                gas_cost_summary,
                execution_result,
                timings,
            },
            Err((error, reason)) => Outcome::BumpOnly {
                gas_status: gas_charger.into_gas_status(),
                error,
                reason,
            },
        }
    }

    #[instrument(name = "tx_execute", level = "debug", skip_all)]
    fn execute_transaction<Mode: ExecutionMode>(
        store: &dyn BackingStore,
        temporary_store: &mut TemporaryStore<'_>,
        transaction_kind: TransactionKind,
        rewritten_inputs: Option<Vec<bool>>,
        gas_charger: &mut GasCharger,
        tx_ctx: Rc<RefCell<TxContext>>,
        move_vm: &Arc<MoveRuntime>,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        execution_params: ExecutionOrEarlyError,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) -> Result<ExecutionOutcome<Mode>, (Mode::Error, BumpOnlyReason)> {
        debug_assert!(
            gas_charger.no_charges(),
            "No gas charges must be applied yet"
        );

        let mut timings: Vec<ExecutionTiming> = vec![];

        let result = gas_charger
            .charge_input_objects(temporary_store)
            .map_err(Into::into)
            // Early errors fail without running the VM
            .and_then(|()| match execution_params.into_early_errors() {
                Some(early_execution_errors) => {
                    Err(ExecutionError::new(early_execution_errors.head, None).into())
                }
                None => execute_ptb::<Mode>(
                    store,
                    temporary_store,
                    transaction_kind,
                    rewritten_inputs,
                    tx_ctx,
                    move_vm,
                    gas_charger,
                    protocol_config,
                    metrics.clone(),
                    trace_builder_opt,
                    &mut timings,
                ),
            })
            .and_then(|v| {
                gas_charger
                    .meter_storage(temporary_store)
                    .map_err(Into::into)
                    .map(|_| v)
            });

        let checks = check_effects::<Mode>(temporary_store, gas_charger, protocol_config, &metrics);
        // Execution error wins; otherwise a failed effects check fails the tx.
        let result = result.and_then(|v| checks.map(|()| v));

        if result.is_err() {
            gas_charger
                .handle_error(temporary_store)
                .map_err(|error| (error.into(), BumpOnlyReason::WriteReset))?;
        }
        let cost_summary = gas_charger.charge(temporary_store, &result);
        Ok(ExecutionOutcome {
            cost_summary,
            execution_result: result,
            timings,
        })
    }

    /// Execute the PTB, then bucketize computation via `round_computation`. Timings are written to
    /// `timings_out` regardless of Ok/Err.
    fn execute_ptb<Mode: ExecutionMode>(
        store: &dyn BackingStore,
        temporary_store: &mut TemporaryStore<'_>,
        transaction_kind: TransactionKind,
        rewritten_inputs: Option<Vec<bool>>,
        tx_ctx: Rc<RefCell<TxContext>>,
        move_vm: &Arc<MoveRuntime>,
        gas_charger: &mut GasCharger,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
        timings_out: &mut Vec<ExecutionTiming>,
    ) -> Result<Mode::ExecutionResults, Mode::Error> {
        let result = match execution_loop::<Mode>(
            store,
            temporary_store,
            transaction_kind,
            rewritten_inputs,
            tx_ctx,
            move_vm,
            gas_charger,
            protocol_config,
            metrics,
            trace_builder_opt,
        ) {
            Ok((v, t)) => {
                *timings_out = t;
                Ok(v)
            }
            Err((e, t)) => {
                *timings_out = t;
                Err(e)
            }
        };
        gas_charger.round_computation(result)
    }

    fn check_effects<Mode: ExecutionMode>(
        temporary_store: &TemporaryStore<'_>,
        gas_charger: &GasCharger,
        protocol_config: &ProtocolConfig,
        metrics: &ExecutionMetrics,
    ) -> Result<(), Mode::Error> {
        let meter =
            check_meter_limit::<Mode>(temporary_store, gas_charger, protocol_config, metrics);
        let written = check_written_objects_limit::<Mode>(
            temporary_store,
            gas_charger,
            protocol_config,
            metrics,
        );
        let representable = temporary_store
            .check_accumulator_amounts_representable()
            .map_err(Into::into);
        meter.and(written).and(representable)
    }

    #[instrument(name = "run_conservation_checks", level = "debug", skip_all)]
    fn run_conservation_checks<Mode: ExecutionMode>(
        temporary_store: &mut TemporaryStore<'_>,
        gas_charger: &GasCharger,
        tx_digest: TransactionDigest,
        move_vm: &Arc<MoveRuntime>,
        enable_expensive_checks: bool,
        cost_summary: &GasCostSummary,
    ) -> Result<(), Mode::Error> {
        if let Err(conservation_err) = temporary_store.check_conservation_invariants::<Mode>(
            move_vm,
            enable_expensive_checks,
            cost_summary,
        ) {
            #[skip_checked_arithmetic]
            tracing::error!(
                tx_digest = ?tx_digest,
                conservation_error = %conservation_err,
                gas_status = %gas_charger.summary(),
                "SUI conservation check failed; falling back to the no-op exit (State 2): \
                 dropping all writes and charging nothing",
            );
            return Err(conservation_err.into());
        }

        Ok(())
    }

    #[instrument(name = "check_meter_limit", level = "debug", skip_all)]
    fn check_meter_limit<Mode: ExecutionMode>(
        temporary_store: &TemporaryStore<'_>,
        gas_charger: &GasCharger,
        protocol_config: &ProtocolConfig,
        metrics: &ExecutionMetrics,
    ) -> Result<(), Mode::Error> {
        let effects_estimated_size = temporary_store.estimate_effects_size_upperbound();

        // Check if a limit threshold was crossed.
        // For metered transactions, there is not soft limit.
        // For system transactions, we allow a soft limit with alerting, and a hard limit where we terminate
        match check_limit_by_meter!(
            !gas_charger.is_unmetered(),
            effects_estimated_size,
            protocol_config.max_serialized_tx_effects_size_bytes(),
            protocol_config.max_serialized_tx_effects_size_bytes_system_tx(),
            metrics.limits_metrics.excessive_estimated_effects_size
        ) {
            LimitThresholdCrossed::None => Ok(()),
            LimitThresholdCrossed::Soft(_, limit) => {
                warn!(
                    effects_estimated_size = effects_estimated_size,
                    soft_limit = limit,
                    "Estimated transaction effects size crossed soft limit",
                );
                Ok(())
            }
            LimitThresholdCrossed::Hard(_, lim) => Err(Mode::Error::new_with_source(
                ExecutionErrorKind::EffectsTooLarge {
                    current_size: effects_estimated_size as u64,
                    max_size: lim as u64,
                },
                "Transaction effects are too large",
            )),
        }
    }

    #[instrument(name = "check_written_objects_limit", level = "debug", skip_all)]
    fn check_written_objects_limit<Mode: ExecutionMode>(
        temporary_store: &TemporaryStore<'_>,
        gas_charger: &GasCharger,
        protocol_config: &ProtocolConfig,
        metrics: &ExecutionMetrics,
    ) -> Result<(), Mode::Error> {
        if let (Some(normal_lim), Some(system_lim)) = (
            protocol_config.max_size_written_objects_as_option(),
            protocol_config.max_size_written_objects_system_tx_as_option(),
        ) {
            let written_objects_size = temporary_store.written_objects_size();

            match check_limit_by_meter!(
                !gas_charger.is_unmetered(),
                written_objects_size,
                normal_lim,
                system_lim,
                metrics.limits_metrics.excessive_written_objects_size
            ) {
                LimitThresholdCrossed::None => (),
                LimitThresholdCrossed::Soft(_, limit) => {
                    warn!(
                        written_objects_size = written_objects_size,
                        soft_limit = limit,
                        "Written objects size crossed soft limit",
                    )
                }
                LimitThresholdCrossed::Hard(_, lim) => {
                    return Err(Mode::Error::new_with_source(
                        ExecutionErrorKind::WrittenObjectsTooLarge {
                            current_size: written_objects_size as u64,
                            max_size: lim as u64,
                        },
                        "Written objects size crossed hard limit",
                    ));
                }
            };
        }

        Ok(())
    }

    #[instrument(level = "debug", skip_all)]
    fn execution_loop<Mode: ExecutionMode>(
        store: &dyn BackingStore,
        temporary_store: &mut TemporaryStore<'_>,
        transaction_kind: TransactionKind,
        rewritten_inputs: Option<Vec<bool>>,
        tx_ctx: Rc<RefCell<TxContext>>,
        move_vm: &Arc<MoveRuntime>,
        gas_charger: &mut GasCharger,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) -> ResultWithTimings<Mode::ExecutionResults, Mode::Error> {
        let result = match transaction_kind {
            TransactionKind::ChangeEpoch(change_epoch) => {
                let builder = ProgrammableTransactionBuilder::new();
                advance_epoch::<Mode>(
                    builder,
                    change_epoch,
                    temporary_store,
                    store,
                    tx_ctx,
                    move_vm,
                    gas_charger,
                    protocol_config,
                    metrics,
                    trace_builder_opt,
                )
                .map_err(|e| (e, vec![]))?;
                Ok((Mode::empty_results(), vec![]))
            }
            TransactionKind::Genesis(GenesisTransaction { objects }) => {
                if tx_ctx.borrow().epoch() != 0 {
                    panic!("BUG: Genesis Transactions can only be executed in epoch 0");
                }

                for genesis_object in objects {
                    match genesis_object {
                        sui_types::transaction::GenesisObject::RawObject { data, owner } => {
                            let object = ObjectInner {
                                data,
                                owner,
                                previous_transaction: tx_ctx.borrow().digest(),
                                storage_rebate: 0,
                            };
                            temporary_store.create_object(object.into());
                        }
                    }
                }
                Ok((Mode::empty_results(), vec![]))
            }
            TransactionKind::ConsensusCommitPrologue(prologue) => {
                setup_consensus_commit::<Mode>(
                    prologue.commit_timestamp_ms,
                    temporary_store,
                    store,
                    tx_ctx,
                    move_vm,
                    gas_charger,
                    protocol_config,
                    metrics,
                    trace_builder_opt,
                )
                .expect("ConsensusCommitPrologue cannot fail");
                Ok((Mode::empty_results(), vec![]))
            }
            TransactionKind::ConsensusCommitPrologueV2(prologue) => {
                setup_consensus_commit::<Mode>(
                    prologue.commit_timestamp_ms,
                    temporary_store,
                    store,
                    tx_ctx,
                    move_vm,
                    gas_charger,
                    protocol_config,
                    metrics,
                    trace_builder_opt,
                )
                .expect("ConsensusCommitPrologueV2 cannot fail");
                Ok((Mode::empty_results(), vec![]))
            }
            TransactionKind::ConsensusCommitPrologueV3(prologue) => {
                setup_consensus_commit::<Mode>(
                    prologue.commit_timestamp_ms,
                    temporary_store,
                    store,
                    tx_ctx,
                    move_vm,
                    gas_charger,
                    protocol_config,
                    metrics,
                    trace_builder_opt,
                )
                .expect("ConsensusCommitPrologueV3 cannot fail");
                Ok((Mode::empty_results(), vec![]))
            }
            TransactionKind::ConsensusCommitPrologueV4(prologue) => {
                setup_consensus_commit::<Mode>(
                    prologue.commit_timestamp_ms,
                    temporary_store,
                    store,
                    tx_ctx,
                    move_vm,
                    gas_charger,
                    protocol_config,
                    metrics,
                    trace_builder_opt,
                )
                .expect("ConsensusCommitPrologue cannot fail");
                Ok((Mode::empty_results(), vec![]))
            }
            TransactionKind::ProgrammableTransaction(pt) => SPT::execute::<Mode>(
                protocol_config,
                metrics,
                move_vm,
                temporary_store,
                store,
                tx_ctx,
                gas_charger,
                rewritten_inputs,
                pt,
                trace_builder_opt,
            ),
            TransactionKind::ProgrammableSystemTransaction(pt) => {
                SPT::execute::<execution_mode::System<Mode::Error>>(
                    protocol_config,
                    metrics,
                    move_vm,
                    temporary_store,
                    store,
                    tx_ctx,
                    gas_charger,
                    None,
                    pt,
                    trace_builder_opt,
                )
                .map_err(|(e, _)| (e, vec![]))?;
                Ok((Mode::empty_results(), vec![]))
            }
            TransactionKind::EndOfEpochTransaction(txns) => {
                let mut builder = ProgrammableTransactionBuilder::new();
                let len = txns.len();
                for (i, tx) in txns.into_iter().enumerate() {
                    match tx {
                        EndOfEpochTransactionKind::ChangeEpoch(change_epoch) => {
                            assert_eq!(i, len - 1);
                            advance_epoch::<Mode>(
                                builder,
                                change_epoch,
                                temporary_store,
                                store,
                                tx_ctx,
                                move_vm,
                                gas_charger,
                                protocol_config,
                                metrics,
                                trace_builder_opt,
                            )
                            .map_err(|e| (e, vec![]))?;
                            return Ok((Mode::empty_results(), vec![]));
                        }
                        EndOfEpochTransactionKind::AuthenticatorStateCreate => {
                            assert!(protocol_config.enable_jwk_consensus_updates());
                            builder = setup_authenticator_state_create(builder);
                        }
                        EndOfEpochTransactionKind::AuthenticatorStateExpire(expire) => {
                            assert!(protocol_config.enable_jwk_consensus_updates());

                            // TODO: it would be nice if a failure of this function didn't cause
                            // safe mode.
                            builder = setup_authenticator_state_expire(builder, expire);
                        }
                        EndOfEpochTransactionKind::RandomnessStateCreate => {
                            assert!(protocol_config.random_beacon());
                            builder = setup_randomness_state_create(builder);
                        }
                        EndOfEpochTransactionKind::DenyListStateCreate => {
                            assert!(protocol_config.enable_coin_deny_list());
                            builder = setup_coin_deny_list_state_create(builder);
                        }
                        EndOfEpochTransactionKind::BridgeStateCreate(chain_id) => {
                            assert!(protocol_config.bridge());
                            builder = setup_bridge_create(builder, chain_id)
                        }
                        EndOfEpochTransactionKind::BridgeCommitteeInit(bridge_shared_version) => {
                            assert!(protocol_config.bridge());
                            assert!(protocol_config.should_try_to_finalize_bridge_committee());
                            builder = setup_bridge_committee_update(builder, bridge_shared_version)
                        }
                        EndOfEpochTransactionKind::StoreExecutionTimeObservations(estimates) => {
                            if let PerObjectCongestionControlMode::ExecutionTimeEstimate(params) =
                                protocol_config.per_object_congestion_control_mode()
                            {
                                let chunk_size = params
                                    .observations_chunk_size
                                    .expect("observation chunking is enabled at all protocol versions handled by this execution layer");
                                builder = setup_store_execution_time_estimates(
                                    builder,
                                    estimates,
                                    chunk_size as usize,
                                );
                            }
                        }
                        EndOfEpochTransactionKind::AccumulatorRootCreate => {
                            assert!(protocol_config.create_root_accumulator_object());
                            builder = setup_accumulator_root_create(builder);
                        }
                        EndOfEpochTransactionKind::WriteAccumulatorStorageCost(
                            write_storage_cost,
                        ) => {
                            assert!(protocol_config.enable_accumulators());
                            builder =
                                setup_write_accumulator_storage_cost(builder, &write_storage_cost);
                        }
                        EndOfEpochTransactionKind::CoinRegistryCreate => {
                            assert!(protocol_config.enable_coin_registry());
                            builder = setup_coin_registry_create(builder);
                        }
                        EndOfEpochTransactionKind::DisplayRegistryCreate => {
                            assert!(protocol_config.enable_display_registry());
                            builder = setup_display_registry_create(builder);
                        }
                        EndOfEpochTransactionKind::AddressAliasStateCreate => {
                            assert!(protocol_config.address_aliases());
                            builder = setup_address_alias_state_create(builder);
                        }
                        EndOfEpochTransactionKind::ForwardingAddressRegistryCreate => {
                            assert!(protocol_config.create_forwarding_address_registry());
                            builder = setup_forwarding_address_registry_create(builder);
                        }
                    }
                }
                unreachable!(
                    "EndOfEpochTransactionKind::ChangeEpoch should be the last transaction in the list"
                )
            }
            TransactionKind::AuthenticatorStateUpdate(auth_state_update) => {
                setup_authenticator_state_update::<Mode>(
                    auth_state_update,
                    temporary_store,
                    store,
                    tx_ctx,
                    move_vm,
                    gas_charger,
                    protocol_config,
                    metrics,
                    trace_builder_opt,
                )
                .map_err(|e| (e, vec![]))?;
                Ok((Mode::empty_results(), vec![]))
            }
            TransactionKind::RandomnessStateUpdate(randomness_state_update) => {
                setup_randomness_state_update::<Mode>(
                    randomness_state_update,
                    temporary_store,
                    store,
                    tx_ctx,
                    move_vm,
                    gas_charger,
                    protocol_config,
                    metrics,
                    trace_builder_opt,
                )
                .map_err(|e| (e, vec![]))?;
                Ok((Mode::empty_results(), vec![]))
            }
        }?;
        temporary_store
            .check_execution_results_consistency::<Mode>()
            .map_err(|e| (e, vec![]))?;
        Ok(result)
    }

    fn mint_epoch_rewards_in_pt(
        builder: &mut ProgrammableTransactionBuilder,
        params: &AdvanceEpochParams,
    ) -> (Argument, Argument) {
        // Create storage rewards.
        let storage_charge_arg = builder
            .input(CallArg::Pure(
                bcs::to_bytes(&params.storage_charge).unwrap(),
            ))
            .unwrap();
        let storage_rewards = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            BALANCE_MODULE_NAME.to_owned(),
            BALANCE_CREATE_REWARDS_FUNCTION_NAME.to_owned(),
            vec![GAS::type_tag()],
            vec![storage_charge_arg],
        );

        // Create computation rewards.
        let computation_charge_arg = builder
            .input(CallArg::Pure(
                bcs::to_bytes(&params.computation_charge).unwrap(),
            ))
            .unwrap();
        let computation_rewards = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            BALANCE_MODULE_NAME.to_owned(),
            BALANCE_CREATE_REWARDS_FUNCTION_NAME.to_owned(),
            vec![GAS::type_tag()],
            vec![computation_charge_arg],
        );
        (storage_rewards, computation_rewards)
    }

    pub fn construct_advance_epoch_pt<Mode: ExecutionMode>(
        mut builder: ProgrammableTransactionBuilder,
        params: &AdvanceEpochParams,
    ) -> Result<ProgrammableTransaction, Mode::Error> {
        // Step 1: Create storage and computation rewards.
        let (storage_rewards, computation_rewards) = mint_epoch_rewards_in_pt(&mut builder, params);

        // Step 2: Advance the epoch.
        let mut arguments = vec![storage_rewards, computation_rewards];
        let call_arg_arguments = vec![
            CallArg::SUI_SYSTEM_MUT,
            CallArg::Pure(bcs::to_bytes(&params.epoch).unwrap()),
            CallArg::Pure(bcs::to_bytes(&params.next_protocol_version.as_u64()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&params.storage_rebate).unwrap()),
            CallArg::Pure(bcs::to_bytes(&params.non_refundable_storage_fee).unwrap()),
            CallArg::Pure(bcs::to_bytes(&params.storage_fund_reinvest_rate).unwrap()),
            CallArg::Pure(bcs::to_bytes(&params.reward_slashing_rate).unwrap()),
            CallArg::Pure(bcs::to_bytes(&params.epoch_start_timestamp_ms).unwrap()),
        ]
        .into_iter()
        .map(|a| builder.input(a))
        .collect::<Result<_, _>>();

        assert_invariant!(
            call_arg_arguments.is_ok(),
            "Unable to generate args for advance_epoch transaction!"
        );

        arguments.append(&mut call_arg_arguments.unwrap());

        info!("Call arguments to advance_epoch transaction: {:?}", params);

        let storage_rebates = builder.programmable_move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            ADVANCE_EPOCH_FUNCTION_NAME.to_owned(),
            vec![],
            arguments,
        );

        // Step 3: Destroy the storage rebates.
        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            BALANCE_MODULE_NAME.to_owned(),
            BALANCE_DESTROY_REBATES_FUNCTION_NAME.to_owned(),
            vec![GAS::type_tag()],
            vec![storage_rebates],
        );
        Ok(builder.finish())
    }

    pub fn construct_advance_epoch_safe_mode_pt(
        params: &AdvanceEpochParams,
    ) -> Result<ProgrammableTransaction, ExecutionError> {
        let mut builder = ProgrammableTransactionBuilder::new();
        // Step 1: Create storage and computation rewards.
        let (storage_rewards, computation_rewards) = mint_epoch_rewards_in_pt(&mut builder, params);

        // Step 2: Advance the epoch.
        let mut arguments = vec![storage_rewards, computation_rewards];

        let mut args = vec![
            CallArg::SUI_SYSTEM_MUT,
            CallArg::Pure(bcs::to_bytes(&params.epoch).unwrap()),
            CallArg::Pure(bcs::to_bytes(&params.next_protocol_version.as_u64()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&params.storage_rebate).unwrap()),
            CallArg::Pure(bcs::to_bytes(&params.non_refundable_storage_fee).unwrap()),
        ];

        args.push(CallArg::Pure(
            bcs::to_bytes(&params.epoch_start_timestamp_ms).unwrap(),
        ));

        let call_arg_arguments = args
            .into_iter()
            .map(|a| builder.input(a))
            .collect::<Result<_, _>>();

        assert_invariant!(
            call_arg_arguments.is_ok(),
            "Unable to generate args for advance_epoch transaction!"
        );

        arguments.append(&mut call_arg_arguments.unwrap());

        info!("Call arguments to advance_epoch transaction: {:?}", params);

        builder.programmable_move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME.to_owned(),
            vec![],
            arguments,
        );

        Ok(builder.finish())
    }

    fn advance_epoch<Mode: ExecutionMode>(
        builder: ProgrammableTransactionBuilder,
        change_epoch: ChangeEpoch,
        temporary_store: &mut TemporaryStore<'_>,
        store: &dyn BackingStore,
        tx_ctx: Rc<RefCell<TxContext>>,
        move_vm: &Arc<MoveRuntime>,
        gas_charger: &mut GasCharger,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) -> Result<(), Mode::Error> {
        let params = AdvanceEpochParams {
            epoch: change_epoch.epoch,
            next_protocol_version: change_epoch.protocol_version,
            storage_charge: change_epoch.storage_charge,
            computation_charge: change_epoch.computation_charge,
            storage_rebate: change_epoch.storage_rebate,
            non_refundable_storage_fee: change_epoch.non_refundable_storage_fee,
            storage_fund_reinvest_rate: protocol_config.storage_fund_reinvest_rate(),
            reward_slashing_rate: protocol_config.reward_slashing_rate(),
            epoch_start_timestamp_ms: change_epoch.epoch_start_timestamp_ms,
        };
        let advance_epoch_pt = construct_advance_epoch_pt::<Mode>(builder, &params)?;
        let result = SPT::execute::<execution_mode::System<Mode::Error>>(
            protocol_config,
            metrics.clone(),
            move_vm,
            temporary_store,
            store,
            tx_ctx.clone(),
            gas_charger,
            None,
            advance_epoch_pt,
            trace_builder_opt,
        );

        #[cfg(msim)]
        let result = maybe_modify_result_for(result, change_epoch.epoch);

        if let Err(err) = &result {
            tracing::error!(
                "Failed to execute advance epoch transaction. Switching to safe mode. Error: {:?}. Input objects: {:?}. Tx data: {:?}",
                err.0,
                temporary_store.objects(),
                change_epoch,
            );
            temporary_store.drop_writes();
            // Must reset the storage rebate since we are re-executing.
            gas_charger.reset_storage_cost_and_rebate();

            temporary_store.advance_epoch_safe_mode(&params, protocol_config);
        }

        let new_vm = new_move_runtime(
            all_natives(/* silent */ true, protocol_config),
            protocol_config,
        )
        .expect("Failed to create new MoveRuntime");
        process_system_packages(
            change_epoch,
            temporary_store,
            store,
            tx_ctx,
            &new_vm,
            gas_charger,
            protocol_config,
            metrics,
            trace_builder_opt,
        );
        Ok(())
    }

    fn process_system_packages(
        change_epoch: ChangeEpoch,
        temporary_store: &mut TemporaryStore<'_>,
        store: &dyn BackingStore,
        tx_ctx: Rc<RefCell<TxContext>>,
        move_vm: &MoveRuntime,
        gas_charger: &mut GasCharger,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) {
        let digest = tx_ctx.borrow().digest();
        let binary_config = protocol_config.binary_config(None);
        for (version, modules, dependencies) in change_epoch.system_packages.into_iter() {
            let deserialized_modules: Vec<_> = modules
                .iter()
                .map(|m| CompiledModule::deserialize_with_config(m, &binary_config).unwrap())
                .collect();

            if version == OBJECT_START_VERSION {
                let package_id = deserialized_modules.first().unwrap().address();
                info!("adding new system package {package_id}");

                let publish_pt = {
                    let mut b = ProgrammableTransactionBuilder::new();
                    b.command(Command::Publish(modules, dependencies));
                    b.finish()
                };

                SPT::execute::<execution_mode::System>(
                    protocol_config,
                    metrics.clone(),
                    move_vm,
                    temporary_store,
                    store,
                    tx_ctx.clone(),
                    gas_charger,
                    None,
                    publish_pt,
                    trace_builder_opt,
                )
                .map_err(|(e, _)| e)
                .expect("System Package Publish must succeed");
            } else {
                let mut new_package = Object::new_system_package(
                    &deserialized_modules,
                    version,
                    dependencies,
                    digest,
                );

                info!(
                    "upgraded system package {:?}",
                    new_package.compute_object_reference()
                );

                // Decrement the version before writing the package so that the store can record the
                // version growing by one in the effects.
                new_package
                    .data
                    .try_as_package_mut()
                    .unwrap()
                    .decrement_version();

                // upgrade of a previously existing framework module
                temporary_store.upgrade_system_package(new_package);
            }
        }
    }

    /// Perform metadata updates in preparation for the transactions in the upcoming checkpoint:
    ///
    /// - Set the timestamp for the `Clock` shared object from the timestamp in the header from
    ///   consensus.
    fn setup_consensus_commit<Mode: ExecutionMode>(
        consensus_commit_timestamp_ms: CheckpointTimestamp,
        temporary_store: &mut TemporaryStore<'_>,
        store: &dyn BackingStore,
        tx_ctx: Rc<RefCell<TxContext>>,
        move_vm: &Arc<MoveRuntime>,
        gas_charger: &mut GasCharger,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) -> Result<(), Mode::Error> {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            let res = builder.move_call(
                SUI_FRAMEWORK_ADDRESS.into(),
                CLOCK_MODULE_NAME.to_owned(),
                CONSENSUS_COMMIT_PROLOGUE_FUNCTION_NAME.to_owned(),
                vec![],
                vec![
                    CallArg::CLOCK_MUT,
                    CallArg::Pure(bcs::to_bytes(&consensus_commit_timestamp_ms).unwrap()),
                ],
            );
            assert_invariant!(
                res.is_ok(),
                "Unable to generate consensus_commit_prologue transaction!"
            );
            builder.finish()
        };
        SPT::execute::<execution_mode::System<Mode::Error>>(
            protocol_config,
            metrics,
            move_vm,
            temporary_store,
            store,
            tx_ctx,
            gas_charger,
            None,
            pt,
            trace_builder_opt,
        )
        .map_err(|(e, _)| e)?;
        Ok(())
    }

    fn setup_authenticator_state_create(
        mut builder: ProgrammableTransactionBuilder,
    ) -> ProgrammableTransactionBuilder {
        builder
            .move_call(
                SUI_FRAMEWORK_ADDRESS.into(),
                AUTHENTICATOR_STATE_MODULE_NAME.to_owned(),
                AUTHENTICATOR_STATE_CREATE_FUNCTION_NAME.to_owned(),
                vec![],
                vec![],
            )
            .expect("Unable to generate authenticator_state_create transaction!");
        builder
    }

    fn setup_randomness_state_create(
        mut builder: ProgrammableTransactionBuilder,
    ) -> ProgrammableTransactionBuilder {
        builder
            .move_call(
                SUI_FRAMEWORK_ADDRESS.into(),
                RANDOMNESS_MODULE_NAME.to_owned(),
                RANDOMNESS_STATE_CREATE_FUNCTION_NAME.to_owned(),
                vec![],
                vec![],
            )
            .expect("Unable to generate randomness_state_create transaction!");
        builder
    }

    fn setup_bridge_create(
        mut builder: ProgrammableTransactionBuilder,
        chain_id: ChainIdentifier,
    ) -> ProgrammableTransactionBuilder {
        let bridge_uid = builder
            .input(CallArg::Pure(UID::new(SUI_BRIDGE_OBJECT_ID).to_bcs_bytes()))
            .expect("Unable to create Bridge object UID!");

        let bridge_chain_id = if chain_id == get_mainnet_chain_identifier() {
            BridgeChainId::SuiMainnet as u8
        } else if chain_id == get_testnet_chain_identifier() {
            BridgeChainId::SuiTestnet as u8
        } else {
            // How do we distinguish devnet from other test envs?
            BridgeChainId::SuiCustom as u8
        };

        let bridge_chain_id = builder.pure(bridge_chain_id).unwrap();
        builder.programmable_move_call(
            BRIDGE_ADDRESS.into(),
            BRIDGE_MODULE_NAME.to_owned(),
            BRIDGE_CREATE_FUNCTION_NAME.to_owned(),
            vec![],
            vec![bridge_uid, bridge_chain_id],
        );
        builder
    }

    fn setup_bridge_committee_update(
        mut builder: ProgrammableTransactionBuilder,
        bridge_shared_version: SequenceNumber,
    ) -> ProgrammableTransactionBuilder {
        let bridge = builder
            .obj(ObjectArg::SharedObject {
                id: SUI_BRIDGE_OBJECT_ID,
                initial_shared_version: bridge_shared_version,
                mutability: sui_types::transaction::SharedObjectMutability::Mutable,
            })
            .expect("Unable to create Bridge object arg!");
        let system_state = builder
            .obj(ObjectArg::SUI_SYSTEM_MUT)
            .expect("Unable to create System State object arg!");

        let voting_power = builder.programmable_move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            ident_str!("validator_voting_powers").to_owned(),
            vec![],
            vec![system_state],
        );

        // Hardcoding min stake participation to 75.00%
        // TODO: We need to set a correct value or make this configurable.
        let min_stake_participation_percentage = builder
            .input(CallArg::Pure(
                bcs::to_bytes(&BRIDGE_COMMITTEE_MINIMAL_VOTING_POWER).unwrap(),
            ))
            .unwrap();

        builder.programmable_move_call(
            BRIDGE_ADDRESS.into(),
            BRIDGE_MODULE_NAME.to_owned(),
            BRIDGE_INIT_COMMITTEE_FUNCTION_NAME.to_owned(),
            vec![],
            vec![bridge, voting_power, min_stake_participation_percentage],
        );
        builder
    }

    fn setup_authenticator_state_update<Mode: ExecutionMode>(
        update: AuthenticatorStateUpdate,
        temporary_store: &mut TemporaryStore<'_>,
        store: &dyn BackingStore,
        tx_ctx: Rc<RefCell<TxContext>>,
        move_vm: &Arc<MoveRuntime>,
        gas_charger: &mut GasCharger,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) -> Result<(), Mode::Error> {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            let res = builder.move_call(
                SUI_FRAMEWORK_ADDRESS.into(),
                AUTHENTICATOR_STATE_MODULE_NAME.to_owned(),
                AUTHENTICATOR_STATE_UPDATE_FUNCTION_NAME.to_owned(),
                vec![],
                vec![
                    CallArg::Object(ObjectArg::SharedObject {
                        id: SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                        initial_shared_version: update.authenticator_obj_initial_shared_version,
                        mutability: sui_types::transaction::SharedObjectMutability::Mutable,
                    }),
                    CallArg::Pure(bcs::to_bytes(&update.new_active_jwks).unwrap()),
                ],
            );
            assert_invariant!(
                res.is_ok(),
                "Unable to generate authenticator_state_update transaction!"
            );
            builder.finish()
        };
        SPT::execute::<execution_mode::System<Mode::Error>>(
            protocol_config,
            metrics,
            move_vm,
            temporary_store,
            store,
            tx_ctx,
            gas_charger,
            None,
            pt,
            trace_builder_opt,
        )
        .map_err(|(e, _)| e)?;
        Ok(())
    }

    fn setup_authenticator_state_expire(
        mut builder: ProgrammableTransactionBuilder,
        expire: AuthenticatorStateExpire,
    ) -> ProgrammableTransactionBuilder {
        builder
            .move_call(
                SUI_FRAMEWORK_ADDRESS.into(),
                AUTHENTICATOR_STATE_MODULE_NAME.to_owned(),
                AUTHENTICATOR_STATE_EXPIRE_JWKS_FUNCTION_NAME.to_owned(),
                vec![],
                vec![
                    CallArg::Object(ObjectArg::SharedObject {
                        id: SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                        initial_shared_version: expire.authenticator_obj_initial_shared_version,
                        mutability: sui_types::transaction::SharedObjectMutability::Mutable,
                    }),
                    CallArg::Pure(bcs::to_bytes(&expire.min_epoch).unwrap()),
                ],
            )
            .expect("Unable to generate authenticator_state_expire transaction!");
        builder
    }

    fn setup_randomness_state_update<Mode: ExecutionMode>(
        update: RandomnessStateUpdate,
        temporary_store: &mut TemporaryStore<'_>,
        store: &dyn BackingStore,
        tx_ctx: Rc<RefCell<TxContext>>,
        move_vm: &Arc<MoveRuntime>,
        gas_charger: &mut GasCharger,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) -> Result<(), Mode::Error> {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            let res = builder.move_call(
                SUI_FRAMEWORK_ADDRESS.into(),
                RANDOMNESS_MODULE_NAME.to_owned(),
                RANDOMNESS_STATE_UPDATE_FUNCTION_NAME.to_owned(),
                vec![],
                vec![
                    CallArg::Object(ObjectArg::SharedObject {
                        id: SUI_RANDOMNESS_STATE_OBJECT_ID,
                        initial_shared_version: update.randomness_obj_initial_shared_version,
                        mutability: sui_types::transaction::SharedObjectMutability::Mutable,
                    }),
                    CallArg::Pure(bcs::to_bytes(&update.randomness_round).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&update.random_bytes).unwrap()),
                ],
            );
            assert_invariant!(
                res.is_ok(),
                "Unable to generate randomness_state_update transaction!"
            );
            builder.finish()
        };
        SPT::execute::<execution_mode::System<Mode::Error>>(
            protocol_config,
            metrics,
            move_vm,
            temporary_store,
            store,
            tx_ctx,
            gas_charger,
            None,
            pt,
            trace_builder_opt,
        )
        .map_err(|(e, _)| e)?;
        Ok(())
    }

    fn setup_coin_deny_list_state_create(
        mut builder: ProgrammableTransactionBuilder,
    ) -> ProgrammableTransactionBuilder {
        builder
            .move_call(
                SUI_FRAMEWORK_ADDRESS.into(),
                DENY_LIST_MODULE.to_owned(),
                DENY_LIST_CREATE_FUNC.to_owned(),
                vec![],
                vec![],
            )
            .expect("Unable to generate coin_deny_list_create transaction!");
        builder
    }

    fn setup_store_execution_time_estimates(
        mut builder: ProgrammableTransactionBuilder,
        estimates: StoredExecutionTimeObservations,
        chunk_size: usize,
    ) -> ProgrammableTransactionBuilder {
        let system_state = builder.obj(ObjectArg::SUI_SYSTEM_MUT).unwrap();

        let estimate_chunks = estimates.chunk_observations(chunk_size);

        let chunk_bytes: Vec<Vec<u8>> = estimate_chunks
            .into_iter()
            .map(|chunk| bcs::to_bytes(&chunk).unwrap())
            .collect();

        let chunks_arg = builder.pure(chunk_bytes).unwrap();

        builder.programmable_move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            ident_str!("store_execution_time_estimates_v2").to_owned(),
            vec![],
            vec![system_state, chunks_arg],
        );
        builder
    }

    fn setup_accumulator_root_create(
        mut builder: ProgrammableTransactionBuilder,
    ) -> ProgrammableTransactionBuilder {
        builder
            .move_call(
                SUI_FRAMEWORK_ADDRESS.into(),
                ACCUMULATOR_ROOT_MODULE.to_owned(),
                ACCUMULATOR_ROOT_CREATE_FUNC.to_owned(),
                vec![],
                vec![],
            )
            .expect("Unable to generate accumulator_root_create transaction!");
        builder
    }

    fn setup_write_accumulator_storage_cost(
        mut builder: ProgrammableTransactionBuilder,
        write_storage_cost: &WriteAccumulatorStorageCost,
    ) -> ProgrammableTransactionBuilder {
        let system_state = builder.obj(ObjectArg::SUI_SYSTEM_MUT).unwrap();
        let storage_cost_arg = builder.pure(write_storage_cost.storage_cost).unwrap();
        builder.programmable_move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            ident_str!("write_accumulator_storage_cost").to_owned(),
            vec![],
            vec![system_state, storage_cost_arg],
        );
        builder
    }

    fn setup_coin_registry_create(
        mut builder: ProgrammableTransactionBuilder,
    ) -> ProgrammableTransactionBuilder {
        builder
            .move_call(
                SUI_FRAMEWORK_ADDRESS.into(),
                ident_str!("coin_registry").to_owned(),
                ident_str!("create").to_owned(),
                vec![],
                vec![],
            )
            .expect("Unable to generate coin_registry_create transaction!");
        builder
    }

    fn setup_display_registry_create(
        mut builder: ProgrammableTransactionBuilder,
    ) -> ProgrammableTransactionBuilder {
        builder
            .move_call(
                SUI_FRAMEWORK_ADDRESS.into(),
                ident_str!("display_registry").to_owned(),
                ident_str!("create").to_owned(),
                vec![],
                vec![],
            )
            .expect("Unable to generate display_registry_create transaction!");
        builder
    }

    fn setup_address_alias_state_create(
        mut builder: ProgrammableTransactionBuilder,
    ) -> ProgrammableTransactionBuilder {
        builder
            .move_call(
                SUI_FRAMEWORK_ADDRESS.into(),
                ident_str!("address_alias").to_owned(),
                ident_str!("create").to_owned(),
                vec![],
                vec![],
            )
            .expect("Unable to generate address_alias_state_create transaction!");
        builder
    }
    fn setup_forwarding_address_registry_create(
        mut builder: ProgrammableTransactionBuilder,
    ) -> ProgrammableTransactionBuilder {
        builder
            .move_call(
                SUI_FRAMEWORK_ADDRESS.into(),
                ident_str!("forwarding_address").to_owned(),
                ident_str!("create").to_owned(),
                vec![],
                vec![],
            )
            .expect("Unable to generate forwarding_address_registry_create transaction!");
        builder
    }
}
