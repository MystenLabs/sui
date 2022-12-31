// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use mysten_metrics::spawn_monitored_task;
use sui_types::messages::VerifiedCertificate;
use tokio::{
    sync::{mpsc::UnboundedReceiver, Semaphore},
    time::sleep,
};
use tracing::{debug, error, info};

use crate::authority::AuthorityState;

#[cfg(test)]
#[path = "unit_tests/execution_driver_tests.rs"]
mod execution_driver_tests;

// Execution should not encounter permanent failures, so any failure can and needs
// to be retried.
const EXECUTION_MAX_ATTEMPTS: usize = 10;
const EXECUTION_FAILURE_RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// When a notification that a new pending transaction is received we activate
/// processing the transaction in a loop.
pub async fn execution_process(
    authority_state: Weak<AuthorityState>,
    mut rx_ready_certificates: UnboundedReceiver<VerifiedCertificate>,
) {
    info!("Starting pending certificates execution process.");

    // Rate limit concurrent executions to # of cpus.
    let limit = Arc::new(Semaphore::new(num_cpus::get()));

    // Loop whenever there is a signal that a new transactions is ready to process.
    loop {
        let certificate = if let Some(cert) = rx_ready_certificates.recv().await {
            cert
        } else {
            // Should only happen after the AuthorityState has shut down and tx_ready_certificate
            // has been dropped by TransactionManager.
            info!("No more certificate will be received. Exiting ...");
            return;
        };
        let authority = if let Some(authority) = authority_state.upgrade() {
            authority
        } else {
            // Terminate the execution if authority has already shutdown, even if there can be more
            // items in rx_ready_certificates.
            info!("Authority state has shutdown. Exiting ...");
            return;
        };

        let epoch_store = authority.epoch_store();

        let digest = *certificate.digest();
        debug!(?digest, "Pending certificate execution activated.");

        // Process any tx that failed to commit.
        if let Err(err) = authority.process_tx_recovery_log(None, &epoch_store).await {
            tracing::error!("Error processing tx recovery log: {:?}", err);
        }

        let limit = limit.clone();
        // hold semaphore permit until task completes. unwrap ok because we never close
        // the semaphore in this context.
        let permit = limit.acquire_owned().await.unwrap();

        // Certificate execution can take significant time, so run it in a separate task.
        spawn_monitored_task!(async move {
            let _guard = permit;
            if let Ok(true) = authority.is_tx_already_executed(&digest) {
                return;
            }
            let mut attempts = 0;
            loop {
                attempts += 1;
                let res = authority
                    .try_execute_immediately(&certificate, &epoch_store)
                    .await;
                if let Err(e) = res {
                    if attempts == EXECUTION_MAX_ATTEMPTS {
                        error!("Failed to execute certified transaction after {attempts} attempts! error={e} certificate={:?}", certificate);
                        authority.metrics.execution_driver_execution_failures.inc();
                        return;
                    }
                    // Assume only transient failure can happen. Permanent failure is probably
                    // a bug. There is nothing that can be done to recover from permanent failures.
                    error!(tx_digest=?digest, "Failed to execute certified transaction! attempt {attempts}, {e}");
                    sleep(EXECUTION_FAILURE_RETRY_INTERVAL).await;
                } else {
                    break;
                }
            }

            // Remove the certificate that finished execution from the pending_certificates table.
            authority.certificate_executed(&digest);

            authority
                .metrics
                .execution_driver_executed_transactions
                .inc();
        });
    }
}
