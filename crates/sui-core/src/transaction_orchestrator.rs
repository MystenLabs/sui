// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
Transaction Orchestrator is a Node component that utilizes Quorum Driver to
submit transactions to validators for finality, and proactively executes
finalized transactions locally, when possible.
*/
use prometheus::core::{AtomicI64, AtomicU64, GenericCounter, GenericGauge};
use std::sync::Arc;
use std::time::Duration;

use crate::authority::AuthorityState;
use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::AuthorityAPI;
use crate::quorum_driver::{QuorumDriver, QuorumDriverHandler, QuorumDriverMetrics};
use mysten_metrics::spawn_monitored_task;
use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry, Registry,
};
use std::path::Path;
use sui_storage::write_path_pending_tx_log::WritePathPendingTransactionLog;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{
    CertifiedTransactionEffects, ExecuteTransactionRequest, ExecuteTransactionRequestType,
    ExecuteTransactionResponse, QuorumDriverRequest, QuorumDriverResponse, VerifiedCertificate,
    VerifiedCertifiedTransactionEffects,
};
use tap::TapFallible;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, error, instrument, warn};

use sui_types::messages::VerifiedTransaction;

// How long to wait for local execution (including parents) before a timeout
// is returned to client.
const LOCAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(5);

pub struct TransactiondOrchestrator<A> {
    quorum_driver_handler: QuorumDriverHandler<A>,
    quorum_driver: Arc<QuorumDriver<A>>,
    validator_state: Arc<AuthorityState>,
    _local_executor_handle: JoinHandle<()>,
    pending_tx_log: Arc<WritePathPendingTransactionLog>,
    metrics: Arc<TransactionOrchestratorMetrics>,
}

impl<A> TransactiondOrchestrator<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(
        validators: Arc<AuthorityAggregator<A>>,
        validator_state: Arc<AuthorityState>,
        parent_path: &Path,
        prometheus_registry: &Registry,
    ) -> Self {
        let quorum_driver_handler =
            QuorumDriverHandler::new(validators, QuorumDriverMetrics::new(prometheus_registry));
        let quorum_driver = quorum_driver_handler.clone_quorum_driver();
        let effects_receiver = quorum_driver_handler.subscribe();
        let state_clone = validator_state.clone();
        let metrics = Arc::new(TransactionOrchestratorMetrics::new(prometheus_registry));
        let metrics_clone = metrics.clone();
        let pending_tx_log = Arc::new(WritePathPendingTransactionLog::new(
            parent_path.join("fullnode_pending_transactions"),
        ));
        let pending_tx_log_clone = pending_tx_log.clone();
        let _local_executor_handle = {
            spawn_monitored_task!(async move {
                Self::loop_execute_finalized_tx_locally(
                    state_clone,
                    effects_receiver,
                    pending_tx_log_clone,
                    metrics_clone,
                )
                .await;
            })
        };
        Self {
            quorum_driver_handler,
            quorum_driver,
            validator_state,
            _local_executor_handle,
            pending_tx_log,
            metrics,
        }
    }

    #[instrument(name = "tx_orchestrator_execute_transaction", level = "debug", skip_all, fields(request_type = ?request.request_type), err)]
    pub async fn execute_transaction(
        &self,
        request: ExecuteTransactionRequest,
    ) -> SuiResult<ExecuteTransactionResponse> {
        let (_in_flight_metrics_guard, good_response_metrics) =
            self.update_metrics(&request.request_type);

        // TODO check if tx is already executed on this node.
        // Note: since EffectsCert is not stored today, we need to gather that from validators
        // (and maybe store it for caching purposes)

        let tx_digest = *request.transaction.digest();
        let transaction = request
            .transaction
            .verify()
            .tap_err(|e| debug!(?tx_digest, "Failed to verify user signature: {:?}", e))?;

        // We will shortly refactor the TransactionOrchestrator with a queue-based implementation.
        // TDOO: `should_enqueue` will be used to determine if the transaction should be enqueued.
        let _should_enqueue = self
            .pending_tx_log
            .write_pending_transaction_maybe(&transaction)
            .await?;

        let wait_for_local_execution = matches!(
            request.request_type,
            ExecuteTransactionRequestType::WaitForLocalExecution
        );

        let execution_result = self
            .quorum_driver
            .execute_transaction(QuorumDriverRequest { transaction })
            .await
            .tap_err(|err| {
                debug!(
                    ?tx_digest,
                    "Failed to execute transction via Quorum Driver: {:?}", err
                )
            })?;

        good_response_metrics.inc();
        match execution_result {
            QuorumDriverResponse::EffectsCert(result) => {
                let (tx_cert, effects_cert) = *result;
                if !wait_for_local_execution {
                    return Ok(ExecuteTransactionResponse::EffectsCert(Box::new((
                        tx_cert.into(),
                        effects_cert.into(),
                        false,
                    ))));
                }
                match Self::execute_finalized_tx_locally_with_timeout(
                    &self.validator_state,
                    &tx_cert,
                    &effects_cert,
                    &self.metrics,
                )
                .await
                {
                    Ok(_) => Ok(ExecuteTransactionResponse::EffectsCert(Box::new((
                        tx_cert.into(),
                        effects_cert.into(),
                        true,
                    )))),
                    Err(_) => Ok(ExecuteTransactionResponse::EffectsCert(Box::new((
                        tx_cert.into(),
                        effects_cert.into(),
                        false,
                    )))),
                }
            }
        }
    }

    #[instrument(name = "tx_orchestrator_execute_finalized_tx_locally_with_timeout", level = "debug", skip_all, fields(tx_digest = ?tx_cert.digest()), err)]
    async fn execute_finalized_tx_locally_with_timeout(
        validator_state: &Arc<AuthorityState>,
        tx_cert: &VerifiedCertificate,
        effects_cert: &CertifiedTransactionEffects,
        metrics: &TransactionOrchestratorMetrics,
    ) -> SuiResult {
        // TODO: attempt a finalized tx at most once per request.
        // Every WaitForLocalExecution request will be attempted to execute twice,
        // one from the subscriber queue, one from the proactive execution before
        // returning results to clients. This is not insanely bad because:
        // 1. it's possible that one attempt finishes before the other, so there's
        //      zero extra work except DB checks
        // 2. an up-to-date fullnode should have minimal overhead to sync parents
        //      (for one extra time)
        // 3. at the end of day, the tx will be executed at most once per lock guard.
        let tx_digest = tx_cert.digest();
        if validator_state.is_tx_already_executed(tx_digest)? {
            return Ok(());
        }
        let _metrics_guard =
            scopeguard::guard(metrics.local_execution_in_flight.clone(), |in_flight| {
                in_flight.dec();
            });
        match timeout(
            LOCAL_EXECUTION_TIMEOUT,
            validator_state.execute_certificate_with_effects(tx_cert, effects_cert),
        )
        .await
        {
            Err(_elapsed) => {
                debug!(
                    ?tx_digest,
                    "Executing tx locally by orchestrator timed out within {:?}.",
                    LOCAL_EXECUTION_TIMEOUT
                );
                metrics.local_execution_timeout.inc();
                Err(SuiError::TimeoutError)
            }
            Ok(Err(err)) => {
                debug!(
                    ?tx_digest,
                    "Executing tx locally by orchestrator failed with error: {:?}", err
                );
                metrics.local_execution_failure.inc();
                Err(SuiError::TransactionOrchestratorLocalExecutionError {
                    error: err.to_string(),
                })
            }
            Ok(Ok(_)) => {
                metrics.local_execution_success.inc();
                Ok(())
            }
        }
    }

    async fn loop_execute_finalized_tx_locally(
        validator_state: Arc<AuthorityState>,
        mut effects_receiver: Receiver<(VerifiedCertificate, VerifiedCertifiedTransactionEffects)>,
        pending_transaction_log: Arc<WritePathPendingTransactionLog>,
        metrics: Arc<TransactionOrchestratorMetrics>,
    ) {
        loop {
            match effects_receiver.recv().await {
                Ok((tx_cert, effects_cert)) => {
                    let tx_digest = tx_cert.digest();
                    if let Err(err) = pending_transaction_log.finish_transaction(tx_digest) {
                        error!(
                            ?tx_digest,
                            "Failed to finish transaction in pending transaction log: {err}"
                        );
                    }
                    let _ = Self::execute_finalized_tx_locally_with_timeout(
                        &validator_state,
                        &tx_cert,
                        &effects_cert,
                        &metrics,
                    )
                    .await;
                }
                Err(RecvError::Closed) => {
                    error!("Sender of effects subscriber queue has been dropped!");
                    return;
                }
                Err(RecvError::Lagged(skipped_count)) => {
                    warn!("Skipped {skipped_count} transasctions in effects subscriber queue.");
                }
            }
        }
    }

    pub fn quorum_driver(&self) -> &Arc<QuorumDriver<A>> {
        &self.quorum_driver
    }

    pub fn subscribe_to_effects_queue(
        &self,
    ) -> Receiver<(VerifiedCertificate, VerifiedCertifiedTransactionEffects)> {
        self.quorum_driver_handler.subscribe()
    }

    fn update_metrics(
        &'_ self,
        request_type: &ExecuteTransactionRequestType,
    ) -> (impl Drop, &'_ GenericCounter<AtomicU64>) {
        let (in_flight, good_response) = match request_type {
            ExecuteTransactionRequestType::WaitForEffectsCert => {
                self.metrics.total_req_received_wait_for_effects_cert.inc();
                (
                    &self.metrics.req_in_flight_wait_for_effects_cert,
                    &self.metrics.good_response_wait_for_effects_cert,
                )
            }
            ExecuteTransactionRequestType::WaitForLocalExecution => {
                self.metrics
                    .total_req_received_wait_for_local_execution
                    .inc();
                (
                    &self.metrics.req_in_flight_wait_for_local_execution,
                    &self.metrics.good_response_wait_for_local_execution,
                )
            }
        };
        in_flight.inc();
        (
            scopeguard::guard(in_flight.clone(), |in_flight| {
                in_flight.dec();
            }),
            good_response,
        )
    }

    pub fn load_all_pending_transactions(&self) -> Vec<VerifiedTransaction> {
        self.pending_tx_log.load_all_pending_transactions()
    }
}

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
#[derive(Clone)]
pub struct TransactionOrchestratorMetrics {
    total_req_received_wait_for_effects_cert: GenericCounter<AtomicU64>,
    total_req_received_wait_for_local_execution: GenericCounter<AtomicU64>,

    good_response_wait_for_effects_cert: GenericCounter<AtomicU64>,
    good_response_wait_for_local_execution: GenericCounter<AtomicU64>,

    req_in_flight_wait_for_effects_cert: GenericGauge<AtomicI64>,
    req_in_flight_wait_for_local_execution: GenericGauge<AtomicI64>,

    local_execution_in_flight: GenericGauge<AtomicI64>,
    local_execution_success: GenericCounter<AtomicU64>,
    local_execution_timeout: GenericCounter<AtomicU64>,
    local_execution_failure: GenericCounter<AtomicU64>,
}

impl TransactionOrchestratorMetrics {
    pub fn new(registry: &Registry) -> Self {
        let total_req_received = register_int_counter_vec_with_registry!(
            "tx_orchestrator_total_req_received",
            "Total number of executions request Transaction Orchestrator receives, group by request type",
            &["request_type"],
            registry
        )
        .unwrap();

        let total_req_received_wait_for_effects_cert =
            total_req_received.with_label_values(&["wait_for_effects_cert"]);
        let total_req_received_wait_for_local_execution =
            total_req_received.with_label_values(&["wait_for_local_execution"]);

        let good_response = register_int_counter_vec_with_registry!(
            "tx_orchestrator_good_response",
            "Total number of good responses Transaction Orchestrator generates, group by request type",
            &["request_type"],
            registry
        )
        .unwrap();

        let good_response_wait_for_effects_cert =
            good_response.with_label_values(&["wait_for_effects_cert"]);
        let good_response_wait_for_local_execution =
            good_response.with_label_values(&["wait_for_local_execution"]);

        let req_in_flight = register_int_gauge_vec_with_registry!(
            "tx_orchestrator_req_in_flight",
            "Number of requests in flights Transaction Orchestrator processes, group by request type",
            &["request_type"],
            registry
        )
        .unwrap();

        let req_in_flight_wait_for_effects_cert =
            req_in_flight.with_label_values(&["wait_for_effects_cert"]);
        let req_in_flight_wait_for_local_execution =
            req_in_flight.with_label_values(&["wait_for_local_execution"]);

        Self {
            total_req_received_wait_for_effects_cert,
            total_req_received_wait_for_local_execution,
            good_response_wait_for_effects_cert,
            good_response_wait_for_local_execution,
            req_in_flight_wait_for_effects_cert,
            req_in_flight_wait_for_local_execution,
            local_execution_in_flight: register_int_gauge_with_registry!(
                "tx_orchestrator_local_execution_in_flight",
                "Number of local execution txns in flights Transaction Orchestrator handles",
                registry,
            )
            .unwrap(),
            local_execution_success: register_int_counter_with_registry!(
                "tx_orchestrator_local_execution_success",
                "Total number of successful local execution txns Transaction Orchestrator handles",
                registry,
            )
            .unwrap(),
            local_execution_timeout: register_int_counter_with_registry!(
                "tx_orchestrator_local_execution_timeout",
                "Total number of timed-out local execution txns Transaction Orchestrator handles",
                registry,
            )
            .unwrap(),
            local_execution_failure: register_int_counter_with_registry!(
                "tx_orchestrator_local_execution_failure",
                "Total number of failed local execution txns Transaction Orchestrator handles",
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
