// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Weak};

use mysten_common::sync::execution_permit::set_execution_permit;
use mysten_common::{fatal, random::get_rng};
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use rand::Rng;
use sui_macros::fail_point_async;
use sui_types::execution::ExecutionOutput;
use sui_types::transaction::TransactionDataAPI;
use tokio::sync::{Semaphore, mpsc::UnboundedReceiver, oneshot};
use tracing::{Instrument, error_span, info, trace, warn};

use crate::authority::AuthorityState;
use crate::execution_scheduler::PendingCertificate;

#[cfg(test)]
#[path = "unit_tests/execution_driver_tests.rs"]
mod execution_driver_tests;

const QUEUEING_DELAY_SAMPLING_RATIO: f64 = 0.05;

/// When a notification that a new pending transaction is received we activate
/// processing the transaction in a loop.
pub async fn execution_process(
    authority_state: Weak<AuthorityState>,
    mut rx_ready_certificates: UnboundedReceiver<PendingCertificate>,
    mut rx_execution_shutdown: oneshot::Receiver<()>,
) {
    info!("Starting pending certificates execution process.");

    // Rate limit concurrent executions to half of the available CPUs.
    let execution_concurrency = std::cmp::max(1, num_cpus::get() / 2);
    let normal_limit = Arc::new(Semaphore::new(execution_concurrency));
    // This is an optimization to speed up the execution of transactions that mutate an implicitly-read system object.
    // These transactions help unblock other transactions that read the same object.
    // Give them a dedicated permit pool so they execute as fast as possible.
    let system_object_writer_limit = Arc::new(Semaphore::new(execution_concurrency));

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
            .transaction_data()
            .kind()
            .mutates_implicitly_read_system_object()
        {
            system_object_writer_limit.clone()
        } else {
            normal_limit.clone()
        };

        // Certificate execution is CPU-bound and can take significant time, so run it on a
        // blocking thread to avoid stalling the async runtime's worker threads.
        let epoch_store_clone = epoch_store.clone();
        let execution_span = error_span!("execution_driver", tx_digest = ?digest);
        // spawn_blocking runs on a thread that does not inherit the current tracing span,
        // so re-enter the span inside the blocking closure to keep execution logs attributed.
        let blocking_span = execution_span.clone();
        spawn_monitored_task!(async move {
            let _scope = monitored_scope("ExecutionDriver::task");
            let _executing_guard = executing_guard;

            if authority.is_tx_already_executed(&digest) {
                return;
            }

            // `permit` is moved into the blocking closure below and installed on the
            // execution thread, so that a blocking sync primitive can release it if
            // execution has to wait on work that another execution must perform (which
            // would otherwise deadlock under limited concurrency). Until then it is held
            // by this task, and the early returns below drop it, freeing the slot.
            let permit = {
                // The scope measures time blocked waiting for a permit, i.e. how saturated
                // execution concurrency is.
                let _scope = monitored_scope("ExecutionDriver::acquire_permit");
                // Unwrap is ok because we never close the semaphore in this context.
                limit.acquire_owned().await.unwrap()
            };

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

            fail_point_async!("transaction_execution_delay");

            // Hold the epoch-alive guard across execution so that `epoch_terminated()` waits
            // for in-flight execution to finish (previously guaranteed by `within_alive_epoch`,
            // since execution has no yield points). Skip if the epoch has already ended.
            let Some(_alive_guard) = epoch_store.enter_alive_epoch().await else {
                info!("Epoch ended before execution could start; transaction will be retried in the next epoch");
                return;
            };

            // Await unconditionally: once dispatched, execution always runs to completion
            // within the alive-epoch guard and is never detached at epoch end.
            // TODO: Currently it is possible that enough transactions get blocked waiting during execution,
            // and exhausted all blocking threads, causing the execution to stall. A fix is on the way.
            tokio::task::spawn_blocking(move || {
                // Install the permit so a blocking sync primitive can release it while
                // execution waits; otherwise it is released when this guard drops at the
                // end of execution. Not re-acquired after a release - any transient
                // over-subscription resolves itself as tasks complete.
                let _permit_guard = set_execution_permit(Box::new(permit));
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
                        // Transaction will be retried later and auto-rescheduled, so we ignore it here
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
