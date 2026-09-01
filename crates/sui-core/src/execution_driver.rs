// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The execution driver receives ready transactions from the ExecutionScheduler and
//! runs them on a dedicated thread pool, admitting them in an order that makes it safe
//! for execution to block waiting on values produced by other transactions.
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
//! updates, settlement batches) take their index at key-enqueue time, so this stays
//! true for them as well.
//!
//! Let C be the watermark: the highest index such that every index at or below it is
//! *done* (finished executing, or retired because the unit was dropped without
//! executing - every drop path retires its index via `CausalIndexGuard`).
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
//! ## Why this cannot deadlock
//!
//! The transaction with index C+1 can always run to completion: everything below it is
//! done, so every value it could wait for - declared or undeclared - already exists.
//! It is admitted even when the concurrency limit is exhausted by parked transactions,
//! and a pool thread is always free for it (the pool is one thread larger than
//! in-flight can ever get, so admission never queues behind parked threads). When it
//! finishes, C advances and the next transaction gains the same guarantee. So the
//! system always makes progress, one transaction at a time in the worst case, no
//! matter how many admitted transactions are parked.
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
//! a throughput problem, releasing the slot on park (the blocking primitives have the
//! hook) is the escalation path.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::{Arc, Weak};

use mysten_common::thread_pool::DedicatedThreadPool;
use mysten_common::{fatal, random::get_rng};
use mysten_metrics::monitored_scope;
use rand::Rng;
use sui_types::execution::ExecutionOutput;
use tokio::sync::{mpsc::UnboundedReceiver, oneshot};
use tracing::{error_span, info, trace, warn};

use crate::authority::AuthorityState;
use crate::execution_scheduler::PendingCertificate;
use crate::execution_scheduler::causal_order::CausalAdmission;

#[cfg(test)]
#[path = "unit_tests/execution_driver_tests.rs"]
mod execution_driver_tests;

const QUEUEING_DELAY_SAMPLING_RATIO: f64 = 0.05;

/// Heap entry ordering pending transactions by causal index (min-heap via `Reverse`).
struct QueuedCertificate {
    index: u64,
    cert: PendingCertificate,
}

impl QueuedCertificate {
    fn new(cert: PendingCertificate) -> Self {
        let index = cert
            .execution_env
            .causal_guard
            .as_ref()
            .expect("causal index is assigned at enqueue")
            .index();
        Self { index, cert }
    }

    fn index(&self) -> u64 {
        self.index
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

    let pool = DedicatedThreadPool::new("sui-execution", causal_admission.required_pool_size());
    // Transactions that have arrived but are not yet admitted, ordered by causal index.
    let mut waiting: BinaryHeap<Reverse<QueuedCertificate>> = BinaryHeap::new();

    loop {
        let _scope = monitored_scope("ExecutionDriver::loop");

        tokio::select! {
            result = rx_ready_certificates.recv() => {
                if let Some(pending_cert) = result {
                    if let Some(authority) = authority_state.upgrade() {
                        authority.metrics.execution_driver_dispatch_queue.dec();
                    }
                    waiting.push(Reverse(QueuedCertificate::new(pending_cert)));
                } else {
                    // Should only happen after the AuthorityState has shut down and tx_ready_certificate
                    // has been dropped by ExecutionScheduler.
                    info!("No more certificate will be received. Exiting executor ...");
                    return;
                };
            }
            // The watermark advanced or a concurrency slot freed; re-check admission.
            _ = causal_admission.changed() => {}
            _ = &mut rx_execution_shutdown => {
                info!("Shutdown signal received. Exiting executor ...");
                return;
            }
        };

        // Drain any further arrivals so admission sees the full picture.
        while let Ok(pending_cert) = rx_ready_certificates.try_recv() {
            if let Some(authority) = authority_state.upgrade() {
                authority.metrics.execution_driver_dispatch_queue.dec();
            }
            waiting.push(Reverse(QueuedCertificate::new(pending_cert)));
        }

        // Admit in causal-index order for as long as the admission rule allows.
        while !waiting.is_empty() {
            let (head_index, cert_epoch, digest) = {
                let head = &waiting.peek().unwrap().0;
                (
                    head.index(),
                    head.cert.certificate.epoch(),
                    *head.cert.certificate.digest(),
                )
            };

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

            // Drop (and thereby retire) transactions that no longer need to run, without
            // consuming an execution slot.
            if epoch_store.epoch() != cert_epoch {
                info!(
                    ?digest,
                    cur_epoch = epoch_store.epoch(),
                    cert_epoch,
                    "Ignoring certificate from previous epoch."
                );
                waiting.pop();
                continue;
            }
            if authority.is_tx_already_executed(&digest) {
                waiting.pop();
                continue;
            }

            let Some(slot) = causal_admission.try_admit(head_index) else {
                break;
            };

            let pending_cert = waiting.pop().unwrap().0.cert;
            let certificate = pending_cert.certificate;
            let execution_env = pending_cert.execution_env;
            let _executing_guard = pending_cert.executing_guard;
            let txn_ready_time = pending_cert.stats.ready_time.unwrap();

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

            // Hold an epoch-alive guard across execution so that `epoch_terminated()` waits
            // for in-flight execution to finish. Awaiting inline stalls admission during
            // reconfiguration, which is intended.
            let Some(alive_guard) = epoch_store.enter_alive_epoch_owned().await else {
                info!(
                    ?digest,
                    "Epoch ended before execution could start; transaction will be retried in the next epoch"
                );
                continue;
            };

            // Certificate execution is CPU-bound, can take significant time, and may park
            // the thread waiting on another transaction's output, so it runs on the
            // dedicated pool. The admission rule guarantees a free thread.
            let epoch_store = epoch_store.clone();
            let execution_span = error_span!("execution_driver", tx_digest = ?digest);
            pool.spawn(move || {
                // Released on completion (or on drop at any early exit), freeing the
                // concurrency slot and waking the driver loop.
                let _slot = slot;
                let _alive_guard = alive_guard;
                let _executing_guard = _executing_guard;
                // The pool thread does not inherit the current tracing span, so re-enter
                // it to keep execution logs attributed.
                let _enter = execution_span.enter();
                let _scope = monitored_scope("ExecutionDriver::blocking_task");
                // `execution_env` carries the causal guard; consuming the env here
                // marks the transaction's causal index done at the end of execution,
                // unless a retry path cloned the env to keep the index alive.
                match authority.try_execute_immediately(&certificate, execution_env, &epoch_store) {
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
                        // Transaction will be retried later and auto-rescheduled (keeping
                        // its causal index via the retry's env clone), so we ignore it here.
                        authority
                            .metrics
                            .execution_driver_paused_transactions
                            .inc();
                    }
                }
            });
        }
    }
}
