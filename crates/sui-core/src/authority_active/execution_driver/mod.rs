// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use prometheus::{
    register_int_counter_with_registry, register_int_gauge_with_registry, IntCounter, IntGauge,
    Registry,
};
use sui_metrics::spawn_monitored_task;
use sui_types::error::SuiError;
use tokio::{sync::Semaphore, time::sleep};
use tracing::{debug, error, info, warn};

use super::ActiveAuthority;
use crate::authority_client::AuthorityAPI;

#[cfg(test)]
pub(crate) mod tests;

// Execution should not encounter permanent failures, so any failure can and needs
// to be retried.
const EXECUTION_MAX_ATTEMPTS: usize = 10;
const EXECUTION_FAILURE_RETRY_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub struct ExecutionDriverMetrics {
    executing_transactions: IntGauge,
    executed_transactions: IntCounter,
    execution_failures: IntCounter,
}

impl ExecutionDriverMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            executing_transactions: register_int_gauge_with_registry!(
                "execution_driver_executing_transactions",
                "Number of currently executing transactions in execution driver",
                registry,
            )
            .unwrap(),
            executed_transactions: register_int_counter_with_registry!(
                "execution_driver_executed_transactions",
                "Cumulative number of transaction executed by execution driver",
                registry,
            )
            .unwrap(),
            execution_failures: register_int_counter_with_registry!(
                "execution_driver_execution_failures",
                "Cumulative number of transactions failed to be executed by execution driver",
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}

/// When a notification that a new pending transaction is received we activate
/// processing the transaction in a loop.
pub async fn execution_process<A>(active_authority: Arc<ActiveAuthority<A>>)
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    info!("Starting pending certificates execution process.");

    // Rate limit concurrent executions to # of cpus.
    let limit = Arc::new(Semaphore::new(num_cpus::get()));

    let mut ready_certificates_stream = active_authority
        .state
        .ready_certificates_stream()
        .await
        .expect(
            "Initialization failed: only the executiion driver should receive ready certificates!",
        );

    // Loop whenever there is a signal that a new transactions is ready to process.
    loop {
        let certificate = if let Some(cert) = ready_certificates_stream.recv().await {
            cert
        } else {
            // Should not happen. Only possible if the AuthorityState has shut down.
            warn!("Ready digest stream from authority state is broken. Retrying in 10s ...");
            sleep(std::time::Duration::from_secs(10)).await;
            continue;
        };
        let epoch = certificate.epoch();
        let digest = *certificate.digest();
        debug!(tx_digest=?digest, "Pending certificate execution activated.");

        let limit = limit.clone();
        // hold semaphore permit until task completes. unwrap ok because we never close
        // the semaphore in this context.
        let permit = limit.acquire_owned().await.unwrap();
        let authority = active_authority.clone();

        authority
            .execution_driver_metrics
            .executing_transactions
            .inc();

        spawn_monitored_task!(async move {
            let _guard = permit;
            if let Ok(true) = authority.state.is_tx_already_executed(certificate.digest()) {
                return;
            }
            let mut attempts = 0;
            loop {
                attempts += 1;
                match authority.state.handle_certificate(&certificate).await {
                    Ok(_) => {
                        authority
                            .execution_driver_metrics
                            .executed_transactions
                            .inc();
                        authority
                            .execution_driver_metrics
                            .executing_transactions
                            .dec();
                        debug!(tx_digest=?digest, "Transaction execution succeeded.");
                        break;
                    }
                    Err(SuiError::ValidatorHaltedAtEpochEnd) => {
                        // Unretriable error.
                        debug!(tx_digest=?digest, "Validator halted. Aborting transaction execution.");
                        break;
                    }
                    Err(e) => {
                        if attempts == EXECUTION_MAX_ATTEMPTS {
                            error!(tx_digest=?digest, "Failed to execute certified transaction after {attempts} attempts! error={e} certificate={:?}", certificate);
                            authority.execution_driver_metrics.execution_failures.inc();
                            break;
                        }
                        // Assume only transient failure can happen. Permanent failure is probably
                        // a bug. There would be nothing that can be done for permanent failures.
                        warn!(tx_digest=?digest, "Failed to execute certified transaction! attempt {attempts}, {e}");
                        sleep(EXECUTION_FAILURE_RETRY_INTERVAL).await;
                    }
                }
            }

            // Notifies transaction manager about a certificate is committed.
            // After this, the transaction will no longer be retried if the effect is written.
            //
            // REQUIRED: this must be called after finishing all operations that must be retried
            // for a transaction after crash recovery. Also, between calling enqueue_to_execute()
            // to calling commit() on a transaction, all operations writing to storage need to be
            // idempotent.
            {
                let mut transaction_manager = authority.state.transaction_manager.lock().await;
                transaction_manager
                    .commit(epoch, &digest)
                    .expect("Transaction commit cannot fail!");
            }
        });
    }
}
