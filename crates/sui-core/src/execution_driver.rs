// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The execution driver receives ready transactions from the ExecutionScheduler and
//! runs them on blocking threads, admitting them in an order that makes it safe for
//! execution to block waiting on values produced by other transactions.
//!
//! Execution can park its thread mid-transaction waiting for a value another
//! transaction's execution will produce (the blocking primitives in
//! `mysten_common::sync`). Declared inputs never cause this - the scheduler sends a
//! transaction only once they are available; blocking happens only for *undeclared*
//! dependencies on system objects (the clock, the accumulator root). With bounded
//! concurrency this could deadlock: every slot parked on a value whose writer was
//! never admitted.
//!
//! Admission prevents this using the causal index assigned to each unit in enqueue
//! order (see `execution_scheduler::causal_order`); enqueue order is causal, so
//! anything a transaction can wait for is produced by a unit with a *lower* index.
//! Let C be the watermark below which every index is done (executed, or dropped as
//! stale). A transaction with index `i` is admitted when
//!
//! ```text
//!     in_flight < K  OR  i == C + 1
//! ```
//!
//! with at most one over-limit (causal-next) admission outstanding at a time,
//! bounding in-flight at K+1. The C+1 transaction can never block - everything below
//! it is done - so it always runs to completion, C advances, and the next
//! transaction inherits the same guarantee: progress is assured no matter how many
//! admitted transactions are parked. Settlement transactions carry no index and are
//! admitted unconditionally: they can never block (they are built from their batch's
//! already-available effects), and running them is what unblocks their waiters. Any
//! transaction admitted unconditionally must be unable to block.
//!
//! Execution occupies at most K+1 threads of the shared tokio blocking pool (whose
//! exhaustion by other subsystems would halt much of the node regardless). Parked
//! transactions hold their concurrency slot; if that ever limits throughput, the
//! escalation path is releasing the slot on park - see the deleted
//! `mysten_common::sync::execution_permit` for the prior mechanism.

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
use crate::execution_scheduler::PendingCertificate;
use crate::execution_scheduler::causal_order::{CausalAdmission, InFlightSlot};

#[cfg(test)]
#[path = "unit_tests/execution_driver_tests.rs"]
mod execution_driver_tests;

const QUEUEING_DELAY_SAMPLING_RATIO: f64 = 0.05;

/// Heap entry ordering pending transactions by causal index (min-heap via `Reverse`).
/// Index-less transactions (settlements, admitted unconditionally) sort first.
struct QueuedCertificate(PendingCertificate);

impl QueuedCertificate {
    fn index(&self) -> Option<u64> {
        self.0.execution_env.causal_index
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

/// Waits until the transaction at the head of the heap is admissible, then pops it,
/// returning it with its concurrency slot (None for unconditionally admitted
/// settlement transactions).
///
/// Cancellation-safe: nothing is popped or admitted until the poll that completes.
async fn pop(
    waiting: &mut BinaryHeap<Reverse<QueuedCertificate>>,
    causal_admission: &Arc<CausalAdmission>,
) -> (PendingCertificate, Option<InFlightSlot>) {
    loop {
        match waiting.peek().map(|Reverse(head)| head.index()) {
            // Settlement transactions are admitted unconditionally: they never block
            // (they are built from their batch's already-available effects), and
            // running them is what unblocks transactions waiting on the accumulator
            // versions they write.
            Some(None) => {
                let cert = waiting.pop().unwrap().0.0;
                debug_assert!(
                    cert.certificate
                        .transaction_data()
                        .kind()
                        .is_accumulator_settle_tx(),
                    "only settlement transactions may bypass causal admission"
                );
                return (cert, None);
            }
            Some(Some(index)) => {
                if let Some(slot) = causal_admission.try_admit(index) {
                    return (waiting.pop().unwrap().0.0, Some(slot));
                }
            }
            None => {}
        }
        // The heap is empty or its head is not admissible; wait for the watermark to
        // advance or a concurrency slot to free.
        causal_admission.changed().await;
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

    loop {
        let _scope = monitored_scope("ExecutionDriver::loop");

        // Get the next admissible transaction, buffering arrivals meanwhile.
        let (pending_cert, mut slot) = loop {
            tokio::select! {
                received = rx_ready_certificates.recv() => {
                    let Some(pending_cert) = received else {
                        // Should only happen after the AuthorityState has shut down and tx_ready_certificate
                        // has been dropped by ExecutionScheduler.
                        info!("No more certificate will be received. Exiting executor ...");
                        return;
                    };
                    if let Some(authority) = authority_state.upgrade() {
                        authority.metrics.execution_driver_dispatch_queue.dec();
                    }
                    waiting.push(Reverse(QueuedCertificate(pending_cert)));
                }
                next = pop(&mut waiting, &causal_admission) => break next,
                _ = &mut rx_execution_shutdown => {
                    info!("Shutdown signal received. Exiting executor ...");
                    return;
                }
            }
        };
        let certificate = pending_cert.certificate;
        let execution_env = pending_cert.execution_env;
        let txn_ready_time = pending_cert.stats.ready_time.unwrap();
        let _executing_guard = pending_cert.executing_guard;

        let authority = if let Some(authority) = authority_state.upgrade() {
            authority
        } else {
            // Terminate the execution if authority has already shutdown, even if there can be more
            // items in rx_ready_certificates.
            info!("Authority state has shutdown. Exiting ...");
            return;
        };

        // TODO: Ideally execution_driver should own a copy of epoch store and recreate each epoch.
        let epoch_store = authority.load_epoch_store_one_call_per_task();

        let digest = *certificate.digest();
        trace!(?digest, "Pending certificate execution activated.");

        if epoch_store.epoch() != certificate.epoch() {
            // With enqueue deduplication, and every transaction committed in an epoch
            // executing before the epoch closes, this should be impossible. Dropping
            // the slot retires the causal index.
            debug_fatal!(
                "certificate from epoch {} in the execution driver at epoch {}",
                certificate.epoch(),
                epoch_store.epoch()
            );
            continue;
        }

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
        let epoch_store_clone = epoch_store.clone();
        let execution_span = error_span!("execution_driver", tx_digest = ?digest);
        // spawn_blocking runs on a thread that does not inherit the current tracing span,
        // so re-enter the span inside the blocking closure to keep execution logs attributed.
        let blocking_span = execution_span.clone();
        spawn_monitored_task!(async move {
            let _scope = monitored_scope("ExecutionDriver::task");

            // Hold the epoch-alive guard across execution so that `epoch_terminated()` waits
            // for in-flight execution to finish. Skip if the epoch has already ended; the
            // slot drop retires the causal index, and the transaction is re-enqueued (and
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
            })
            .await
            .expect("transaction execution task panicked");
        }.instrument(execution_span));
    }
}
