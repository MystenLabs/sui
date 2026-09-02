// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The execution driver receives ready transactions from the ExecutionScheduler and
//! runs them on blocking threads, admitting them in an order that makes it safe for
//! execution to block waiting on values produced by other transactions.
//!
//! ## The problem
//!
//! Execution can park its thread mid-transaction to wait for a value that another
//! transaction's execution will produce (the blocking primitives in
//! `mysten_common::sync`). Declared input objects never cause this - the scheduler only
//! sends us a transaction after its declared inputs are available. Blocking happens
//! only for *undeclared* dependencies: system objects such as the clock (written by the
//! consensus commit prologue) and the accumulator root (written by settlement
//! transactions).
//!
//! Blocking makes naive scheduling deadlock-prone: if every execution slot is occupied
//! by transactions parked waiting for a value, and the transaction that would write
//! that value has not been admitted yet, nothing ever finishes.
//!
//! ## Causal order
//!
//! The scheduler assigns every enqueued unit a *causal index* - its position in the
//! order units were enqueued from the consensus handler or checkpoint executor (see
//! `execution_scheduler::causal_order`). Both sources enqueue in causal order, so
//! anything a transaction can wait for - declared or undeclared - is produced by a unit
//! with a *lower* index. Keys that materialize into transactions later (randomness
//! updates) take their index at key-enqueue time, so this stays true for them as well;
//! settlement transactions are the exception and carry no index at all (below).
//!
//! Let C be the watermark: the highest index such that every index at or below it is
//! *done* (finished executing, or dropped here as no longer needed). The driver is the
//! sole point where transactions execute and where causal indices are retired; enqueue
//! deduplication guarantees every assigned index reaches it (see
//! `execution_scheduler::causal_order`).
//!
//! ## The admission rule
//!
//! A transaction with causal index `i` is admitted for execution when:
//!
//! ```text
//!     in_flight < K  OR  i == C + 1
//! ```
//!
//! where `in_flight` counts admitted-but-unfinished transactions (including parked
//! ones) and K is the concurrency limit. The second branch admits at most one
//! transaction over the limit at a time; when it finishes, C advances and the new
//! C+1 transaction may again be admitted over the limit, while the capacity branch
//! stays closed until in-flight drops below K.
//!
//! Settlement transactions carry no index and are admitted unconditionally: they can
//! never block (they are constructed from their batch's already-available effects),
//! and running them immediately is what unblocks transactions waiting on the
//! accumulator versions they write. Any transaction admitted unconditionally must be
//! unable to block - that is what keeps unconditional admission safe.
//!
//! ## Why this cannot deadlock
//!
//! The transaction with index C+1 can always run to completion: everything below it is
//! done, so every value it could wait for - declared or undeclared - already exists.
//! It is admitted even when the concurrency limit is exhausted by parked transactions.
//! When it finishes, C advances and the next transaction gains the same guarantee. So
//! the system always makes progress, one transaction at a time in the worst case, no
//! matter how many admitted transactions are parked.
//!
//! Execution runs on tokio's shared blocking pool and occupies at most K+1 of its
//! threads (in-flight is bounded by the admission rule). We assume the shared pool is
//! not permanently exhausted by other subsystems: its other users run terminating
//! work, and exhausting it would halt much of the node regardless of execution.
//!
//! Transactions parked on an undeclared dependency do not park forever for the same
//! reason: the writer they wait for has a lower index, so the watermark reaches it and
//! it executes.
//!
//! ## Throughput
//!
//! Admission takes waiting transactions in causal-index order, so execution slots
//! always go to the nearest available work first; how far execution may run ahead of a
//! parked or not-yet-materialized unit is deliberately unbounded (the far
//! transactions are committed work that must execute anyway). Parked transactions do
//! hold their concurrency slot; if occupancy by parked transactions ever shows up as
//! a throughput problem, the escalation path is to release the slot when execution
//! parks (the blocking primitives once carried a release hook for this - see
//! mysten-common's deleted `sync::execution_permit`).

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::{Arc, Weak};

use mysten_common::{debug_fatal, fatal, random::get_rng};
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use rand::Rng;
use sui_types::execution::ExecutionOutput;
use sui_types::transaction::TransactionDataAPI;
use tokio::sync::{mpsc::UnboundedReceiver, oneshot};
use tracing::{Instrument, error_span, info, trace, warn};

use crate::authority::AuthorityState;
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::execution_scheduler::PendingCertificate;
use crate::execution_scheduler::causal_order::{CausalAdmission, InFlightSlot};

#[cfg(test)]
#[path = "unit_tests/execution_driver_tests.rs"]
mod execution_driver_tests;

const QUEUEING_DELAY_SAMPLING_RATIO: f64 = 0.05;

/// Max certificates pulled from the ready channel per loop pass; a longer backlog is
/// picked up by the immediately-ready next pass.
const RECV_BATCH_SIZE: usize = 1024;

/// Heap entry ordering pending transactions by causal index (min-heap via `Reverse`).
/// Index-less transactions (settlements, admitted unconditionally) sort first.
struct QueuedCertificate(PendingCertificate);

impl QueuedCertificate {
    fn index(&self) -> Option<u64> {
        self.0.execution_env.causal_index
    }

    /// Retires the causal index of a transaction the driver drops without executing.
    fn retire(self, causal_admission: &CausalAdmission) {
        if let Some(index) = self.index() {
            causal_admission.mark_done(index);
        }
    }
}

impl PartialEq for QueuedCertificate {
    fn eq(&self, other: &Self) -> bool {
        self.index() == other.index()
    }
}
impl Eq for QueuedCertificate {}
impl PartialOrd for QueuedCertificate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for QueuedCertificate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.index().cmp(&other.index())
    }
}

/// When a notification that a new pending transaction is received we activate
/// processing the transaction in a loop.
pub async fn execution_process(
    authority_state: Weak<AuthorityState>,
    mut rx_ready_certificates: UnboundedReceiver<PendingCertificate>,
    mut rx_execution_shutdown: oneshot::Receiver<()>,
    causal_admission: Arc<CausalAdmission>,
) {
    info!("Starting pending certificates execution process.");

    // Transactions that have arrived but are not yet admitted, ordered by causal index.
    let mut waiting: BinaryHeap<Reverse<QueuedCertificate>> = BinaryHeap::new();
    let mut arrivals: Vec<PendingCertificate> = Vec::new();

    loop {
        let _scope = monitored_scope("ExecutionDriver::loop");

        tokio::select! {
            received = rx_ready_certificates.recv_many(&mut arrivals, RECV_BATCH_SIZE) => {
                if received == 0 {
                    // Should only happen after the AuthorityState has shut down and tx_ready_certificate
                    // has been dropped by ExecutionScheduler.
                    info!("No more certificate will be received. Exiting executor ...");
                    return;
                }
            }
            // The watermark advanced or a concurrency slot freed; re-check admission.
            _ = causal_admission.changed() => {}
            _ = &mut rx_execution_shutdown => {
                info!("Shutdown signal received. Exiting executor ...");
                return;
            }
        };

        let Some(authority) = authority_state.upgrade() else {
            // Terminate the execution if authority has already shutdown, even if there can be more
            // items in rx_ready_certificates.
            info!("Authority state has shutdown. Exiting ...");
            return;
        };
        if !arrivals.is_empty() {
            authority
                .metrics
                .execution_driver_dispatch_queue
                .sub(arrivals.len() as i64);
            waiting.extend(arrivals.drain(..).map(|c| Reverse(QueuedCertificate(c))));
        }

        // TODO: Ideally execution_driver should own a copy of epoch store and recreate each epoch.
        let epoch_store = authority.load_epoch_store_one_call_per_task();

        // Dispatch for as long as the admission rule allows; once the head is not
        // admissible, wait above for an arrival, a watermark advance or a freed slot.
        while let Some((pending_cert, slot)) =
            pop_admissible(&mut waiting, &causal_admission, &epoch_store)
        {
            dispatch_certificate(&authority, epoch_store.clone(), pending_cert, slot);
        }
    }
}

/// Pops the next admissible transaction from the heap. Returns the transaction and its
/// concurrency slot (None for unconditionally admitted settlement transactions), or
/// None when the heap is empty or its head is not admissible yet.
fn pop_admissible(
    waiting: &mut BinaryHeap<Reverse<QueuedCertificate>>,
    causal_admission: &Arc<CausalAdmission>,
    epoch_store: &AuthorityPerEpochStore,
) -> Option<(PendingCertificate, Option<InFlightSlot>)> {
    let (head_index, cert_epoch) = {
        let head = &waiting.peek()?.0;
        (head.index(), head.0.certificate.epoch())
    };

    // With enqueue deduplication, and every transaction committed in an epoch
    // executing before the epoch closes, no certificate can still be waiting here
    // when the epoch changes.
    if epoch_store.epoch() != cert_epoch {
        debug_fatal!(
            "certificate from epoch {} in the execution driver at epoch {}",
            cert_epoch,
            epoch_store.epoch()
        );
        waiting.pop().unwrap().0.retire(causal_admission);
        return None;
    }

    let slot = match head_index {
        // Settlement transactions are admitted unconditionally: they never block
        // (they are built from their batch's already-available effects), and
        // running them is what unblocks transactions waiting on the accumulator
        // versions they write.
        None => {
            debug_assert!(
                waiting
                    .peek()
                    .unwrap()
                    .0
                    .0
                    .certificate
                    .transaction_data()
                    .kind()
                    .is_accumulator_settle_tx(),
                "only settlement transactions may bypass causal admission"
            );
            None
        }
        Some(index) => Some(causal_admission.try_admit(index)?),
    };

    Some((waiting.pop().unwrap().0.0, slot))
}

/// Runs one admitted transaction on a blocking thread. Dropping the slot when
/// execution completes releases it and retires the transaction's causal index.
fn dispatch_certificate(
    authority: &Arc<AuthorityState>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    pending_cert: PendingCertificate,
    mut slot: Option<InFlightSlot>,
) {
    let certificate = pending_cert.certificate;
    let execution_env = pending_cert.execution_env;
    let _executing_guard = pending_cert.executing_guard;
    let txn_ready_time = pending_cert.stats.ready_time.unwrap();

    let digest = *certificate.digest();
    trace!(?digest, "Pending certificate execution activated.");

    if get_rng().gen_range(0.0..1.0) < QUEUEING_DELAY_SAMPLING_RATIO {
        authority
            .metrics
            .execution_queueing_latency
            .report(txn_ready_time.elapsed());
        if let Some(latency) = authority.metrics.execution_queueing_latency.latency() {
            authority
                .metrics
                .execution_queueing_delay_s
                .observe(latency.as_secs_f64());
        }
    }

    authority.metrics.execution_rate_tracker.lock().record();

    // Certificate execution is CPU-bound and can take significant time, so run it on a
    // blocking thread to avoid stalling the async runtime's worker threads.
    let authority = authority.clone();
    let epoch_store_clone = epoch_store.clone();
    let execution_span = error_span!("execution_driver", tx_digest = ?digest);
    // spawn_blocking runs on a thread that does not inherit the current tracing span,
    // so re-enter the span inside the blocking closure to keep execution logs attributed.
    let blocking_span = execution_span.clone();
    spawn_monitored_task!(async move {
        let _scope = monitored_scope("ExecutionDriver::task");
        let _executing_guard = _executing_guard;

        // Hold the epoch-alive guard across execution so that `epoch_terminated()` waits
        // for in-flight execution to finish. Skip if the epoch has already ended; the
        // slot drop retires the index, and the transaction is re-enqueued (and
        // re-indexed) in the next epoch.
        let Some(_alive_guard) = epoch_store.enter_alive_epoch().await else {
            info!("Epoch ended before execution could start; transaction will be retried in the next epoch");
            return;
        };

        // Await unconditionally: once dispatched, execution always runs to completion
        // within the alive-epoch guard and is never detached at epoch end.
        tokio::task::spawn_blocking(move || {
            let _enter = blocking_span.enter();
            let _scope = monitored_scope("ExecutionDriver::blocking_task");
            match authority.try_execute_immediately(
                &certificate,
                execution_env,
                &epoch_store_clone,
            ) {
                ExecutionOutput::Success(_) => {
                    authority
                        .metrics
                        .execution_driver_executed_transactions
                        .inc();
                }
                ExecutionOutput::EpochEnded => {
                    warn!("Could not execute transaction {digest:?} because validator is halted at epoch end. certificate={certificate:?}");
                }
                ExecutionOutput::Fatal(e) => {
                    fatal!("Failed to execute certified transaction {digest:?}! error={e} certificate={certificate:?}");
                }
                ExecutionOutput::RetryLater => {
                    // Transaction will be retried later and auto-rescheduled under its
                    // original causal index (carried in the retry's env clone), so keep
                    // the index alive when the slot is released.
                    if let Some(slot) = &mut slot {
                        slot.skip_retire();
                    }
                    authority
                        .metrics
                        .execution_driver_paused_transactions
                        .inc();
                }
            }
            // Dropping the slot releases it and retires the causal index (unless a
            // retry kept it alive), waking the driver once.
            drop(slot);
        })
        .await
        .expect("transaction execution task panicked");
    }.instrument(execution_span));
}
