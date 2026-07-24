// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex, Weak};

use mysten_common::{debug_fatal, fatal, random::get_rng};
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use rand::Rng;
use sui_macros::fail_point_async;
use sui_types::execution::ExecutionOutput;
use sui_types::transaction::TransactionDataAPI;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc::UnboundedReceiver, oneshot};
use tracing::{Instrument, error_span, info, trace, warn};

use crate::authority::AuthorityState;
use crate::execution_scheduler::PendingCertificate;

#[cfg(test)]
#[path = "unit_tests/execution_driver_tests.rs"]
mod execution_driver_tests;

const QUEUEING_DELAY_SAMPLING_RATIO: f64 = 0.05;

pub(crate) struct ExecutionPermit {
    semaphore: Arc<Semaphore>,
    permit: Mutex<Option<OwnedSemaphorePermit>>,
}

impl ExecutionPermit {
    pub(crate) fn new(semaphore: Arc<Semaphore>) -> Self {
        Self {
            semaphore,
            permit: Mutex::new(None),
        }
    }

    pub(crate) fn store(&self, permit: OwnedSemaphorePermit) {
        let prev = self.permit.lock().unwrap().replace(permit);
        assert!(prev.is_none(), "execution permit stored twice");
    }
}

tokio::task_local! {
    /// This is used to pass down the permit to the execution task, so that when a transaction is blocked,
    /// the permit can be released and reacquired when the transaction is unblocked.
    /// This ensures that blocked transactions never starve other transactions.
    pub(crate) static EXECUTION_PERMIT: Arc<ExecutionPermit>;
}

pub(crate) struct ReleasedExecutionPermit(Arc<ExecutionPermit>);

impl ReleasedExecutionPermit {
    pub(crate) async fn reacquire(self) {
        let permit = self.0.semaphore.clone().acquire_owned().await.unwrap();
        self.0.store(permit);
    }
}

pub(crate) fn release_execution_permit_for_wait() -> Option<ReleasedExecutionPermit> {
    EXECUTION_PERMIT
        .try_with(|slot| {
            // Drop the permit by discarding the result.
            if slot.permit.lock().unwrap().take().is_some() {
                Some(ReleasedExecutionPermit(slot.clone()))
            } else {
                debug_fatal!("execution permit slot is empty at release");
                None
            }
        })
        // The EXECUTION_PERMIT is only available in the scope if the task is scheduled from execution driver.
        // In other cases such as dry-run/simulate, this is expected to be None.
        .unwrap_or(None)
}

/// When a notification that a new pending transaction is received we activate
/// processing the transaction in a loop.
pub async fn execution_process(
    authority_state: Weak<AuthorityState>,
    mut rx_ready_certificates: UnboundedReceiver<PendingCertificate>,
    mut rx_execution_shutdown: oneshot::Receiver<()>,
) {
    info!("Starting pending certificates execution process.");

    // Rate limit concurrent executions to # of cpus.
    let normal_limit = Arc::new(Semaphore::new(num_cpus::get()));
    // This is an optimization to speed up the execution of transactions that mutate an implicitly-read system object.
    // These transactions help unblock other transactions that read the same object.
    // Give them a dedicated permit pool so they execute as fast as possible.
    let system_object_writer_limit = Arc::new(Semaphore::new(num_cpus::get()));

    // Loop whenever there is a signal that a new transactions is ready to process.
    loop {
        let _scope = monitored_scope("ExecutionDriver::loop");

        let certificate;
        let execution_env;
        let txn_ready_time;
        let executing_guard;
        tokio::select! {
            result = rx_ready_certificates.recv() => {
                if let Some(pending_cert) = result {
                    certificate = pending_cert.certificate;
                    execution_env = pending_cert.execution_env;
                    txn_ready_time = pending_cert.stats.ready_time.unwrap();
                    executing_guard = pending_cert.executing_guard;
                } else {
                    // Should only happen after the AuthorityState has shut down and tx_ready_certificate
                    // has been dropped by ExecutionScheduler.
                    info!("No more certificate will be received. Exiting executor ...");
                    return;
                };
            }
            _ = &mut rx_execution_shutdown => {
                info!("Shutdown signal received. Exiting executor ...");
                return;
            }
        };

        let authority = if let Some(authority) = authority_state.upgrade() {
            authority
        } else {
            // Terminate the execution if authority has already shutdown, even if there can be more
            // items in rx_ready_certificates.
            info!("Authority state has shutdown. Exiting ...");
            return;
        };
        authority.metrics.execution_driver_dispatch_queue.dec();

        // TODO: Ideally execution_driver should own a copy of epoch store and recreate each epoch.
        let epoch_store = authority.load_epoch_store_one_call_per_task();

        let digest = *certificate.digest();
        trace!(?digest, "Pending certificate execution activated.");

        if epoch_store.epoch() != certificate.epoch() {
            info!(
                ?digest,
                cur_epoch = epoch_store.epoch(),
                cert_epoch = certificate.epoch(),
                "Ignoring certificate from previous epoch."
            );
            continue;
        }

        let limit = if certificate
            .data()
            .transaction_data()
            .kind()
            .mutates_implicitly_read_system_object()
        {
            system_object_writer_limit.clone()
        } else {
            normal_limit.clone()
        };

        // Certificate execution can take significant time, so run it in a separate task.
        let epoch_store_clone = epoch_store.clone();
        let execution_permit = Arc::new(ExecutionPermit::new(limit.clone()));
        spawn_monitored_task!(epoch_store.within_alive_epoch(EXECUTION_PERMIT.scope(execution_permit.clone(), async move {
            let _scope = monitored_scope("ExecutionDriver::task");
            let _executing_guard = executing_guard;
            // Hold semaphore permit until task completes. Unwrap is ok because we never close
            // the semaphore in this context.
            execution_permit.store(limit.acquire_owned().await.unwrap());

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

            if authority.is_tx_already_executed(&digest) {
                return;
            }

            fail_point_async!("transaction_execution_delay");

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
                    // Transaction will be retried later and auto-rescheduled, so we ignore it here
                    authority
                        .metrics
                        .execution_driver_paused_transactions
                        .inc();
                }
            }
        }).instrument(error_span!("execution_driver", tx_digest = ?digest))));
    }
}
