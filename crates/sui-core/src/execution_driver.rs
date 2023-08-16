// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::RwLock;
use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use mysten_metrics::{monitored_scope, spawn_monitored_task};
use sui_types::{
    digests::TransactionEffectsDigest, executable_transaction::VerifiedExecutableTransaction,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::OwnedSemaphorePermit;
use tokio::{
    sync::{mpsc::UnboundedReceiver, oneshot, Semaphore},
    time::sleep,
};
use tracing::{error, error_span, info, trace, Instrument};

use crate::authority::{AuthorityMetrics, AuthorityState};

#[cfg(test)]
#[path = "unit_tests/execution_driver_tests.rs"]
mod execution_driver_tests;

// Execution should not encounter permanent failures, so any failure can and needs
// to be retried.
pub const EXECUTION_MAX_ATTEMPTS: u32 = 10;
const EXECUTION_FAILURE_RETRY_INTERVAL: Duration = Duration::from_secs(1);

pub struct ExecutionDispatcher {
    limit: Arc<Semaphore>,
    tx_ready_certificates: UnboundedSender<(
        VerifiedExecutableTransaction,
        Option<TransactionEffectsDigest>,
    )>,
    authority: RwLock<Option<Weak<AuthorityState>>>,
    metrics: Arc<AuthorityMetrics>,
}

/// When a notification that a new pending transaction is received we activate
/// processing the transaction in a loop.
pub async fn execution_process(
    authority_state: Weak<AuthorityState>,
    mut rx_ready_certificates: UnboundedReceiver<(
        VerifiedExecutableTransaction,
        Option<TransactionEffectsDigest>,
    )>,
    mut rx_execution_shutdown: oneshot::Receiver<()>,
    limit: Arc<Semaphore>,
) {
    info!("Starting pending certificates execution process.");

    // Loop whenever there is a signal that a new transactions is ready to process.
    loop {
        let _scope = monitored_scope("ExecutionDriver::loop");

        let certificate;
        let expected_effects_digest;
        tokio::select! {
            result = rx_ready_certificates.recv() => {
                if let Some((cert, fx_digest)) = result {
                    certificate = cert;
                    expected_effects_digest = fx_digest;
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

        let digest = *certificate.digest();
        trace!(?digest, "Pending certificate execution activated.");

        let limit = limit.clone();
        // hold semaphore permit until task completes. unwrap ok because we never close
        // the semaphore in this context.
        let permit = {
            let _scope = monitored_scope("ExecutionDriver::acquire_semaphore");
            limit.acquire_owned().await.unwrap()
        };
        // Certificate execution can take significant time, so run it in a separate task.
        spawn_monitored_task!(execution_task(
            permit,
            authority,
            certificate,
            expected_effects_digest
        )
        .instrument(error_span!("execution_driver", tx_digest = ?digest)));
    }
}

async fn execution_task(
    _permit: OwnedSemaphorePermit,
    authority: Arc<AuthorityState>,
    certificate: VerifiedExecutableTransaction,
    expected_effects_digest: Option<TransactionEffectsDigest>,
) {
    let digest = *certificate.digest();
    let _scope = monitored_scope("ExecutionDriver::task");
    if let Ok(true) = authority.is_tx_already_executed(&digest) {
        return;
    }
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    let mut attempts = 0;
    loop {
        attempts += 1;
        let res = authority
            .try_execute_immediately(&certificate, expected_effects_digest, &epoch_store)
            .await;
        if let Err(e) = res {
            if attempts == EXECUTION_MAX_ATTEMPTS {
                panic!("Failed to execute certified transaction {digest:?} after {attempts} attempts! error={e} certificate={certificate:?}");
            }
            // Assume only transient failure can happen. Permanent failure is probably
            // a bug. There is nothing that can be done to recover from permanent failures.
            error!(tx_digest=?digest, "Failed to execute certified transaction {digest:?}! attempt {attempts}, {e}");
            sleep(EXECUTION_FAILURE_RETRY_INTERVAL).await;
        } else {
            break;
        }
    }
    authority
        .metrics
        .execution_driver_executed_transactions
        .inc();
}

impl ExecutionDispatcher {
    pub fn new(
        tx_ready_certificates: UnboundedSender<(
            VerifiedExecutableTransaction,
            Option<TransactionEffectsDigest>,
        )>,
        semaphore: Arc<Semaphore>,
        metrics: Arc<AuthorityMetrics>,
    ) -> Self {
        let authority = RwLock::new(None);
        Self {
            limit: semaphore,
            tx_ready_certificates,
            authority,
            metrics,
        }
    }

    pub fn dispatch_certificate(
        &self,
        certificate: VerifiedExecutableTransaction,
        expected_effects_digest: Option<TransactionEffectsDigest>,
    ) {
        let authority = self.authority.read();
        let authority = authority.as_ref().and_then(|w| w.upgrade());
        if let Some(authority) = authority {
            if let Ok(permit) = self.limit.clone().try_acquire_owned() {
                // Schedule execution immediately when permit is available
                // This helps to avoid using single dispatcher thread to dispatch all transactions
                let digest = *certificate.digest();
                spawn_monitored_task!(execution_task(
                    permit,
                    authority,
                    certificate,
                    expected_effects_digest
                )
                .instrument(error_span!("execution_driver", tx_digest = ?digest)));
                return;
            }
        }
        self.metrics.execution_driver_dispatch_queue.inc();
        self.tx_ready_certificates
            .send((certificate, expected_effects_digest))
            .ok();
    }

    pub fn set_authority(&self, authority: Weak<AuthorityState>) {
        let mut ptr = self.authority.write();
        assert!(ptr.is_none());
        *ptr = Some(authority);
    }

    #[cfg(test)]
    pub fn shutdown_execution_for_test(&self) {
        *self.authority.write() = None;
    }
}
