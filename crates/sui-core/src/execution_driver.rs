// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Weak,
};
use std::time::Duration;

use mysten_common::{fatal, random::get_rng};
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use rand::Rng;
use sui_macros::fail_point_async;
use sui_types::error::SuiError;
use tokio::sync::{mpsc::UnboundedReceiver, oneshot, OwnedSemaphorePermit, Semaphore};
use tracing::{error_span, info, trace, warn, Instrument};

use crate::authority::AuthorityState;
use crate::execution_scheduler::PendingCertificate;

#[cfg(test)]
#[path = "unit_tests/execution_driver_tests.rs"]
mod execution_driver_tests;

const QUEUEING_DELAY_SAMPLING_RATIO: f64 = 0.05;

/// Define dynamic concurrency controller
#[derive(Clone)]
pub struct DynamicConcurrencyController {
    #[allow(dead_code)]
    base_limit: usize, // cpu limit
    current_limit: Arc<AtomicUsize>,
    max_limit: usize,
    min_limit: usize,
    semaphore: Arc<Semaphore>,
    exec_stats: Arc<ExecutionStats>,
}
#[derive(Default)]
struct ExecutionStats {
    success_count: AtomicUsize,
    fail_count: AtomicUsize,
}
impl DynamicConcurrencyController {
    pub fn new() -> Self {
        let base_limit = num_cpus::get();
        let max_limit = base_limit * 2;
        let min_limit = base_limit.max(2);
        let controller = Self {
            base_limit,
            current_limit: Arc::new(AtomicUsize::new(base_limit)),
            max_limit,
            min_limit,
            semaphore: Arc::new(Semaphore::new(base_limit)),
            exec_stats: Arc::new(ExecutionStats::default()),
        };
        controller.start_adjustment_task();
        controller
    }

    fn start_adjustment_task(&self) {
        let controller = self.clone();
        spawn_monitored_task!(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                controller.adjust_concurrency_limit().await;
            }
        });
    }
    async fn adjust_concurrency_limit(&self) {
        let success = self.exec_stats.success_count.swap(0, Ordering::Relaxed);
        let failure = self.exec_stats.fail_count.swap(0, Ordering::Relaxed);
        let total = success + failure;
        if total == 0 {
            return;
        }

        let failure_rate = failure as f64 / total as f64;
        let current_limit = self.current_limit.load(Ordering::Relaxed);

        let new_limit = if failure_rate > 0.1 {
            current_limit.saturating_sub(1).max(self.min_limit)
        } else if failure_rate < 0.05 {
            current_limit.saturating_add(1).min(self.max_limit)
        } else {
            current_limit
        };

        if new_limit != current_limit {
            let current_permits = self.semaphore.available_permits() as isize;
            let target_permits = new_limit as isize - (current_limit as isize - current_permits);
            if target_permits > current_permits {
                for _ in current_permits..target_permits {
                    self.semaphore.add_permits(1);
                }
            }

            self.current_limit.store(new_limit, Ordering::Relaxed);

            trace!(
                "Adjust concurrency limit from {} to {}",
                current_limit,
                new_limit
            );
        }
    }
    pub async fn acquire(&self) -> OwnedSemaphorePermit {
        self.semaphore.clone().acquire_owned().await.unwrap()
    }
    pub fn record_success(&self) {
        self.exec_stats
            .success_count
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        self.exec_stats.fail_count.fetch_add(1, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn current_limit(&self) -> usize {
        self.current_limit.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    pub fn min_limit(&self) -> usize {
        self.min_limit
    }

    #[allow(dead_code)]
    pub fn max_limit(&self) -> usize {
        self.max_limit
    }
}

/// When a notification that a new pending transaction is received we activate
/// processing the transaction in a loop.
pub async fn execution_process(
    authority_state: Weak<AuthorityState>,
    mut rx_ready_certificates: UnboundedReceiver<PendingCertificate>,
    mut rx_execution_shutdown: oneshot::Receiver<()>,
) {
    info!("Starting pending certificates execution process.");

    let concurrency_controller = Arc::new(DynamicConcurrencyController::new());

    // Loop whenever there is a signal that a new transactions is ready to process.
    loop {
        let _scope = monitored_scope("ExecutionDriver::loop");

        let certificate;
        let execution_env;
        let txn_ready_time;
        let _executing_guard;
        tokio::select! {
            result = rx_ready_certificates.recv() => {
                if let Some(pending_cert) = result {
                    certificate = pending_cert.certificate;
                    execution_env = pending_cert.execution_env;
                    txn_ready_time = pending_cert.stats.ready_time.unwrap();
                    _executing_guard = pending_cert.executing_guard;
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

        let controller = concurrency_controller.clone();
        // Acquire execution permit
        let permit = controller.acquire().await;

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
                execution_env,
                &epoch_store_clone,
            ).await {
                Err(SuiError::ValidatorHaltedAtEpochEnd) => {
                    warn!("Could not execute transaction {digest:?} because validator is halted at epoch end. certificate={certificate:?}");
                    controller.record_failure();
                    return;
                }
                Err(e) => {
                    controller.record_failure();
                    fatal!("Failed to execute certified transaction {digest:?}! error={e} certificate={certificate:?}");
                }
                Ok(_) => {
                    controller.record_success();
                }
            }
            authority
                .metrics
                .execution_driver_executed_transactions
                .inc();
        }.instrument(error_span!("execution_driver", tx_digest = ?digest))));
    }
}
