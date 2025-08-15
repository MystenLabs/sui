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
use crate::transaction_driver::{
    choose_transaction_driver_percentage, QuorumTransactionResponse, SubmitTransactionOptions,
    SubmitTxRequest, TransactionDriver, TransactionDriverMetrics,
};
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
use rand::Rng;
use std::net::SocketAddr;
use std::ops::Deref;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sui_config::NodeConfig;
use sui_storage::write_path_pending_tx_log::WritePathPendingTransactionLog;
use sui_types::base_types::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::{SuiError, SuiResult};
use sui_types::quorum_driver_types::{
    EffectsFinalityInfo, ExecuteTransactionRequestType, ExecuteTransactionRequestV3,
    ExecuteTransactionResponseV3, FinalizedEffects, IsTransactionExecutedLocally,
    QuorumDriverEffectsQueueResult, QuorumDriverError, QuorumDriverResult,
};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::transaction::{Transaction, TransactionData, VerifiedTransaction};
use sui_types::transaction_executor::{SimulateTransactionResult, TransactionChecks};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, error, error_span, info, instrument, warn, Instrument};

// How long to wait for local execution (including parents) before a timeout
// is returned to client.
const LOCAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(10);

// Timeout for waiting for finality for each transaction.
const WAIT_FOR_FINALITY_TIMEOUT: Duration = Duration::from_secs(30);

pub type QuorumTransactionEffectsResult =
    Result<(Transaction, QuorumTransactionResponse), (TransactionDigest, QuorumDriverError)>;
pub struct TransactionOrchestrator<A: Clone> {
    quorum_driver_handler: Arc<QuorumDriverHandler<A>>,
    validator_state: Arc<AuthorityState>,
    _local_executor_handle: JoinHandle<()>,
    pending_tx_log: Arc<WritePathPendingTransactionLog>,
    notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
    metrics: Arc<TransactionOrchestratorMetrics>,
    transaction_driver: Option<Arc<TransactionDriver<A>>>,
    td_percentage: u8,
    td_effects_broadcaster: Sender<QuorumTransactionEffectsResult>,
    _effects_merger_handle: JoinHandle<()>,
    merged_effects_broadcaster: Sender<QuorumTransactionEffectsResult>,
}

impl TransactionOrchestrator<NetworkAuthorityClient> {
    pub fn new_with_auth_aggregator(
        validators: Arc<AuthorityAggregator<NetworkAuthorityClient>>,
        validator_state: Arc<AuthorityState>,
        reconfig_channel: Receiver<SuiSystemState>,
        parent_path: &Path,
        prometheus_registry: &Registry,
        node_config: &NodeConfig,
    ) -> Self {
        let observer = OnsiteReconfigObserver::new(
            reconfig_channel,
            validator_state.get_object_cache_reader().clone(),
            validator_state.clone_committee_store(),
            validators.safe_client_metrics_base.clone(),
            validators.metrics.deref().clone(),
        );
        TransactionOrchestrator::new(
            validators,
            validator_state,
            parent_path,
            prometheus_registry,
            observer,
            node_config,
        )
    }
}

impl<A> TransactionOrchestrator<A>
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
        node_config: &NodeConfig,
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

        let epoch_store = validator_state.load_epoch_store_one_call_per_task();
        let td_percentage = if !epoch_store.protocol_config().mysticeti_fastpath() {
            0
        } else {
            choose_transaction_driver_percentage()
        };

        let transaction_driver = if td_percentage > 0 {
            let td_metrics = Arc::new(TransactionDriverMetrics::new(prometheus_registry));
            let client_metrics = Arc::new(
                crate::validator_client_monitor::ValidatorClientMetrics::new(prometheus_registry),
            );
            Some(TransactionDriver::new(
                validators.clone(),
                reconfig_observer.clone(),
                td_metrics,
                Some(node_config),
                client_metrics,
            ))
        } else {
            None
        };

        const EFFECTS_QUEUE_SIZE: usize = 20000;
        let (td_effects_broadcaster, td_effects_receiver) =
            tokio::sync::broadcast::channel(EFFECTS_QUEUE_SIZE);
        let (merged_effects_broadcaster, _) = tokio::sync::broadcast::channel(EFFECTS_QUEUE_SIZE);

        let qd_receiver = quorum_driver_handler.subscribe_to_effects();
        let merged_sender = merged_effects_broadcaster.clone();
        let _effects_merger_handle = spawn_monitored_task!(async move {
            Self::merge_effects_streams(qd_receiver, td_effects_receiver, merged_sender).await;
        });

        Self {
            quorum_driver_handler,
            validator_state,
            _local_executor_handle,
            pending_tx_log,
            notifier,
            metrics,
            transaction_driver,
            td_percentage,
            td_effects_broadcaster,
            _effects_merger_handle,
            merged_effects_broadcaster,
        }
    }
}

impl<A> TransactionOrchestrator<A>
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
        let transaction = request.transaction.clone();
        let (response, mut executed_locally) = self
            .execute_transaction_with_effects_waiting(request, client_addr)
            .await?;

        if !executed_locally {
            executed_locally = if matches!(
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
        }

        let QuorumTransactionResponse {
            effects,
            events,
            input_objects,
            output_objects,
            auxiliary_data,
        } = response;

        let response = ExecuteTransactionResponseV3 {
            effects,
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
        let (response, _) = self
            .execute_transaction_with_effects_waiting(request, client_addr)
            .await?;

        let QuorumTransactionResponse {
            effects,
            events,
            input_objects,
            output_objects,
            auxiliary_data,
        } = response;

        Ok(ExecuteTransactionResponseV3 {
            effects,
            events,
            input_objects,
            output_objects,
            auxiliary_data,
        })
    }

    /// Shared implementation for executing transactions with parallel local effects waiting
    async fn execute_transaction_with_effects_waiting(
        &self,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> Result<(QuorumTransactionResponse, IsTransactionExecutedLocally), QuorumDriverError> {
        let epoch_store = self.validator_state.load_epoch_store_one_call_per_task();
        let transaction = request.transaction.clone();
        let tx_digest = *transaction.digest();

        let include_events = request.include_events;
        let include_input_objects = request.include_input_objects;
        let include_output_objects = request.include_output_objects;

        // Track whether TD is being used for this transaction
        let using_td = Arc::new(AtomicBool::new(false));

        // Set up parallel waiting for effects
        let cache_reader = self.validator_state.get_transaction_cache_reader().clone();
        let digests = [tx_digest];
        let effects_await =
            epoch_store.within_alive_epoch(cache_reader.notify_read_executed_effects(
                "TransactionOrchestrator::notify_read_execute_transaction_with_effects_waiting",
                &digests,
            ));

        // Wait for either execution result or local effects to become available
        let mut local_effects_future = effects_await.boxed();
        let mut execution_future = self
            .execute_transaction_impl_with_td_tracking(
                &epoch_store,
                request,
                client_addr,
                using_td.clone(),
            )
            .boxed();

        // Add timeout to the overall operation
        let finality_timeout = std::env::var("WAIT_FOR_FINALITY_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .map(Duration::from_secs)
            .unwrap_or(WAIT_FOR_FINALITY_TIMEOUT);

        let mut timeout_future = tokio::time::sleep(finality_timeout).boxed();
        loop {
            tokio::select! {
                // Execution result returned
                execution_result = &mut execution_future => {
                    match execution_result {
                        Err(QuorumDriverError::PendingExecutionInTransactionOrchestrator) => {
                            debug!(
                                ?tx_digest,
                                "Transaction already being processed, disabling execution branch and waiting for local effects"
                            );
                            // Disable this branch similar to how we disable local_effects_future
                            execution_future = futures::future::pending().boxed();
                        }
                        other_result => {
                            match other_result {
                                Ok(resp) => {
                                    return Ok((
                                        resp,
                                        false
                                    ));
                                }
                                Err(e) => return Err(e),
                            }
                        }
                    }
                }
                // Local effects might be available
                local_effects_result = &mut local_effects_future => {
                    match local_effects_result {
                        Ok(effects) => {
                            debug!(
                                ?tx_digest,
                                "Effects became available while execution was running"
                            );
                            if let Some(effects) = effects.into_iter().next() {
                                let epoch = effects.executed_epoch();
                                let events = if include_events {
                                    if effects.events_digest().is_some() {
                                        Some(self.validator_state.get_transaction_events(effects.transaction_digest())
                                            .map_err(QuorumDriverError::QuorumDriverInternalError)?)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                let input_objects = include_input_objects
                                    .then(|| self.validator_state.get_transaction_input_objects(&effects))
                                    .transpose()
                                    .map_err(QuorumDriverError::QuorumDriverInternalError)?;


                                let output_objects = include_output_objects
                                    .then(|| self.validator_state.get_transaction_output_objects(&effects))
                                    .transpose()
                                    .map_err(QuorumDriverError::QuorumDriverInternalError)?;

                                let response = QuorumTransactionResponse {
                                    effects: FinalizedEffects {
                                        effects,
                                        finality_info: EffectsFinalityInfo::QuorumExecuted(epoch),
                                    },
                                    events,
                                    input_objects,
                                    output_objects,
                                    auxiliary_data: None,
                                };
                                return Ok((response, true));
                            }
                        }
                        Err(_) => {
                            warn!(?tx_digest, "Epoch terminated before effects were available");
                        }
                    };

                    // Prevent this branch from being selected again
                    local_effects_future = futures::future::pending().boxed();
                }
                // A timeout has occurred while waiting for finality
                _ = &mut timeout_future => {
                    debug!(?tx_digest, "Timeout waiting for transaction finality.");
                    self.metrics.wait_for_finality_timeout.inc();

                    // Clean up transaction from WAL log only for TD submissions
                    // For QD submissions, the cleanup happens in loop_pending_transaction_log
                    if using_td.load(Ordering::Acquire) {
                        debug!(?tx_digest, "Cleaning up TD transaction from WAL due to timeout");
                        if let Err(err) = self.pending_tx_log.finish_transaction(&tx_digest) {
                            warn!(
                                ?tx_digest,
                                "Failed to finish TD transaction in pending transaction log: {err}"
                            );
                        }
                    }

                    return Err(QuorumDriverError::TimeoutBeforeFinality);
                }
            }
        }
    }

    pub async fn execute_transaction_impl(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> Result<QuorumTransactionResponse, QuorumDriverError> {
        // Call the tracking version with a dummy AtomicBool since this public method
        // doesn't need to track TD usage
        self.execute_transaction_impl_with_td_tracking(
            epoch_store,
            request,
            client_addr,
            Arc::new(AtomicBool::new(false)),
        )
        .await
    }

    async fn execute_transaction_impl_with_td_tracking(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<SocketAddr>,
        using_td: Arc<AtomicBool>,
    ) -> Result<QuorumTransactionResponse, QuorumDriverError> {
        let verified_transaction = epoch_store
            .verify_transaction(request.transaction.clone())
            .map_err(QuorumDriverError::InvalidUserSignature)?;
        let (_in_flight_metrics_guards, good_response_metrics) =
            self.update_metrics(&request.transaction);
        let tx_digest = *verified_transaction.digest();
        debug!(?tx_digest, "TO Received transaction execution request.");

        let (_e2e_latency_timer, _txn_finality_timer) = if request.transaction.is_consensus_tx() {
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

        // Check if TransactionDriver should be used for submission
        if let Some(td) = &self.transaction_driver {
            let random_value = rand::thread_rng().gen_range(1..=100);
            if random_value <= self.td_percentage {
                // Mark that we're using TD before submitting
                using_td.store(true, Ordering::Release);

                let td_response = self
                    .submit_with_transaction_driver(
                        td,
                        &request,
                        client_addr,
                        &verified_transaction,
                        good_response_metrics,
                        tx_digest,
                    )
                    .await;

                add_server_timing("[TransactionDriver] wait_for_finality");

                drop(_txn_finality_timer);
                drop(_wait_for_finality_gauge);
                self.metrics.wait_for_finality_finished.inc();

                return td_response;
            }
        }

        // Submit transaction through QuorumDriver (using_td remains false)
        let result = self
            .submit_with_quorum_driver(
                epoch_store.clone(),
                verified_transaction.clone(),
                request,
                client_addr,
            )
            .await
            .map_err(|e| {
                warn!(?tx_digest, "QuorumDriverInternalError: {e:?}");
                QuorumDriverError::QuorumDriverInternalError(e)
            })?
            .await;

        add_server_timing("[QuorumDriver] wait_for_finality");

        drop(_txn_finality_timer);
        drop(_wait_for_finality_gauge);
        self.metrics.wait_for_finality_finished.inc();

        match result {
            Err(err) => {
                warn!(?tx_digest, "QuorumDriverInternalError: {err:?}");
                Err(QuorumDriverError::QuorumDriverInternalError(err))
            }
            Ok(Err(err)) => Err(err),
            Ok(Ok(qd_response)) => {
                good_response_metrics.inc();
                let effects_cert = qd_response.effects_cert;

                let quorum_response = QuorumTransactionResponse {
                    effects: FinalizedEffects::new_from_effects_cert(effects_cert.into()),
                    events: qd_response.events,
                    input_objects: qd_response.input_objects,
                    output_objects: qd_response.output_objects,
                    auxiliary_data: qd_response.auxiliary_data,
                };
                Ok(quorum_response)
            }
        }
    }

    async fn submit_with_transaction_driver(
        &self,
        td: &Arc<TransactionDriver<A>>,
        request: &ExecuteTransactionRequestV3,
        client_addr: Option<SocketAddr>,
        verified_transaction: &VerifiedTransaction,
        good_response_metrics: &GenericCounter<AtomicU64>,
        tx_digest: TransactionDigest,
    ) -> Result<QuorumTransactionResponse, QuorumDriverError> {
        debug!("Using TransactionDriver for transaction {:?}", tx_digest);
        // Add transaction to WAL log for TransactionDriver path
        let is_new_transaction = self
            .pending_tx_log
            .write_pending_transaction_maybe(verified_transaction)
            .await
            .map_err(|e| {
                warn!(?tx_digest, "QuorumDriverInternalError: {e:?}");
                QuorumDriverError::QuorumDriverInternalError(e)
            })?;

        if !is_new_transaction {
            debug!(
                ?tx_digest,
                "Transaction already in pending_tx_log, returning PendingExecutionInTransactionOrchestrator"
            );
            // Return the special error to signal that we should wait for effects
            return Err(QuorumDriverError::PendingExecutionInTransactionOrchestrator);
        }

        debug!(
            ?tx_digest,
            "Added transaction to WAL log for TransactionDriver"
        );

        let td_response = td
            .drive_transaction(
                SubmitTxRequest {
                    transaction: request.transaction.clone(),
                },
                SubmitTransactionOptions {
                    forwarded_client_addr: client_addr,
                },
            )
            .await
            .map_err(|e| QuorumDriverError::TransactionFailed {
                retriable: e.is_retriable(),
                details: e.to_string(),
            });

        // Broadcast TD effects to the channel
        let effects_queue_result = match &td_response {
            Ok(qtx_response) => Ok((verified_transaction.clone().into_inner(), qtx_response)),
            Err(e) => Err((tx_digest, e.clone())),
        };

        // Send to TD effects broadcast channel
        if let Err(err) = self.td_effects_broadcaster.send(
            effects_queue_result.map(|(transaction, response)| (transaction, response.clone())),
        ) {
            debug!(?tx_digest, "No subscriber found for TD effects: {}", err);
        }

        // Clean up transaction from WAL log
        if let Err(err) = self.pending_tx_log.finish_transaction(&tx_digest) {
            warn!(
                ?tx_digest,
                "Failed to finish transaction in pending transaction log: {err}"
            );
        }

        match td_response {
            Err(e) => {
                warn!(?tx_digest, "{e:?}");
                Err(e)
            }
            Ok(quorum_transaction_response) => {
                good_response_metrics.inc();
                Ok(quorum_transaction_response)
            }
        }
    }

    /// Submits the transaction to Quorum Driver for execution.
    /// Returns an awaitable Future.
    #[instrument(name = "tx_orchestrator_submit", level = "trace", skip_all)]
    async fn submit_with_quorum_driver(
        &self,
        epoch_store: Arc<AuthorityPerEpochStore>,
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
            let effects_await =
                epoch_store.within_alive_epoch(cache_reader.notify_read_executed_effects(
                    "TransactionOrchestrator::notify_read_submit_with_qd",
                    &digests,
                ));
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
        transaction: &Transaction,
        metrics: &TransactionOrchestratorMetrics,
    ) -> SuiResult {
        let tx_digest = *transaction.digest();
        metrics.local_execution_in_flight.inc();
        let _metrics_guard =
            scopeguard::guard(metrics.local_execution_in_flight.clone(), |in_flight| {
                in_flight.dec();
            });

        let _guard = if transaction.is_consensus_tx() {
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
                .notify_read_executed_effects_digests(
                    "TransactionOrchestrator::notify_read_wait_for_local_execution",
                    &[tx_digest],
                ),
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

    pub fn subscribe_to_effects_queue(&self) -> Receiver<QuorumTransactionEffectsResult> {
        self.merged_effects_broadcaster.subscribe()
    }

    /// Merges QD and TD effects streams into a single broadcast channel
    async fn merge_effects_streams(
        mut qd_receiver: Receiver<QuorumDriverEffectsQueueResult>,
        mut td_receiver: Receiver<QuorumTransactionEffectsResult>,
        merged_sender: Sender<QuorumTransactionEffectsResult>,
    ) {
        loop {
            tokio::select! {
                qd_result = qd_receiver.recv() => {
                    match qd_result {
                        Ok(effects) => {
                            let _ = merged_sender.send(convert_to_quorum_transaction_effects_result(effects));
                        }
                        Err(RecvError::Closed) => {
                            error!("QD effects channel closed unexpectedly");
                            break;
                        }
                        Err(RecvError::Lagged(n)) => {
                            warn!("QD effects receiver lagged by {} messages", n);
                        }
                    }
                }
                td_result = td_receiver.recv() => {
                    match td_result {
                        Ok(effects) => {
                            let _ = merged_sender.send(effects);
                        }
                        Err(RecvError::Closed) => {
                            error!("TD effects channel closed unexpectedly");
                            break;
                        }
                        Err(RecvError::Lagged(n)) => {
                            warn!("TD effects receiver lagged by {} messages", n);
                        }
                    }
                }
            }
        }
    }

    fn update_metrics(
        &'_ self,
        transaction: &Transaction,
    ) -> (impl Drop, &'_ GenericCounter<AtomicU64>) {
        let (in_flight, good_response) = if transaction.is_consensus_tx() {
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
            let pending_txes = pending_tx_log
                .load_all_pending_transactions()
                .expect("failed to load all pending transactions");
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

    pub fn load_all_pending_transactions(&self) -> SuiResult<Vec<VerifiedTransaction>> {
        self.pending_tx_log.load_all_pending_transactions()
    }
}

// Convert from QuorumDriverEffectsQueueResult to QuorumTransactionEffectsResult
fn convert_to_quorum_transaction_effects_result(
    quorum_driver_effects_queue_result: QuorumDriverEffectsQueueResult,
) -> QuorumTransactionEffectsResult {
    match quorum_driver_effects_queue_result {
        Ok((transaction, effects)) => Ok((
            transaction,
            QuorumTransactionResponse {
                effects: FinalizedEffects::new_from_effects_cert(effects.effects_cert.into()),
                events: effects.events,
                input_objects: effects.input_objects,
                output_objects: effects.output_objects,
                auxiliary_data: effects.auxiliary_data,
            },
        )),
        Err((tx_digest, err)) => Err((tx_digest, err)),
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
impl<A> sui_types::transaction_executor::TransactionExecutor for TransactionOrchestrator<A>
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
        checks: TransactionChecks,
    ) -> Result<SimulateTransactionResult, SuiError> {
        self.validator_state
            .simulate_transaction(transaction, checks)
    }
}
