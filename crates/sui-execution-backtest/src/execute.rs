// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Re-execution of a single historical transaction against reconstructed checkpoint state, and the
//! per-checkpoint tally a worker returns. The pipeline (see [`crate::handler`]) drives this stage
//! per transaction on a blocking worker.

use std::collections::BTreeMap;

use move_core_types::language_storage::TypeTag;
use sui_types::accumulator_root::AccumulatorValue;
use sui_types::balance::Balance;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::coin_reservation::ParsedObjectRefWithdrawal;
use sui_types::digests::{ChainIdentifier, TransactionDigest};
use sui_types::effects::{InputConsensusObject, TransactionEffects, TransactionEffectsAPI};
use sui_types::error::ExecutionError;
use sui_types::execution_params::ExecutionOrEarlyError;
use sui_types::execution_status::{ExecutionErrorKind, ExecutionStatus};
use sui_types::full_checkpoint_content::ExecutedTransaction;
use sui_types::gas::SuiGasStatus;
use sui_types::gas_coin::GAS;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use sui_types::transaction::{
    CallArg, CheckedInputObjects, Command, FundsWithdrawalArg, GasData, ObjectArg, TransactionData,
    TransactionDataAPI, TransactionKind,
};
use tracing::error;

use crate::StatusFilter;
use crate::context::{EpochCtx, PreparedCheckpoint};
use crate::rows::DivergenceRow;
use crate::store::{ScanStore, resolve_input_objects};

/// Per-checkpoint tally returned by a worker; merged by the sequential collector.
#[derive(Default)]
pub(crate) struct CheckpointStats {
    pub(crate) checked: u64,
    /// Transactions we could not faithfully reconstruct/replay (input resolution failure, gas-status
    /// build failure, or a panicked pipeline) — counted, not reported as divergences.
    pub(crate) reconstruction_errors: u64,
    pub(crate) coin_reservation_skipped: u64,
    /// Transactions skipped because they can't be faithfully executed (price-0, non-gasless).
    pub(crate) execute_skipped: u64,
    /// Transactions with empty gas payment (gas paid from address balance via the
    /// accumulator/withdrawal mechanism — state we don't reconstruct). Counted, not skipped (yet).
    pub(crate) gas_from_balance: u64,
    /// Transactions that ran to completion (got effects, no panic).
    pub(crate) executed: u64,
    /// Executed txns whose recomputed success/failure status disagrees with the on-chain status —
    /// an execution-divergence signal. Each is written to the output (see `records`).
    pub(crate) divergences: u64,
    /// Txns whose on-chain failure is a consensus-layer cancellation (shared-object congestion /
    /// randomness unavailable) and which therefore "succeed" in single-tx replay. Excluded from
    /// `divergences` since they're not reproducible.
    pub(crate) cancellation_excluded: u64,
    /// One typed row per divergent transaction.
    pub(crate) records: Vec<DivergenceRow>,
}

impl CheckpointStats {
    /// Fold another worker's tally into this one: numeric counters are summed and `records` are
    /// moved in (the collector drains them to the output after each merge). Destructured so adding
    /// a counter field is a compile error here until it is folded.
    pub(crate) fn merge(&mut self, other: CheckpointStats) {
        let CheckpointStats {
            checked,
            reconstruction_errors,
            coin_reservation_skipped,
            execute_skipped,
            gas_from_balance,
            executed,
            divergences,
            cancellation_excluded,
            mut records,
        } = other;
        self.checked += checked;
        self.reconstruction_errors += reconstruction_errors;
        self.coin_reservation_skipped += coin_reservation_skipped;
        self.execute_skipped += execute_skipped;
        self.gas_from_balance += gas_from_balance;
        self.executed += executed;
        self.divergences += divergences;
        self.cancellation_excluded += cancellation_excluded;
        self.records.append(&mut records);
    }
}

/// On-chain outcome of a transaction, derived from its effects.
struct OnChainStatus {
    /// `"success"` / `"failure"`, recorded verbatim in divergence records.
    status_label: &'static str,
    /// Stringified failure kind when the tx failed on chain (`None` on success).
    failure: Option<String>,
    /// The on-chain failure is a consensus-layer cancellation (shared-object congestion /
    /// randomness unavailable). Such cancellations happen *before* execution, so the transaction
    /// never ran on chain and single-tx replay can't reproduce it; it is skipped up front and
    /// counted under `cancellation_excluded` rather than replayed.
    non_replayable_cancellation: bool,
    is_success: bool,
}

/// How a transaction's execution should be metered — or that it can't be. Carries only the
/// budget/price *decision* (not the built `SuiGasStatus`, which is large); the caller builds the
/// gas status so its `from_balance` tally is recorded even if that build fails.
enum GasPlan {
    /// Price-0 but not the gasless pattern. The gas model asserts `price > 0`, so it can't be
    /// metered faithfully — skip it.
    Skip,
    /// Meter with this budget/price. `from_balance` = gas paid from the address balance (empty
    /// payment), counted for visibility.
    Meter {
        budget: u64,
        price: u64,
        from_balance: bool,
    },
}

/// The fully-prepared, metered inputs for one execution. Bundled so [`run_execution`] takes a small
/// argument list.
struct PreparedTx {
    input_objects: CheckedInputObjects,
    /// The versions of the system (consensus-sequenced) objects this transaction read, taken from
    /// its recorded effects. The executor loads each system object at exactly this version and
    /// treats a system read with no assigned version as an invariant violation, so it must cover
    /// every such object the transaction touched.
    system_object_versions: BTreeMap<ObjectID, SequenceNumber>,
    gas_data: GasData,
    gas_status: SuiGasStatus,
    txn_kind: TransactionKind,
    rewritten_inputs: Option<Vec<bool>>,
    signer: SuiAddress,
    digest: TransactionDigest,
}

/// Execute stage: re-execute a single transaction (by index) of a prepared checkpoint against the
/// shared store, returning its contribution to the checkpoint tally. Operating per-transaction (vs
/// per-checkpoint) keeps the blocking pool load-balanced — a fat transaction can't head-of-line
/// block the others — and all of the per-transaction prep + execution runs here on the blocking
/// worker, off the async runtime.
pub(crate) fn execute_one_transaction(
    prepared: &PreparedCheckpoint,
    store: &ScanStore,
    idx: usize,
    status: StatusFilter,
    task: &str,
) -> CheckpointStats {
    let mut stats = CheckpointStats::default();
    let ctx = &prepared.ctx;
    let objects = &prepared.objects;
    let executed = &prepared.checkpoint.transactions[idx];

    // Apply the status filter. `Success` is the strict differential baseline (only txns that
    // succeeded on-chain can "now fail"); `Failed`/`All` also admit failures, whose original
    // status/failure kind is recorded so the differential can be applied downstream.
    let on_chain = derive_on_chain_status(&executed.effects);
    match status {
        StatusFilter::Success if !on_chain.is_success => return stats,
        StatusFilter::Failed if on_chain.is_success => return stats,
        _ => {}
    }
    if !matches!(
        executed.transaction.kind(),
        TransactionKind::ProgrammableTransaction(_)
    ) {
        return stats;
    }
    // Consensus-cancelled transactions (shared-object congestion / randomness) never executed on
    // chain: their shared inputs carry no live version and aren't materialized in the checkpoint, so
    // single-tx replay can't reproduce them. Bucket them here rather than letting them fail input
    // resolution (a spurious reconstruction error) or surface as a post-execution mismatch.
    if on_chain.non_replayable_cancellation {
        stats.cancellation_excluded += 1;
        return stats;
    }
    let digest = *executed.effects.transaction_digest();

    // Rewrite coin-reservation (address-balance fake-coin) inputs into FundsWithdrawal args, on a
    // cloned TransactionData, so input resolution and execution both see the rewritten inputs. The
    // mask flows to the executor (`rewritten_inputs`). A reservation we can't resolve is skipped +
    // counted.
    let mut txn_data: TransactionData = executed.transaction.clone();
    let rewritten_inputs = match rewrite_coin_reservations(
        prepared.chain_id,
        txn_data.sender(),
        txn_data.kind_mut(),
        objects,
        &executed.effects,
    ) {
        Ok(mask) => mask,
        Err(e) => {
            stats.coin_reservation_skipped += 1;
            error!(%digest, "coin-reservation rewrite failed: {e:#}");
            return stats;
        }
    };
    let TransactionKind::ProgrammableTransaction(pt) = txn_data.kind() else {
        return stats; // checked above
    };
    let pt = pt.clone();

    let input_objects = match resolve_input_objects(&txn_data, &executed.effects, store) {
        Ok(objs) => CheckedInputObjects::new_for_replay(objs),
        Err(e) => {
            stats.reconstruction_errors += 1;
            error!(%digest, "could not resolve inputs: {e:#}");
            return stats;
        }
    };

    // The versions the transaction's system (consensus) objects were sequenced against, recovered
    // from its effects (mirrors the per-transaction map a live node assigns). Cancelled inputs carry
    // no live version and are excluded above, so only mutated/read-only entries remain.
    let system_object_versions = executed
        .effects
        .input_consensus_objects()
        .into_iter()
        .filter_map(|ico| match ico {
            InputConsensusObject::Mutate((id, v, _))
            | InputConsensusObject::ReadOnly((id, v, _)) => Some((id, v)),
            _ => None,
        })
        .collect();

    let gas_data = txn_data.gas_data().clone();
    let signer = txn_data.sender();
    // Execution must be *metered*: unmetered execution routes storage rebate through the 0x5
    // system-state object (see the adapter's `conserve_unmetered_storage_rebate`), which we do not
    // carry for user transactions.
    let gas_status = match plan_gas(&gas_data, ctx) {
        GasPlan::Skip => {
            stats.execute_skipped += 1;
            return stats;
        }
        GasPlan::Meter {
            budget,
            price,
            from_balance,
        } => {
            if from_balance {
                // Gas (and possibly funds) from the address balance via the accumulator; the
                // executor handles this without us reconstructing accumulator state.
                stats.gas_from_balance += 1;
            }
            match SuiGasStatus::new(budget, price, ctx.reference_gas_price, &ctx.protocol_config) {
                Ok(gas_status) => gas_status,
                Err(e) => {
                    stats.reconstruction_errors += 1;
                    error!(%digest, "building gas status: {e}");
                    return stats;
                }
            }
        }
    };

    stats.checked += 1;

    let prepared_tx = PreparedTx {
        input_objects,
        system_object_versions,
        gas_data,
        gas_status,
        txn_kind: TransactionKind::ProgrammableTransaction(pt),
        rewritten_inputs,
        signer,
        digest,
    };
    let (recomputed_ok, result) = run_execution(ctx, store, prepared_tx);

    stats.executed += 1;
    // Divergence: the recomputed success/failure status disagrees with what happened on chain. This
    // captures any execution-behavior change (e.g. a transaction that succeeded on chain now
    // erroring, or vice versa); the recomputed error, if any, is recorded for triage.
    if recomputed_ok != on_chain.is_success {
        stats.divergences += 1;
        let err = result.as_ref().err();
        let recomputed_error = err.map(|e| e.to_string());
        let triage = log_divergence(
            executed,
            objects,
            &digest,
            on_chain.is_success,
            &recomputed_error,
        );
        stats.records.push(DivergenceRow {
            task: task.to_owned(),
            epoch: ctx.epoch as i64,
            checkpoint: prepared.cp as i64,
            tx_digest: digest.to_string(),
            original_status: on_chain.status_label.to_owned(),
            original_failure_kind: on_chain.failure.clone(),
            recomputed_status: if recomputed_ok { "success" } else { "failure" }.to_owned(),
            recomputed_error_kind: err.map(|e| error_kind_name(e.kind())),
            recomputed_error_detail: recomputed_error,
            missing_modified: triage.missing_modified as i64,
            missing_loaded: triage.missing_loaded as i64,
            missing_consensus: triage.missing_consensus as i64,
            digest_mismatches: triage.digest_mismatches as i64,
        });
    }

    stats
}

/// The bare variant name of an `ExecutionErrorKind` (the leading identifier of its `Debug`), so
/// divergence rows can be grouped by error kind without the per-instance fields (abort codes,
/// indices) that would otherwise split each kind into many groups.
fn error_kind_name(kind: &ExecutionErrorKind) -> String {
    let dbg = format!("{kind:?}");
    dbg.split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(&dbg)
        .to_owned()
}

fn derive_on_chain_status(effects: &TransactionEffects) -> OnChainStatus {
    match effects.status() {
        ExecutionStatus::Success => OnChainStatus {
            status_label: "success",
            failure: None,
            non_replayable_cancellation: false,
            is_success: true,
        },
        ExecutionStatus::Failure(f) => OnChainStatus {
            status_label: "failure",
            failure: Some(format!("{:?}", f.error)),
            non_replayable_cancellation: matches!(
                f.error,
                ExecutionErrorKind::ExecutionCancelledDueToSharedObjectCongestion { .. }
                    | ExecutionErrorKind::ExecutionCancelledDueToRandomnessUnavailable
            ),
            is_success: false,
        },
    }
}

/// Rewrite coin-reservation inputs (address-balance backward-compat "fake coin" `ImmOrOwnedObject`
/// refs) into `FundsWithdrawal` args, mirroring the validator's rewrite — but *without* reading the
/// accumulator balance field object (which is often deleted/pruned). The only thing the rewrite
/// needs from that object is the coin type; the amount is in the reservation digest and the owner
/// is the sender. The reservation's (unmasked) id is the deterministic dynamic-field id
/// `derive(accumulator_root, sender, Balance<T>)`, so we recover `T` by re-deriving that id for
/// candidate coin types (every MoveCall type argument in the PTB, plus SUI) and matching.
///
/// Returns the per-input "was rewritten" mask (`None` if nothing was rewritten); errors (→ skip) if
/// a reservation's coin type can't be identified from the candidates.
fn rewrite_coin_reservations(
    chain_id: ChainIdentifier,
    sender: SuiAddress,
    kind: &mut TransactionKind,
    objects: &BTreeMap<ObjectKey, Object>,
    effects: &TransactionEffects,
) -> anyhow::Result<Option<Vec<bool>>> {
    let TransactionKind::ProgrammableTransaction(pt) = kind else {
        return Ok(None);
    };
    if pt.coin_reservation_obj_refs().next().is_none() {
        return Ok(None);
    }
    // Candidate coin types `T` for `Balance<T>`: SUI, every MoveCall type argument, and the type
    // parameters of the object types this tx touches (these protocols' calls are often monomorphic
    // with no type args, but an input/output `Pool<_, USDC>` / `Coin<USDC>` still reveals the coin
    // type). The reservation's id deterministically encodes `Balance<T>`, so we match to recover T.
    let mut candidates: Vec<TypeTag> = vec![GAS::type_tag()];
    for cmd in &pt.commands {
        if let Command::MoveCall(call) = cmd {
            candidates.extend(
                call.type_arguments
                    .iter()
                    .filter_map(|ti| ti.to_type_tag().ok()),
            );
        }
    }
    let touched = effects.modified_at_versions().into_iter().chain(
        effects
            .all_changed_objects()
            .into_iter()
            .map(|(oref, _, _)| (oref.0, oref.1)),
    );
    for (id, ver) in touched {
        if let Some(mo) = objects
            .get(&ObjectKey(id, ver))
            .and_then(|o| o.data.try_as_move())
        {
            for p in mo.type_().type_params() {
                collect_struct_types(&p, &mut candidates);
            }
        }
    }
    let mut mask = Vec::with_capacity(pt.inputs.len());
    for input in pt.inputs.iter_mut() {
        let parsed = match input {
            CallArg::Object(ObjectArg::ImmOrOwnedObject(oref)) => {
                ParsedObjectRefWithdrawal::parse(oref, chain_id)
            }
            _ => None,
        };
        let Some(parsed) = parsed else {
            mask.push(false);
            continue;
        };
        // Identify the coin type by matching the reservation's derived accumulator-field id.
        let withdrawal = candidates.iter().find_map(|coin_ty| {
            let field_id =
                AccumulatorValue::get_field_id(sender, &Balance::type_tag(coin_ty.clone())).ok()?;
            (*field_id.inner() == parsed.unmasked_object_id).then(|| {
                FundsWithdrawalArg::balance_from_sender(
                    parsed.reservation_amount(),
                    coin_ty.clone(),
                )
            })
        });
        match withdrawal {
            Some(w) => {
                *input = CallArg::FundsWithdrawal(w);
                mask.push(true);
            }
            None => anyhow::bail!(
                "could not identify balance type for coin reservation {}",
                parsed.unmasked_object_id
            ),
        }
    }
    Ok(Some(mask))
}

/// Recursively collect every struct `TypeTag` appearing in `tt` (itself + nested type params).
fn collect_struct_types(tt: &TypeTag, out: &mut Vec<TypeTag>) {
    if let TypeTag::Struct(s) = tt {
        out.push(tt.clone());
        for p in &s.type_params {
            collect_struct_types(p, out);
        }
    }
}

/// Decide how to meter, mirroring `sui-transaction-checks::check_gas`: gasless transactions (empty
/// payment + price 0; gas paid from the address balance) are metered at the epoch RGP with the
/// gasless compute cap, not their zero price/budget.
fn plan_gas(gas_data: &GasData, ctx: &EpochCtx) -> GasPlan {
    let gasless = gas_data.price == 0 && gas_data.payment.is_empty();
    if gas_data.price == 0 && !gasless {
        return GasPlan::Skip;
    }
    let from_balance = gas_data.payment.is_empty();
    let (budget, price) = if gasless {
        let r = ctx.reference_gas_price.max(1);
        (
            ctx.protocol_config
                .gasless_max_computation_units()
                .saturating_mul(r),
            r,
        )
    } else {
        (gas_data.budget, gas_data.price)
    };
    GasPlan::Meter {
        budget,
        price,
        from_balance,
    }
}

/// Run the executor, returning `(recomputed_ok, exec_result)`. The epoch id + start timestamp come
/// from `ctx` (the executor wants the epoch start timestamp, constant across the epoch, not the
/// per-checkpoint one).
fn run_execution(
    ctx: &EpochCtx,
    store: &ScanStore,
    prepared: PreparedTx,
) -> (bool, Result<(), ExecutionError>) {
    let PreparedTx {
        input_objects,
        system_object_versions,
        gas_data,
        gas_status,
        txn_kind,
        rewritten_inputs,
        signer,
        digest,
    } = prepared;
    let (_inner, _gas, effects, _timing, exec_res) = ctx
        .executor
        .execute_transaction_to_effects_and_execution_error(
            store,
            &ctx.protocol_config,
            ctx.metrics.clone(),
            /* enable_expensive_checks */ false,
            ExecutionOrEarlyError::ok(None),
            &ctx.epoch,
            ctx.epoch_start_timestamp_ms,
            input_objects,
            system_object_versions,
            gas_data,
            gas_status,
            txn_kind,
            rewritten_inputs,
            signer,
            digest,
            &mut None,
        );
    (effects.status().is_ok(), exec_res)
}

/// Counts of the triage signals attached to a divergence row: how much of the transaction's read
/// set our reconstructed object set was missing or disagreed on. Nonzero values point at a
/// reconstruction gap rather than a genuine execution divergence.
struct TriageCounts {
    missing_modified: usize,
    missing_loaded: usize,
    missing_consensus: usize,
    digest_mismatches: usize,
}

/// Log a divergence with triage diagnostics: which object versions the transaction read that our
/// reconstructed object set is *missing* (a logic gap — a loaded dynamic-field child or consensus
/// input the stream didn't carry, which makes the store serve stale/absent state), and which
/// *present* versions have a mismatched digest (a data inconsistency — the stream gave us different
/// bytes for that version).
fn log_divergence(
    executed: &ExecutedTransaction,
    objects: &BTreeMap<ObjectKey, Object>,
    digest: &TransactionDigest,
    on_chain_success: bool,
    recomputed_error: &Option<String>,
) -> TriageCounts {
    let missing_modified: Vec<String> = executed
        .effects
        .modified_at_versions()
        .into_iter()
        .filter(|(id, v)| !objects.contains_key(&ObjectKey(*id, *v)))
        .map(|(id, v)| format!("{id}@{v}"))
        .collect();
    let missing_loaded: Vec<String> = executed
        .unchanged_loaded_runtime_objects
        .iter()
        .filter(|k| !objects.contains_key(k))
        .map(|k| format!("{}@{}", k.0, k.1))
        .collect();
    let missing_consensus: Vec<String> = executed
        .effects
        .input_consensus_objects()
        .into_iter()
        .filter_map(|ico| match ico {
            InputConsensusObject::Mutate((id, v, _))
            | InputConsensusObject::ReadOnly((id, v, _)) => Some((id, v)),
            _ => None,
        })
        .filter(|(id, v)| !objects.contains_key(&ObjectKey(*id, *v)))
        .map(|(id, v)| format!("{id}@{v}"))
        .collect();
    let digest_mismatches: Vec<String> = executed
        .effects
        .old_object_metadata()
        .into_iter()
        .filter_map(|((id, v, d), _)| {
            objects.get(&ObjectKey(id, v)).and_then(|o| {
                let ours = o.compute_object_reference().2;
                (ours != d).then(|| format!("{id}@{v}"))
            })
        })
        .collect();
    error!(%digest, on_chain_success, recomputed_error = ?recomputed_error,
            ?missing_modified, ?missing_loaded, ?missing_consensus, ?digest_mismatches,
            loaded_children = executed.unchanged_loaded_runtime_objects.len(),
            "execution diverges from on-chain");

    TriageCounts {
        missing_modified: missing_modified.len(),
        missing_loaded: missing_loaded.len(),
        missing_consensus: missing_consensus.len(),
        digest_mismatches: digest_mismatches.len(),
    }
}
