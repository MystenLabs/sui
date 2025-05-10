// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Weak};

use mysten_common::{fatal, random::get_rng};
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use rand::Rng;
use sui_macros::fail_point_async;
use sui_types::error::SuiError;
use tokio::sync::{mpsc::UnboundedReceiver, oneshot, Semaphore};
use tracing::{error_span, info, trace, warn, Instrument};

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

    // Rate limit concurrent executions to # of cpus.
    let limit = Arc::new(Semaphore::new(num_cpus::get()));

    // Loop whenever there is a signal that a new transactions is ready to process.
    loop {
        let _scope = monitored_scope("ExecutionDriver::loop");

        let certificate;
        let expected_effects_digest;
        let txn_ready_time;
        tokio::select! {
            result = rx_ready_certificates.recv() => {
                if let Some(pending_cert) = result {
                    certificate = pending_cert.certificate;
                    expected_effects_digest = pending_cert.expected_effects_digest;
                    txn_ready_time = pending_cert.stats.ready_time;
                } else {
                    // Should only happen after the AuthorityState has shut down and tx_ready_certificate
                    // has been dropped by TransactionManager.
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

        let limit = limit.clone();
        // hold semaphore permit until task completes. unwrap ok because we never close
        // the semaphore in this context.
        let permit = limit.acquire_owned().await.unwrap();

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

        // Certificate execution can take significant time, so run it in a separate task.
        let epoch_store_clone = epoch_store.clone();
        spawn_monitored_task!(epoch_store.within_alive_epoch(async move {
            let _scope = monitored_scope("ExecutionDriver::task");
            let _guard = permit;
            if authority.is_tx_already_executed(&digest) {
                return;
            }

            fail_point_async!("transaction_execution_delay");

            match authority.try_execute_immediately(
                &certificate,
                expected_effects_digest,
                &epoch_store_clone,
            ).await {
                Err(SuiError::ValidatorHaltedAtEpochEnd) => {
                    warn!("Could not execute transaction {digest:?} because validator is halted at epoch end. certificate={certificate:?}");
                    return;
                }
                Err(e) => {
                    fatal!("Failed to execute certified transaction {digest:?}! error={e} certificate={certificate:?}");
                }
                _ => (),
            }
            authority
                .metrics
                .execution_driver_executed_transactions
                .inc();
        }.instrument(error_span!("execution_driver", tx_digest = ?digest))));
    }
}
