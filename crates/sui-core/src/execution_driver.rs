// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use futures::stream::StreamExt;
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use sui_types::{
    digests::TransactionEffectsDigest, executable_transaction::VerifiedExecutableTransaction,
    messages::MultiTxBatch,
};
use tokio::{
    sync::{mpsc::UnboundedReceiver, oneshot, Semaphore},
    time::sleep,
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{debug, error, error_span, info, trace, Instrument};

use crate::authority::AuthorityState;

#[cfg(test)]
#[path = "unit_tests/execution_driver_tests.rs"]
mod execution_driver_tests;

// Execution should not encounter permanent failures, so any failure can and needs
// to be retried.
pub const EXECUTION_MAX_ATTEMPTS: u32 = 10;
const EXECUTION_FAILURE_RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// When a notification that a new pending transaction is received we activate
/// processing the transaction in a loop.
pub async fn execution_process(
    authority_state: Weak<AuthorityState>,
    rx_ready_certificates: UnboundedReceiver<(
        VerifiedExecutableTransaction,
        Option<TransactionEffectsDigest>,
    )>,
    mut rx_execution_shutdown: oneshot::Receiver<()>,
) {
    info!("Starting pending certificates execution process.");

    // Rate limit concurrent executions to # of cpus.
    let limit = Arc::new(Semaphore::new(num_cpus::get()));

    let mut batch_size = 0;
    let mut rx_ready_certificates =
        UnboundedReceiverStream::new(rx_ready_certificates).ready_chunks(100);

    // Loop whenever there is a signal that a new transactions is ready to process.
    loop {
        let _scope = monitored_scope("ExecutionDriver::loop");

        let certificates: Vec<_>;
        tokio::select! {
            result = rx_ready_certificates.next() => {
                if let Some(certs) = result {
                    certificates = certs.into_iter().collect();
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

        let limit = limit.clone();
        // hold semaphore permit until task completes. unwrap ok because we never close
        // the semaphore in this context.
        let permit = limit.acquire_owned().await.unwrap();

        let (certificates, expected_effects_digests): (Vec<_>, Vec<_>) =
            certificates.into_iter().unzip();

        let digests: Vec<_> = certificates.iter().map(|t| t.digest()).collect();
        let batch_id = certificates.batch_id();

        batch_size = certificates.len();
        debug!(
            ?digests,
            ?batch_id,
            "Executing certificate batch of size {}",
            batch_size,
        );

        // Certificate execution can take significant time, so run it in a separate task.
        spawn_monitored_task!(async move {
            let _scope = monitored_scope("ExecutionDriver::task");
            let _guard = permit;
            let mut attempts = 0;

            let res = authority
                .try_execute_immediately(
                    certificates.clone(),
                    expected_effects_digests.clone(),
                    &epoch_store,
                )
                .await;

            if let Err(e) = res {
                error!("Failed to execute transaction batch, attempting one-by-one execution: {e}");

                let mut certificates = certificates.into_iter();
                let mut expected_effects_digests = expected_effects_digests.into_iter();
                loop {
                    attempts += 1;

                    let (Some(certificate), Some(expected_effects_digest)) = (certificates.next(), expected_effects_digests.next()) else {
                        break;
                    };

                    let digest = *certificate.digest();

                    let res = authority
                        .try_execute_immediately(
                            vec![certificate],
                            vec![expected_effects_digest],
                            &epoch_store,
                        )
                        .await;
                    if let Err(e) = res {
                        if attempts == EXECUTION_MAX_ATTEMPTS {
                            panic!("Failed to execute transaction {digest:?} after {attempts} attempts! error={e}");
                        }
                        // Assume only transient failure can happen. Permanent failure is probably
                        // a bug. There is nothing that can be done to recover from permanent failures.
                        error!("Failed to execute transaction {digest:?}! attempt {attempts}, {e}");
                        sleep(EXECUTION_FAILURE_RETRY_INTERVAL).await;
                    } else {
                        authority
                            .metrics
                            .execution_driver_executed_transactions
                            .inc();
                        // reset attempts for next cert
                        attempts = 0;
                    }
                }
            } else {
                authority
                    .metrics
                    .execution_driver_executed_transactions
                    .inc_by(certificates.len() as u64);
            }
        }.instrument(error_span!("execution_driver", ?batch_id)));
    }
}
