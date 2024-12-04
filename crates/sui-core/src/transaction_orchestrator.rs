// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
Transaction Orchestrator is a Node component that utilizes Quorum Driver to
submit transactions to validators for finality, and proactively executes
finalized transactions locally, when possible.
*/

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::AuthorityState;
use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::{AuthorityAPI, NetworkAuthorityClient};
use crate::quorum_driver::reconfig_observer::{OnsiteReconfigObserver, ReconfigObserver};
use crate::quorum_driver::{QuorumDriverHandler, QuorumDriverHandlerBuilder, QuorumDriverMetrics};
use futures::future::{select, Either, Future};
use futures::FutureExt;
use mysten_common::sync::notify_read::NotifyRead;
use mysten_metrics::{add_server_timing, spawn_logged_monitored_task, spawn_monitored_task};
use mysten_metrics::{TX_TYPE_SHARED_OBJ_TX, TX_TYPE_SINGLE_WRITER_TX};
use prometheus::core::{AtomicI64, AtomicU64, GenericCounter, GenericGauge};
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry, Histogram, Registry,
};
use std::net::SocketAddr;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use sui_storage::write_path_pending_tx_log::WritePathPendingTransactionLog;
use sui_types::base_types::TransactionDigest;
use sui_types::error::{SuiError, SuiResult};
use sui_types::quorum_driver_types::{
    ExecuteTransactionRequestType, ExecuteTransactionRequestV3, ExecuteTransactionResponseV3,
    FinalizedEffects, IsTransactionExecutedLocally, QuorumDriverEffectsQueueResult,
    QuorumDriverError, QuorumDriverResponse, QuorumDriverResult,
};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::transaction::{TransactionData, VerifiedTransaction};
use sui_types::transaction_executor::SimulateTransactionResult;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, error, error_span, info, instrument, warn, Instrument};

// How long to wait for local execution (including parents) before a timeout
// is returned to client.
const LOCAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(10);

const WAIT_FOR_FINALITY_TIMEOUT: Duration = Duration::from_secs(30);

pub struct TransactiondOrchestrator<A: Clone> {
    quorum_driver_handler: Arc<QuorumDriverHandler<A>>,
    validator_state: Arc<AuthorityState>,
    _local_executor_handle: JoinHandle<()>,
    pending_tx_log: Arc<WritePathPendingTransactionLog>,
    notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
    metrics: Arc<TransactionOrchestratorMetrics>,
}

impl TransactiondOrchestrator<NetworkAuthorityClient> {
    pub fn new_with_auth_aggregator(
        validators: Arc<AuthorityAggregator<NetworkAuthorityClient>>,
        validator_state: Arc<AuthorityState>,
        reconfig_channel: Receiver<SuiSystemState>,
        parent_path: &Path,
        prometheus_registry: &Registry,
    ) -> Self {
        let observer = OnsiteReconfigObserver::new(
            reconfig_channel,
            validator_state.get_object_cache_reader().clone(),
            validator_state.clone_committee_store(),
            validators.safe_client_metrics_base.clone(),
            validators.metrics.deref().clone(),
        );
        TransactiondOrchestrator::new(
            validators,
            validator_state,
            parent_path,
            prometheus_registry,
            observer,
        )
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
        let metrics = Arc::new(QuorumDriverMetrics::new(prometheus_registry));
        let notifier = Arc::new(NotifyRead::new());
        let reconfig_observer = Arc::new(reconfig_observer);
        let quorum_driver_handler = Arc::new(
            QuorumDriverHandlerBuilder::new(validators.clone(), metrics.clone())
                .with_notifier(notifier.clone())
                .with_reconfig_observer(reconfig_observer.clone())
                .start(),
        );

        let effects_receiver = quorum_driver_handler.subscribe_to_effects();
        let metrics = Arc::new(TransactionOrchestratorMetrics::new(prometheus_registry));
        let pending_tx_log = Arc::new(WritePathPendingTransactionLog::new(
            parent_path.join("fullnode_pending_transactions"),
        ));
        let pending_tx_log_clone = pending_tx_log.clone();
        let _local_executor_handle = {
            spawn_monitored_task!(async move {
                Self::loop_pending_transaction_log(effects_receiver, pending_tx_log_clone).await;
            })
        };
        Self::schedule_txes_in_log(pending_tx_log.clone(), quorum_driver_handler.clone());
        Self {
            quorum_driver_handler,
            validator_state,
            _local_executor_handle,
            pending_tx_log,
            notifier,
            metrics,
        }
    }
}

impl<A> TransactiondOrchestrator<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    #[instrument(name = "tx_orchestrator_execute_transaction", level = "debug", skip_all,
    fields(
        tx_digest = ?request.transaction.digest(),
        tx_type = ?request_type,
    ),
    err)]
    pub async fn execute_transaction_block(
        &self,
        request: ExecuteTransactionRequestV3,
        request_type: ExecuteTransactionRequestType,
        client_addr: Option<SocketAddr>,
    ) -> Result<(ExecuteTransactionResponseV3, IsTransactionExecutedLocally), QuorumDriverError>
    {
        let epoch_store = self.validator_state.load_epoch_store_one_call_per_task();

        let (transaction, response) = self
            .execute_transaction_impl(&epoch_store, request, client_addr)
            .await?;

        let executed_locally = if matches!(
            request_type,
            ExecuteTransactionRequestType::WaitForLocalExecution
        ) {
            let executed_locally = Self::wait_for_finalized_tx_executed_locally_with_timeout(
                &self.validator_state,
                &transaction,
                &self.metrics,
            )
            .await
            .is_ok();
            add_server_timing("local_execution");
            executed_locally
        } else {
            false
        };

        let QuorumDriverResponse {
            effects_cert,
            events,
            input_objects,
            output_objects,
            auxiliary_data,
        } = response;

        let response = ExecuteTransactionResponseV3 {
            effects: FinalizedEffects::new_from_effects_cert(effects_cert.into()),
            events,
            input_objects,
            output_objects,
            auxiliary_data,
        };

        Ok((response, executed_locally))
    }

    // Utilize the handle_certificate_v3 validator api to request input/output objects
    #[instrument(name = "tx_orchestrator_execute_transaction_v3", level = "trace", skip_all,
                 fields(tx_digest = ?request.transaction.digest()))]
    pub async fn execute_transaction_v3(
        &self,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> Result<ExecuteTransactionResponseV3, QuorumDriverError> {
        let epoch_store = self.validator_state.load_epoch_store_one_call_per_task();

        let QuorumDriverResponse {
            effects_cert,
            events,
            input_objects,
            output_objects,
            auxiliary_data,
        } = self
            .execute_transaction_impl(&epoch_store, request, client_addr)
            .await
            .map(|(_, r)| r)?;

        Ok(ExecuteTransactionResponseV3 {
            effects: FinalizedEffects::new_from_effects_cert(effects_cert.into()),
            events,
            input_objects,
            output_objects,
            auxiliary_data,
        })
    }

    // TODO check if tx is already executed on this node.
    // Note: since EffectsCert is not stored today, we need to gather that from validators
    // (and maybe store it for caching purposes)
    pub async fn execute_transaction_impl(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> Result<(VerifiedTransaction, QuorumDriverResponse), QuorumDriverError> {
        let transaction = epoch_store
            .verify_transaction(request.transaction.clone())
            .map_err(QuorumDriverError::InvalidUserSignature)?;
        let (_in_flight_metrics_guards, good_response_metrics) = self.update_metrics(&transaction);
        let tx_digest = *transaction.digest();
        debug!(?tx_digest, "TO Received transaction execution request.");

        let (_e2e_latency_timer, _txn_finality_timer) = if transaction.contains_shared_object() {
            (
                self.metrics.request_latency_shared_obj.start_timer(),
                self.metrics
                    .wait_for_finality_latency_shared_obj
                    .start_timer(),
            )
        } else {
            (
                self.metrics.request_latency_single_writer.start_timer(),
                self.metrics
                    .wait_for_finality_latency_single_writer
                    .start_timer(),
            )
        };

        // TODO: refactor all the gauge and timer metrics with `monitored_scope`
        let wait_for_finality_gauge = self.metrics.wait_for_finality_in_flight.clone();
        wait_for_finality_gauge.inc();
        let _wait_for_finality_gauge = scopeguard::guard(wait_for_finality_gauge, |in_flight| {
            in_flight.dec();
        });

        let ticket = self
            .submit(transaction.clone(), request, client_addr)
            .await
            .map_err(|e| {
                warn!(?tx_digest, "QuorumDriverInternalError: {e:?}");
                QuorumDriverError::QuorumDriverInternalError(e)
            })?;

        let Ok(result) = timeout(WAIT_FOR_FINALITY_TIMEOUT, ticket).await else {
            debug!(?tx_digest, "Timeout waiting for transaction finality.");
            self.metrics.wait_for_finality_timeout.inc();
            return Err(QuorumDriverError::TimeoutBeforeFinality);
        };
        add_server_timing("wait_for_finality");

        drop(_txn_finality_timer);
        drop(_wait_for_finality_gauge);
        self.metrics.wait_for_finality_finished.inc();

        match result {
            Err(err) => {
                warn!(?tx_digest, "QuorumDriverInternalError: {err:?}");
                Err(QuorumDriverError::QuorumDriverInternalError(err))
            }
            Ok(Err(err)) => Err(err),
            Ok(Ok(response)) => {
                good_response_metrics.inc();
                Ok((transaction, response))
            }
        }
    }

    /// Submits the transaction to Quorum Driver for execution.
    /// Returns an awaitable Future.
    #[instrument(name = "tx_orchestrator_submit", level = "trace", skip_all)]
    async fn submit(
        &self,
        transaction: VerifiedTransaction,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> SuiResult<impl Future<Output = SuiResult<QuorumDriverResult>> + '_> {
        let tx_digest = *transaction.digest();
        let ticket = self.notifier.register_one(&tx_digest);
        // TODO(william) need to also write client adr to pending tx log below
        // so that we can re-execute with this client addr if we restart
        if self
            .pending_tx_log
            .write_pending_transaction_maybe(&transaction)
            .await?
        {
            debug!(?tx_digest, "no pending request in flight, submitting.");
            self.quorum_driver()
                .submit_transaction_no_ticket(request.clone(), client_addr)
                .await?;
        }
        // It's possible that the transaction effects is already stored in DB at this point.
        // So we also subscribe to that. If we hear from `effects_await` first, it means
        // the ticket misses the previous notification, and we want to ask quorum driver
        // to form a certificate for us again, to serve this request.
        let cache_reader = self.validator_state.get_transaction_cache_reader().clone();
        let qd = self.clone_quorum_driver();
        Ok(async move {
            let digests = [tx_digest];
            let effects_await = cache_reader.notify_read_executed_effects(&digests);
            // let-and-return necessary to satisfy borrow checker.
            #[allow(clippy::let_and_return)]
            let res = match select(ticket, effects_await.boxed()).await {
                Either::Left((quorum_driver_response, _)) => Ok(quorum_driver_response),
                Either::Right((_, unfinished_quorum_driver_task)) => {
                    debug!(
                        ?tx_digest,
                        "Effects are available in DB, use quorum driver to get a certificate"
                    );
                    qd.submit_transaction_no_ticket(request, client_addr)
                        .await?;
                    Ok(unfinished_quorum_driver_task.await)
                }
            };
            res
        })
    }

    #[instrument(name = "tx_orchestrator_wait_for_finalized_tx_executed_locally_with_timeout", level = "debug", skip_all, fields(tx_digest = ?transaction.digest()), err)]
    async fn wait_for_finalized_tx_executed_locally_with_timeout(
        validator_state: &Arc<AuthorityState>,
        transaction: &VerifiedTransaction,
        metrics: &TransactionOrchestratorMetrics,
    ) -> SuiResult {
        let tx_digest = *transaction.digest();
        metrics.local_execution_in_flight.inc();
        let _metrics_guard =
            scopeguard::guard(metrics.local_execution_in_flight.clone(), |in_flight| {
                in_flight.dec();
            });

        let _guard = if transaction.contains_shared_object() {
            metrics.local_execution_latency_shared_obj.start_timer()
        } else {
            metrics.local_execution_latency_single_writer.start_timer()
        };
        debug!(
            ?tx_digest,
            "Waiting for finalized tx to be executed locally."
        );
        match timeout(
            LOCAL_EXECUTION_TIMEOUT,
            validator_state
                .get_transaction_cache_reader()
                .notify_read_executed_effects_digests(&[tx_digest]),
        )
        .instrument(error_span!(
            "transaction_orchestrator::local_execution",
            ?tx_digest
        ))
        .await
        {
            Err(_elapsed) => {
                debug!(
                    ?tx_digest,
                    "Waiting for finalized tx to be executed locally timed out within {:?}.",
                    LOCAL_EXECUTION_TIMEOUT
                );
                metrics.local_execution_timeout.inc();
                Err(SuiError::TimeoutError)
            }
            Ok(_) => {
                metrics.local_execution_success.inc();
                Ok(())
            }
        }
    }

    // TODO: Potentially cleanup this function and pending transaction log.
    async fn loop_pending_transaction_log(
        mut effects_receiver: Receiver<QuorumDriverEffectsQueueResult>,
        pending_transaction_log: Arc<WritePathPendingTransactionLog>,
    ) {
        loop {
            match effects_receiver.recv().await {
                Ok(Ok((transaction, ..))) => {
                    let tx_digest = transaction.digest();
                    if let Err(err) = pending_transaction_log.finish_transaction(tx_digest) {
                        error!(
                            ?tx_digest,
                            "Failed to finish transaction in pending transaction log: {err}"
                        );
                    }
                }
                Ok(Err((tx_digest, _err))) => {
                    if let Err(err) = pending_transaction_log.finish_transaction(&tx_digest) {
                        error!(
                            ?tx_digest,
                            "Failed to finish transaction in pending transaction log: {err}"
                        );
                    }
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

    pub fn subscribe_to_effects_queue(&self) -> Receiver<QuorumDriverEffectsQueueResult> {
        self.quorum_driver_handler.subscribe_to_effects()
    }

    fn update_metrics(
        &'_ self,
        transaction: &VerifiedTransaction,
    ) -> (impl Drop, &'_ GenericCounter<AtomicU64>) {
        let (in_flight, good_response) = if transaction.contains_shared_object() {
            self.metrics.total_req_received_shared_object.inc();
            (
                self.metrics.req_in_flight_shared_object.clone(),
                &self.metrics.good_response_shared_object,
            )
        } else {
            self.metrics.total_req_received_single_writer.inc();
            (
                self.metrics.req_in_flight_single_writer.clone(),
                &self.metrics.good_response_single_writer,
            )
        };
        in_flight.inc();
        (
            scopeguard::guard(in_flight, |in_flight| {
                in_flight.dec();
            }),
            good_response,
        )
    }

    fn schedule_txes_in_log(
        pending_tx_log: Arc<WritePathPendingTransactionLog>,
        quorum_driver: Arc<QuorumDriverHandler<A>>,
    ) {
        spawn_logged_monitored_task!(async move {
            if std::env::var("SKIP_LOADING_FROM_PENDING_TX_LOG").is_ok() {
                info!("Skipping loading pending transactions from pending_tx_log.");
                return;
            }
            let pending_txes = pending_tx_log.load_all_pending_transactions();
            info!(
                "Recovering {} pending transactions from pending_tx_log.",
                pending_txes.len()
            );
            for (i, tx) in pending_txes.into_iter().enumerate() {
                // TODO: ideally pending_tx_log would not contain VerifiedTransaction, but that
                // requires a migration.
                let tx = tx.into_inner();
                let tx_digest = *tx.digest();
                // It's not impossible we fail to enqueue a task but that's not the end of world.
                // TODO(william) correctly extract client_addr from logs
                if let Err(err) = quorum_driver
                    .submit_transaction_no_ticket(
                        ExecuteTransactionRequestV3 {
                            transaction: tx,
                            include_events: true,
                            include_input_objects: false,
                            include_output_objects: false,
                            include_auxiliary_data: false,
                        },
                        None,
                    )
                    .await
                {
                    warn!(
                        ?tx_digest,
                        "Failed to enqueue transaction from pending_tx_log, err: {err:?}"
                    );
                } else {
                    debug!(?tx_digest, "Enqueued transaction from pending_tx_log");
                    if (i + 1) % 1000 == 0 {
                        info!("Enqueued {} transactions from pending_tx_log.", i + 1);
                    }
                }
            }
            // Transactions will be cleaned up in loop_execute_finalized_tx_locally() after they
            // produce effects.
        });
    }

    pub fn load_all_pending_transactions(&self) -> Vec<VerifiedTransaction> {
        self.pending_tx_log.load_all_pending_transactions()
    }
}

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
#[derive(Clone)]
pub struct TransactionOrchestratorMetrics {
    total_req_received_single_writer: GenericCounter<AtomicU64>,
    total_req_received_shared_object: GenericCounter<AtomicU64>,

    good_response_single_writer: GenericCounter<AtomicU64>,
    good_response_shared_object: GenericCounter<AtomicU64>,

    req_in_flight_single_writer: GenericGauge<AtomicI64>,
    req_in_flight_shared_object: GenericGauge<AtomicI64>,

    wait_for_finality_in_flight: GenericGauge<AtomicI64>,
    wait_for_finality_finished: GenericCounter<AtomicU64>,
    wait_for_finality_timeout: GenericCounter<AtomicU64>,

    local_execution_in_flight: GenericGauge<AtomicI64>,
    local_execution_success: GenericCounter<AtomicU64>,
    local_execution_timeout: GenericCounter<AtomicU64>,

    request_latency_single_writer: Histogram,
    request_latency_shared_obj: Histogram,
    wait_for_finality_latency_single_writer: Histogram,
    wait_for_finality_latency_shared_obj: Histogram,
    local_execution_latency_single_writer: Histogram,
    local_execution_latency_shared_obj: Histogram,
}

// Note that labeled-metrics are stored upfront individually
// to mitigate the perf hit by MetricsVec.
// See https://github.com/tikv/rust-prometheus/tree/master/static-metric
impl TransactionOrchestratorMetrics {
    pub fn new(registry: &Registry) -> Self {
        let total_req_received = register_int_counter_vec_with_registry!(
            "tx_orchestrator_total_req_received",
            "Total number of executions request Transaction Orchestrator receives, group by tx type",
            &["tx_type"],
            registry
        )
        .unwrap();

        let total_req_received_single_writer =
            total_req_received.with_label_values(&[TX_TYPE_SINGLE_WRITER_TX]);
        let total_req_received_shared_object =
            total_req_received.with_label_values(&[TX_TYPE_SHARED_OBJ_TX]);

        let good_response = register_int_counter_vec_with_registry!(
            "tx_orchestrator_good_response",
            "Total number of good responses Transaction Orchestrator generates, group by tx type",
            &["tx_type"],
            registry
        )
        .unwrap();

        let good_response_single_writer =
            good_response.with_label_values(&[TX_TYPE_SINGLE_WRITER_TX]);
        let good_response_shared_object = good_response.with_label_values(&[TX_TYPE_SHARED_OBJ_TX]);

        let req_in_flight = register_int_gauge_vec_with_registry!(
            "tx_orchestrator_req_in_flight",
            "Number of requests in flights Transaction Orchestrator processes, group by tx type",
            &["tx_type"],
            registry
        )
        .unwrap();

        let req_in_flight_single_writer =
            req_in_flight.with_label_values(&[TX_TYPE_SINGLE_WRITER_TX]);
        let req_in_flight_shared_object = req_in_flight.with_label_values(&[TX_TYPE_SHARED_OBJ_TX]);

        let request_latency = register_histogram_vec_with_registry!(
            "tx_orchestrator_request_latency",
            "Time spent in processing one Transaction Orchestrator request",
            &["tx_type"],
            mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
            registry,
        )
        .unwrap();
        let wait_for_finality_latency = register_histogram_vec_with_registry!(
            "tx_orchestrator_wait_for_finality_latency",
            "Time spent in waiting for one Transaction Orchestrator request gets finalized",
            &["tx_type"],
            mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
            registry,
        )
        .unwrap();
        let local_execution_latency = register_histogram_vec_with_registry!(
            "tx_orchestrator_local_execution_latency",
            "Time spent in waiting for one Transaction Orchestrator gets locally executed",
            &["tx_type"],
            mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
            registry,
        )
        .unwrap();

        Self {
            total_req_received_single_writer,
            total_req_received_shared_object,
            good_response_single_writer,
            good_response_shared_object,
            req_in_flight_single_writer,
            req_in_flight_shared_object,
            wait_for_finality_in_flight: register_int_gauge_with_registry!(
                "tx_orchestrator_wait_for_finality_in_flight",
                "Number of in flight txns Transaction Orchestrator are waiting for finality for",
                registry,
            )
            .unwrap(),
            wait_for_finality_finished: register_int_counter_with_registry!(
                "tx_orchestrator_wait_for_finality_fnished",
                "Total number of txns Transaction Orchestrator gets responses from Quorum Driver before timeout, either success or failure",
                registry,
            )
            .unwrap(),
            wait_for_finality_timeout: register_int_counter_with_registry!(
                "tx_orchestrator_wait_for_finality_timeout",
                "Total number of txns timing out in waiting for finality Transaction Orchestrator handles",
                registry,
            )
            .unwrap(),
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
            request_latency_single_writer: request_latency
                .with_label_values(&[TX_TYPE_SINGLE_WRITER_TX]),
            request_latency_shared_obj: request_latency.with_label_values(&[TX_TYPE_SHARED_OBJ_TX]),
            wait_for_finality_latency_single_writer: wait_for_finality_latency
                .with_label_values(&[TX_TYPE_SINGLE_WRITER_TX]),
            wait_for_finality_latency_shared_obj: wait_for_finality_latency
                .with_label_values(&[TX_TYPE_SHARED_OBJ_TX]),
            local_execution_latency_single_writer: local_execution_latency
                .with_label_values(&[TX_TYPE_SINGLE_WRITER_TX]),
            local_execution_latency_shared_obj: local_execution_latency
                .with_label_values(&[TX_TYPE_SHARED_OBJ_TX]),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}

#[async_trait::async_trait]
impl<A> sui_types::transaction_executor::TransactionExecutor for TransactiondOrchestrator<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    async fn execute_transaction(
        &self,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<std::net::SocketAddr>,
    ) -> Result<ExecuteTransactionResponseV3, QuorumDriverError> {
        self.execute_transaction_v3(request, client_addr).await
    }

    fn simulate_transaction(
        &self,
        transaction: TransactionData,
    ) -> Result<SimulateTransactionResult, SuiError> {
        self.validator_state.simulate_transaction(transaction)
    }
}
