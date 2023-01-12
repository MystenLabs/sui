// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
Transaction Orchestrator is a Node component that utilizes Quorum Driver to
submit transactions to validators for finality, and proactively executes
finalized transactions locally, when possible.
*/
use crate::authority::authority_notify_read::{NotifyRead, Registration};
use crate::authority::AuthorityState;
use crate::authority_aggregator::{AuthAggMetrics, AuthorityAggregator};
use crate::authority_client::{AuthorityAPI, NetworkAuthorityClient};
use crate::histogram::Histogram;
use crate::quorum_driver::reconfig_observer::{OnsiteReconfigObserver, ReconfigObserver};
use crate::quorum_driver::{QuorumDriverHandler, QuorumDriverHandlerBuilder, QuorumDriverMetrics};
use crate::safe_client::SafeClientMetricsBase;
use mysten_metrics::spawn_monitored_task;
use prometheus::core::{AtomicI64, AtomicU64, GenericCounter, GenericGauge};
use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry, Registry,
};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use sui_storage::write_path_pending_tx_log::WritePathPendingTransactionLog;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::Committee;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
    QuorumDriverResponse, VerifiedCertificate, VerifiedCertifiedTransactionEffects,
};
use sui_types::quorum_driver_types::{QuorumDriverError, QuorumDriverResult};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::{self, Receiver};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, error, instrument, warn};

use sui_types::messages::VerifiedTransaction;

// How long to wait for local execution (including parents) before a timeout
// is returned to client.
const LOCAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(5);

const WAIT_FOR_FINALITY_TIMEOUT: Duration = Duration::from_secs(30);

pub struct TransactiondOrchestrator<A> {
    quorum_driver_handler: Arc<QuorumDriverHandler<A>>,
    validator_state: Arc<AuthorityState>,
    _local_executor_handle: JoinHandle<()>,
    pending_tx_log: Arc<WritePathPendingTransactionLog>,
    notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
    metrics: Arc<TransactionOrchestratorMetrics>,
}

impl TransactiondOrchestrator<NetworkAuthorityClient> {
    pub fn new_with_network_clients(
        validator_state: Arc<AuthorityState>,
        reconfig_channel: broadcast::Receiver<Committee>,
        parent_path: &Path,
        prometheus_registry: &Registry,
    ) -> anyhow::Result<Self> {
        let safe_client_metrics_base = SafeClientMetricsBase::new(prometheus_registry);
        let auth_agg_metrics = AuthAggMetrics::new(prometheus_registry);
        let validators = AuthorityAggregator::new_from_local_system_state(
            &validator_state.db(),
            validator_state.committee_store(),
            safe_client_metrics_base.clone(),
            auth_agg_metrics.clone(),
        )?;

        let observer = OnsiteReconfigObserver::new(
            reconfig_channel,
            validator_state.db(),
            validator_state.clone_committee_store(),
            safe_client_metrics_base,
            auth_agg_metrics,
        );
        Ok(TransactiondOrchestrator::new(
            Arc::new(validators),
            validator_state,
            parent_path,
            prometheus_registry,
            observer,
        ))
    }
}

impl<A> TransactiondOrchestrator<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
    OnsiteReconfigObserver: ReconfigObserver<A>,
{
    pub fn new(
        validators: Arc<AuthorityAggregator<A>>,
        validator_state: Arc<AuthorityState>,
        parent_path: &Path,
        prometheus_registry: &Registry,
        reconfig_observer: OnsiteReconfigObserver,
    ) -> Self {
        let notifier = Arc::new(NotifyRead::new());
        let quorum_driver_handler = Arc::new(
            QuorumDriverHandlerBuilder::new(
                validators,
                Arc::new(QuorumDriverMetrics::new(prometheus_registry)),
            )
            .with_notifier(notifier.clone())
            .with_reconfig_observer(Arc::new(reconfig_observer))
            .start(),
        );

        let effects_receiver = quorum_driver_handler.subscribe_to_effects();
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
            validator_state,
            _local_executor_handle,
            pending_tx_log,
            notifier,
            metrics,
        }
    }

    #[instrument(name = "tx_orchestrator_execute_transaction", level = "debug", skip_all, fields(request_type = ?request.request_type), err)]
    pub async fn execute_transaction(
        &self,
        request: ExecuteTransactionRequest,
    ) -> Result<ExecuteTransactionResponse, QuorumDriverError> {
        let (_in_flight_metrics_guard, good_response_metrics) =
            self.update_metrics(&request.request_type);

        // TODO check if tx is already executed on this node.
        // Note: since EffectsCert is not stored today, we need to gather that from validators
        // (and maybe store it for caching purposes)

        let transaction = request.transaction.verify()?;
        let tx_digest = *transaction.digest();

        let _request_guard = self.metrics.request_latency.start_timer();
        let _wait_for_finality_guard = self.metrics.wait_for_finality_latency.start_timer();

        let ticket = self.submit(transaction).await?;

        let wait_for_local_execution = matches!(
            request.request_type,
            ExecuteTransactionRequestType::WaitForLocalExecution
        );

        let Ok(result) = timeout(
            WAIT_FOR_FINALITY_TIMEOUT,
            ticket,
        ).await else {
            debug!(?tx_digest, "Timeout waiting for transaction finality.");
            return Err(QuorumDriverError::TimeoutBeforeReachFinality);
        };
        match result {
            Err(err) => Err(err),
            Ok(response) => {
                good_response_metrics.inc();
                let QuorumDriverResponse {
                    tx_cert,
                    effects_cert,
                } = response;
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

    /// Submits the transaction for execution queue, returns a Future to be awaited
    async fn submit(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiResult<Registration<TransactionDigest, QuorumDriverResult>> {
        let ticket = self.notifier.register_one(transaction.digest());
        if self
            .pending_tx_log
            .write_pending_transaction_maybe(&transaction)
            .await?
        {
            self.quorum_driver().submit_transaction(transaction).await?;
        }
        Ok(ticket)
    }

    #[instrument(name = "tx_orchestrator_execute_finalized_tx_locally_with_timeout", level = "debug", skip_all, fields(tx_digest = ?tx_cert.digest()), err)]
    async fn execute_finalized_tx_locally_with_timeout(
        validator_state: &Arc<AuthorityState>,
        tx_cert: &VerifiedCertificate,
        effects_cert: &VerifiedCertifiedTransactionEffects,
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
        let _guard = metrics.local_execution_latency.start_timer();
        match timeout(
            LOCAL_EXECUTION_TIMEOUT,
            validator_state.execute_certificate_with_effects(
                tx_cert,
                effects_cert,
                // TODO: Check whether it's safe to call epoch_store here.
                &validator_state.epoch_store(),
            ),
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
        mut effects_receiver: Receiver<QuorumDriverResponse>,
        pending_transaction_log: Arc<WritePathPendingTransactionLog>,
        metrics: Arc<TransactionOrchestratorMetrics>,
    ) {
        loop {
            match effects_receiver.recv().await {
                Ok(QuorumDriverResponse {
                    tx_cert,
                    effects_cert,
                }) => {
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

    pub fn quorum_driver(&self) -> &Arc<QuorumDriverHandler<A>> {
        &self.quorum_driver_handler
    }

    pub fn clone_quorum_driver(&self) -> Arc<QuorumDriverHandler<A>> {
        self.quorum_driver_handler.clone()
    }

    pub fn clone_authority_aggregator(&self) -> Arc<AuthorityAggregator<A>> {
        self.quorum_driver().authority_aggregator().load_full()
    }

    pub fn subscribe_to_effects_queue(&self) -> Receiver<QuorumDriverResponse> {
        self.quorum_driver_handler.subscribe_to_effects()
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

    request_latency: Histogram,
    wait_for_finality_latency: Histogram,
    local_execution_latency: Histogram,
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
            request_latency: Histogram::new_in_registry(
                "tx_orchestrator_request_latency",
                "Time spent in processing one Transaction Orchestrator request",
                registry,
            ),
            wait_for_finality_latency: Histogram::new_in_registry(
                "tx_orchestrator_wait_for_finality_latency",
                "Time spent in waiting for one Transaction Orchestrator request gets finalized",
                registry,
            ),
            local_execution_latency: Histogram::new_in_registry(
                "tx_orchestrator_local_execution_latency",
                "Time spent in waiting for one Transaction Orchestrator gets locally executed",
                registry,
            ),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}
