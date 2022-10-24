// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
Transaction Orchestrator is a Node component that utilizes Quorum Driver to
submit transactions to validators for finality, and proactively executes
finalized transactions locally, with the help of Node Sync.
*/
use prometheus::core::{AtomicI64, AtomicU64, GenericCounter, GenericGauge};
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::StreamExt;

use crate::authority::AuthorityState;
use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::AuthorityAPI;
use crate::node_sync::{NodeSyncHandle, SyncStatus};
use crate::quorum_driver::{QuorumDriver, QuorumDriverHandler, QuorumDriverMetrics};
use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry, Registry,
};
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{
    CertifiedTransaction, CertifiedTransactionEffects, ExecuteTransactionRequest,
    ExecuteTransactionRequestType, ExecuteTransactionResponse, QuorumDriverRequest,
    QuorumDriverRequestType, QuorumDriverResponse,
};
use tap::TapFallible;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, error, instrument, warn, Instrument};

// How long to wait for local execution (including parents) before a timeout
// is returned to client.
const LOCAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(5);

pub struct TransactiondOrchestrator<A> {
    quorum_driver_handler: QuorumDriverHandler<A>,
    quorum_driver: Arc<QuorumDriver<A>>,
    node_sync_handle: NodeSyncHandle,
    validator_state: Arc<AuthorityState>,
    _local_executor_handle: JoinHandle<()>,
    metrics: Arc<TransactionOrchestratorMetrics>,
}

impl<A> TransactiondOrchestrator<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(
        validators: Arc<AuthorityAggregator<A>>,
        validator_state: Arc<AuthorityState>,
        node_sync_handle: NodeSyncHandle,
        prometheus_registry: &Registry,
    ) -> Self {
        let quorum_driver_handler =
            QuorumDriverHandler::new(validators, QuorumDriverMetrics::new(prometheus_registry));
        let quorum_driver = quorum_driver_handler.clone_quorum_driver();
        let effects_receiver = quorum_driver_handler.subscribe();
        let state_clone = validator_state.clone();
        let handle_clone = node_sync_handle.clone();
        let metrics = Arc::new(TransactionOrchestratorMetrics::new(prometheus_registry));
        let metrics_clone = metrics.clone();
        let _local_executor_handle = {
            tokio::task::spawn(async move {
                Self::loop_execute_finalized_tx_locally(
                    state_clone,
                    handle_clone,
                    effects_receiver,
                    metrics_clone,
                )
                .await;
            })
        };
        Self {
            quorum_driver_handler,
            quorum_driver,
            validator_state,
            node_sync_handle,
            _local_executor_handle,
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
        let wait_for_local_execution = matches!(
            request.request_type,
            ExecuteTransactionRequestType::WaitForLocalExecution
        );
        let transaction = request.transaction;
        let tx_digest = *transaction.digest();
        let request_type = match request.request_type {
            ExecuteTransactionRequestType::ImmediateReturn => {
                QuorumDriverRequestType::ImmediateReturn
            }
            ExecuteTransactionRequestType::WaitForTxCert => QuorumDriverRequestType::WaitForTxCert,
            ExecuteTransactionRequestType::WaitForEffectsCert
            | ExecuteTransactionRequestType::WaitForLocalExecution => {
                QuorumDriverRequestType::WaitForEffectsCert
            }
        };
        let execution_result = self
            .quorum_driver
            .execute_transaction(QuorumDriverRequest {
                transaction,
                request_type,
            })
            .await
            .tap_err(|err| {
                debug!(
                    ?tx_digest,
                    "Failed to execute transction via Quorum Driver: {:?}", err
                )
            })?;

        good_response_metrics.inc();
        match execution_result {
            QuorumDriverResponse::ImmediateReturn => {
                Ok(ExecuteTransactionResponse::ImmediateReturn)
            }
            QuorumDriverResponse::TxCert(result) => {
                Ok(ExecuteTransactionResponse::TxCert(Box::new(*result)))
            }
            QuorumDriverResponse::EffectsCert(result) => {
                let (tx_cert, effects_cert) = *result;
                if !wait_for_local_execution {
                    return Ok(ExecuteTransactionResponse::EffectsCert(Box::new((
                        tx_cert,
                        effects_cert,
                        false,
                    ))));
                }
                match Self::execute_finalized_tx_locally_with_timeout(
                    &self.validator_state,
                    &self.node_sync_handle,
                    &tx_cert,
                    &effects_cert,
                    &self.metrics,
                )
                .await
                {
                    Ok(_) => Ok(ExecuteTransactionResponse::EffectsCert(Box::new((
                        tx_cert,
                        effects_cert,
                        true,
                    )))),
                    Err(_) => Ok(ExecuteTransactionResponse::EffectsCert(Box::new((
                        tx_cert,
                        effects_cert,
                        false,
                    )))),
                }
            }
        }
    }

    #[instrument(name = "tx_orchestrator_execute_finalized_tx_locally_with_timeout", level = "debug", skip_all, fields(tx_digest = ?tx_cert.digest()), err)]
    async fn execute_finalized_tx_locally_with_timeout(
        validator_state: &Arc<AuthorityState>,
        node_sync_handle: &NodeSyncHandle,
        tx_cert: &CertifiedTransaction,
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
            Self::execute_impl(
                validator_state,
                node_sync_handle,
                tx_cert,
                effects_cert,
                metrics,
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
        node_sync_handle: NodeSyncHandle,
        mut effects_receiver: Receiver<(CertifiedTransaction, CertifiedTransactionEffects)>,
        metrics: Arc<TransactionOrchestratorMetrics>,
    ) {
        loop {
            match effects_receiver.recv().await {
                Ok((tx_cert, effects_cert)) => {
                    let _ = Self::execute_finalized_tx_locally_with_timeout(
                        &validator_state,
                        &node_sync_handle,
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
    ) -> Receiver<(CertifiedTransaction, CertifiedTransactionEffects)> {
        self.quorum_driver_handler.subscribe()
    }

    /// Execute a finalized transaction locally.
    /// Firstly it tries to execute it optimistically. If there are missing
    /// dependencies, it then leverages Node Sync to process the parents.
    async fn execute_impl(
        state: &Arc<AuthorityState>,
        node_sync_handle: &NodeSyncHandle,
        tx_cert: &CertifiedTransaction,
        effects_cert: &CertifiedTransactionEffects,
        metrics: &TransactionOrchestratorMetrics,
    ) -> SuiResult {
        let tx_digest = tx_cert.digest();
        let res = state
            .handle_certificate_with_effects(tx_cert, effects_cert)
            .await;
        match res {
            Ok(_) => {
                debug!(
                    ?tx_digest,
                    "Orchestrator optimistically executed transaction successfully."
                );
                metrics.tx_directly_executed.inc();
                Ok(())
            }
            e @ Err(SuiError::TransactionInputObjectsErrors { .. }) => {
                debug!(?tx_digest, "Orchestrator failed to executue transaction optimistically due to missing parents: {:?}", e);

                match node_sync_handle
                    .handle_parents_request(
                        state.committee.load().epoch,
                        std::iter::once(*tx_digest),
                    )
                    .await?
                    .next()
                    .instrument(tracing::debug_span!(
                        "transaction_orchestrator_execute_tx_via_node_sync"
                    ))
                    .await
                    // Safe to unwrap because `handle_execution_request` wraps futures one by one
                    .unwrap()?
                {
                    SyncStatus::CertExecuted => {
                        metrics.tx_executed_via_node_sync.inc();
                        debug!(
                            ?tx_digest,
                            "Orchestrator executed transaction via Node Sync."
                        );
                        Ok(())
                    }
                    SyncStatus::NotFinal => {
                        // This shall not happen
                        metrics.tx_not_executed.inc();
                        error!(
                            ?tx_digest,
                            "Orchestrator failed to execute finalized transaction via Node Sync"
                        );
                        Err(SuiError::from(
                            "Tx from orchestrator failed to be executed via node sync",
                        ))
                    }
                }
            }
            Err(e) => Err(e),
        }
    }

    fn update_metrics(
        &'_ self,
        request_type: &ExecuteTransactionRequestType,
    ) -> (impl Drop, &'_ GenericCounter<AtomicU64>) {
        let (in_flight, good_response) = match request_type {
            ExecuteTransactionRequestType::ImmediateReturn => {
                self.metrics.total_req_received_immediate_return.inc();
                (
                    &self.metrics.req_in_flight_immediate_return,
                    &self.metrics.good_response_immediate_return,
                )
            }
            ExecuteTransactionRequestType::WaitForTxCert => {
                self.metrics.total_req_received_wait_for_tx_cert.inc();
                (
                    &self.metrics.req_in_flight_wait_for_tx_cert,
                    &self.metrics.good_response_wait_for_tx_cert,
                )
            }
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
}

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
#[derive(Clone)]
pub struct TransactionOrchestratorMetrics {
    total_req_received_immediate_return: GenericCounter<AtomicU64>,
    total_req_received_wait_for_tx_cert: GenericCounter<AtomicU64>,
    total_req_received_wait_for_effects_cert: GenericCounter<AtomicU64>,
    total_req_received_wait_for_local_execution: GenericCounter<AtomicU64>,

    good_response_immediate_return: GenericCounter<AtomicU64>,
    good_response_wait_for_tx_cert: GenericCounter<AtomicU64>,
    good_response_wait_for_effects_cert: GenericCounter<AtomicU64>,
    good_response_wait_for_local_execution: GenericCounter<AtomicU64>,

    req_in_flight_immediate_return: GenericGauge<AtomicI64>,
    req_in_flight_wait_for_tx_cert: GenericGauge<AtomicI64>,
    req_in_flight_wait_for_effects_cert: GenericGauge<AtomicI64>,
    req_in_flight_wait_for_local_execution: GenericGauge<AtomicI64>,

    local_execution_in_flight: GenericGauge<AtomicI64>,
    local_execution_success: GenericCounter<AtomicU64>,
    local_execution_timeout: GenericCounter<AtomicU64>,
    local_execution_failure: GenericCounter<AtomicU64>,

    tx_directly_executed: GenericCounter<AtomicU64>,
    tx_executed_via_node_sync: GenericCounter<AtomicU64>,
    tx_not_executed: GenericCounter<AtomicU64>,
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

        let total_req_received_immediate_return =
            total_req_received.with_label_values(&["immediate_return"]);
        let total_req_received_wait_for_tx_cert =
            total_req_received.with_label_values(&["wait_for_tx_cert"]);
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

        let good_response_immediate_return = good_response.with_label_values(&["immediate_return"]);
        let good_response_wait_for_tx_cert = good_response.with_label_values(&["wait_for_tx_cert"]);
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

        let req_in_flight_immediate_return = req_in_flight.with_label_values(&["immediate_return"]);
        let req_in_flight_wait_for_tx_cert = req_in_flight.with_label_values(&["wait_for_tx_cert"]);
        let req_in_flight_wait_for_effects_cert =
            req_in_flight.with_label_values(&["wait_for_effects_cert"]);
        let req_in_flight_wait_for_local_execution =
            req_in_flight.with_label_values(&["wait_for_local_execution"]);

        Self {
            total_req_received_immediate_return,
            total_req_received_wait_for_tx_cert,
            total_req_received_wait_for_effects_cert,
            total_req_received_wait_for_local_execution,
            good_response_immediate_return,
            good_response_wait_for_tx_cert,
            good_response_wait_for_effects_cert,
            good_response_wait_for_local_execution,
            req_in_flight_immediate_return,
            req_in_flight_wait_for_tx_cert,
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
            tx_directly_executed: register_int_counter_with_registry!(
                "tx_orchestrator_tx_directly_executed",
                "Total number of txns Transaction Orchestrator directly executed",
                registry,
            )
            .unwrap(),
            tx_executed_via_node_sync: register_int_counter_with_registry!(
                "tx_orchestrator_tx_executed_via_node_sync",
                "Total number of txns Transaction Orchestrator executed via node sync",
                registry,
            )
            .unwrap(),
            tx_not_executed: register_int_counter_with_registry!(
                "tx_orchestrator_tx_not_executed",
                "Total number of txns Transaction Orchestrator failed to execute",
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
