// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
Transaction Orchestrator is a Node component that utilizes Quorum Driver to
submit transactions to validators for finality, and proactively executes
finalized transactions locally, when possible.
*/

use std::net::SocketAddr;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::FutureExt;
use futures::future::{Either, Future, select};
use futures::stream::{FuturesUnordered, StreamExt};
use mysten_common::in_antithesis;
use mysten_common::sync::notify_read::NotifyRead;
use mysten_metrics::{TX_TYPE_SHARED_OBJ_TX, TX_TYPE_SINGLE_WRITER_TX};
use mysten_metrics::{add_server_timing, spawn_logged_monitored_task, spawn_monitored_task};
use prometheus::core::{AtomicI64, AtomicU64, GenericCounter, GenericGauge};
use prometheus::{
    HistogramVec, IntCounter, IntCounterVec, Registry, register_histogram_vec_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry,
};
use rand::Rng;
use sui_config::NodeConfig;
use sui_protocol_config::Chain;
use sui_storage::write_path_pending_tx_log::WritePathPendingTransactionLog;
use sui_types::base_types::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::{SuiError, SuiErrorKind, SuiResult};
use sui_types::messages_grpc::{SubmitTxRequest, TxType};
use sui_types::quorum_driver_types::{
    EffectsFinalityInfo, ExecuteTransactionRequestType, ExecuteTransactionRequestV3,
    ExecuteTransactionResponseV3, FinalizedEffects, IsTransactionExecutedLocally,
    QuorumDriverEffectsQueueResult, QuorumDriverError, QuorumDriverResult,
};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::transaction::{Transaction, TransactionData, VerifiedTransaction};
use sui_types::transaction_executor::{SimulateTransactionResult, TransactionChecks};
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep, timeout};
use tracing::{Instrument, debug, error, error_span, info, instrument, warn};

use crate::authority::AuthorityState;
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::{AuthorityAPI, NetworkAuthorityClient};
use crate::quorum_driver::reconfig_observer::{OnsiteReconfigObserver, ReconfigObserver};
use crate::quorum_driver::{QuorumDriverHandler, QuorumDriverHandlerBuilder, QuorumDriverMetrics};
use crate::transaction_driver::{
    QuorumTransactionResponse, SubmitTransactionOptions, TransactionDriver, TransactionDriverError,
    TransactionDriverMetrics, choose_transaction_driver_percentage,
};

// How long to wait for local execution (including parents) before a timeout
// is returned to client.
const LOCAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(10);

// Timeout for waiting for finality for each transaction.
const WAIT_FOR_FINALITY_TIMEOUT: Duration = Duration::from_secs(90);

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
    td_allowed_submission_list: Vec<String>,
    enable_early_validation: bool,
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
            choose_transaction_driver_percentage(Some(epoch_store.get_chain_identifier()))
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

        let td_allowed_submission_list = node_config
            .transaction_driver_config
            .as_ref()
            .map(|config| config.allowed_submission_validators.clone())
            .unwrap_or_default();

        let enable_early_validation = node_config
            .transaction_driver_config
            .as_ref()
            .map(|config| config.enable_early_validation)
            .unwrap_or(true);

        Self {
            quorum_driver_handler,
            validator_state,
            _local_executor_handle,
            pending_tx_log,
            notifier,
            metrics,
            transaction_driver,
            td_percentage,
            td_allowed_submission_list,
            enable_early_validation,
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
    ))]
    pub async fn execute_transaction_block(
        &self,
        request: ExecuteTransactionRequestV3,
        request_type: ExecuteTransactionRequestType,
        client_addr: Option<SocketAddr>,
    ) -> Result<(ExecuteTransactionResponseV3, IsTransactionExecutedLocally), QuorumDriverError>
    {
        let timer = Instant::now();
        let tx_type = if request.transaction.is_consensus_tx() {
            TxType::SharedObject
        } else {
            TxType::SingleWriter
        };
        let tx_digest = *request.transaction.digest();

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
                    tx_digest,
                    tx_type,
                    &self.metrics,
                )
                .await
                .is_ok();
                add_server_timing("local_execution done");
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

        self.metrics
            .request_latency
            .with_label_values(&[
                tx_type.as_str(),
                "execute_transaction_block",
                executed_locally.to_string().as_str(),
            ])
            .observe(timer.elapsed().as_secs_f64());

        Ok((response, executed_locally))
    }

    // Utilize the handle_certificate_v3 validator api to request input/output objects
    #[instrument(name = "tx_orchestrator_execute_transaction_v3", level = "debug", skip_all,
                 fields(tx_digest = ?request.transaction.digest()))]
    pub async fn execute_transaction_v3(
        &self,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> Result<ExecuteTransactionResponseV3, QuorumDriverError> {
        let timer = Instant::now();
        let tx_type = if request.transaction.is_consensus_tx() {
            TxType::SharedObject
        } else {
            TxType::SingleWriter
        };

        let (response, _) = self
            .execute_transaction_with_effects_waiting(request, client_addr)
            .await?;

        self.metrics
            .request_latency
            .with_label_values(&[tx_type.as_str(), "execute_transaction_v3", "false"])
            .observe(timer.elapsed().as_secs_f64());

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
        let verified_transaction = epoch_store
            .verify_transaction(request.transaction.clone())
            .map_err(QuorumDriverError::InvalidUserSignature)?;
        let tx_digest = *verified_transaction.digest();

        // Early validation check against local state before submission to catch non-retriable errors
        // TODO: Consider moving this check to TransactionDriver for per-retry validation
        if self.enable_early_validation
            && let Err(e) = self
                .validator_state
                .check_transaction_validity(&epoch_store, &verified_transaction)
        {
            let error_category = e.categorize();
            if !error_category.is_submission_retriable() {
                // Skip early validation rejection if transaction has already been executed (allows retries to return cached results)
                if !self.validator_state.is_tx_already_executed(&tx_digest) {
                    self.metrics
                        .early_validation_rejections
                        .with_label_values(&[e.to_variant_name()])
                        .inc();
                    debug!(
                        error = ?e,
                        "Transaction rejected during early validation"
                    );

                    return Err(QuorumDriverError::TransactionFailed {
                        category: error_category,
                        details: e.to_string(),
                    });
                }
            }
        }

        // Add transaction to WAL log.
        let is_new_transaction = self
            .pending_tx_log
            .write_pending_transaction_maybe(&verified_transaction)
            .await
            .map_err(|e| {
                warn!("QuorumDriverInternalError: {e:?}");
                QuorumDriverError::QuorumDriverInternalError(e)
            })?;
        if is_new_transaction {
            debug!("Added transaction to WAL log for TransactionDriver");
        } else {
            debug!("Transaction already in pending_tx_log");
        }

        let include_events = request.include_events;
        let include_input_objects = request.include_input_objects;
        let include_output_objects = request.include_output_objects;
        let include_auxiliary_data = request.include_auxiliary_data;

        // Track whether TD is being used for this transaction
        let using_td = Arc::new(AtomicBool::new(false));

        let finality_timeout = std::env::var("WAIT_FOR_FINALITY_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .map(Duration::from_secs)
            .unwrap_or(WAIT_FOR_FINALITY_TIMEOUT);

        let num_submissions = if !is_new_transaction {
            // No need to submit when the transaction is already being processed.
            0
        } else if cfg!(msim) || in_antithesis() {
            // Allow duplicated submissions in tests.
            let r = rand::thread_rng().gen_range(1..=100);
            let n = if r <= 10 {
                3
            } else if r <= 30 {
                2
            } else {
                1
            };
            if n > 1 {
                debug!("Making {n} execution calls");
            }
            n
        } else {
            1
        };

        // Wait for one of the execution futures to succeed, or all of them to fail.
        let mut execution_futures = FuturesUnordered::new();
        for i in 0..num_submissions {
            // Generate jitter values outside the async block
            let should_delay = i > 0 && rand::thread_rng().gen_bool(0.8);
            let delay_ms = if should_delay {
                rand::thread_rng().gen_range(100..=500)
            } else {
                0
            };

            let epoch_store = epoch_store.clone();
            let request = request.clone();
            let verified_transaction = verified_transaction.clone();
            let using_td = using_td.clone();

            let future = async move {
                if delay_ms > 0 {
                    // Add jitters to duplicated submissions.
                    sleep(Duration::from_millis(delay_ms)).await;
                }
                self.execute_transaction_impl(
                    &epoch_store,
                    request,
                    verified_transaction,
                    client_addr,
                    Some(finality_timeout),
                    using_td,
                )
                .await
            }
            .boxed();
            execution_futures.push(future);
        }

        // Track the last execution error.
        let mut last_execution_error: Option<QuorumDriverError> = None;

        // Wait for execution result outside of this call to become available.
        let digests = [tx_digest];
        let mut local_effects_future = epoch_store
            .within_alive_epoch(
                self.validator_state
                    .get_transaction_cache_reader()
                    .notify_read_executed_effects(
                    "TransactionOrchestrator::notify_read_execute_transaction_with_effects_waiting",
                    &digests,
                ),
            )
            .boxed();

        // Wait for execution timeout.
        let mut timeout_future = tokio::time::sleep(finality_timeout).boxed();

        let result = loop {
            tokio::select! {
                biased;

                // Local effects might be available
                local_effects_result = &mut local_effects_future => {
                    match local_effects_result {
                        Ok(effects) => {
                            debug!(
                                "Effects became available while execution was running"
                            );
                            if let Some(effects) = effects.into_iter().next() {
                                self.metrics.concurrent_execution.inc();
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
                                break Ok((response, true));
                            }
                        }
                        Err(_) => {
                            warn!("Epoch terminated before effects were available");
                        }
                    };

                    // Prevent this branch from being selected again
                    local_effects_future = futures::future::pending().boxed();
                }

                // This branch is disabled if execution_futures is empty.
                Some(result) = execution_futures.next() => {
                    match result {
                        Ok(resp) => {
                            // First success gets returned.
                            debug!("Execution succeeded, returning response");
                            let QuorumTransactionResponse {
                                effects,
                                events,
                                input_objects,
                                output_objects,
                                auxiliary_data,
                            } = resp;
                            // Filter fields based on request flags.
                            let resp = QuorumTransactionResponse {
                                effects,
                                events: if include_events { events } else { None },
                                input_objects: if include_input_objects { input_objects } else { None },
                                output_objects: if include_output_objects { output_objects } else { None },
                                auxiliary_data: if include_auxiliary_data { auxiliary_data } else { None },
                            };
                            break Ok((resp, false));
                        }
                        Err(QuorumDriverError::PendingExecutionInTransactionOrchestrator) => {
                            debug!(
                                "Transaction is already being processed"
                            );
                            // Avoid overriding errors with transaction already being processed.
                            if last_execution_error.is_none() {
                                last_execution_error = Some(QuorumDriverError::PendingExecutionInTransactionOrchestrator);
                            }
                        }
                        Err(e) => {
                            debug!(?e, "Execution attempt failed, wait for other attempts");
                            last_execution_error = Some(e);
                        }
                    };

                    // Last error must have been recorded.
                    if execution_futures.is_empty() {
                        break Err(last_execution_error.unwrap());
                    }
                }

                // A timeout has occurred while waiting for finality
                _ = &mut timeout_future => {
                    debug!("Timeout waiting for transaction finality.");
                    self.metrics.wait_for_finality_timeout.inc();

                    // Clean up transaction from WAL log only for TD submissions
                    // For QD submissions, the cleanup happens in loop_pending_transaction_log
                    if using_td.load(Ordering::Acquire) {
                        debug!("Cleaning up TD transaction from WAL due to timeout");
                        if let Err(err) = self.pending_tx_log.finish_transaction(&tx_digest) {
                            warn!(
                                "Failed to finish TD transaction in pending transaction log: {err}"
                            );
                        }
                    }

                    break Err(QuorumDriverError::TimeoutBeforeFinality);
                }
            }
        };

        // Clean up transaction from WAL log
        if let Err(err) = self.pending_tx_log.finish_transaction(&tx_digest) {
            warn!("Failed to finish transaction in pending transaction log: {err}");
        }

        result
    }

    #[instrument(level = "error", skip_all)]
    async fn execute_transaction_impl(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        request: ExecuteTransactionRequestV3,
        verified_transaction: VerifiedTransaction,
        client_addr: Option<SocketAddr>,
        finality_timeout: Option<Duration>,
        using_td: Arc<AtomicBool>,
    ) -> Result<QuorumTransactionResponse, QuorumDriverError> {
        let tx_digest = *verified_transaction.digest();
        debug!("TO Received transaction execution request.");

        let timer = Instant::now();
        let tx_type = if verified_transaction.is_consensus_tx() {
            TxType::SharedObject
        } else {
            TxType::SingleWriter
        };

        let (_in_flight_metrics_guards, good_response_metrics) =
            self.update_metrics(&request.transaction);

        // TODO: refactor all the gauge and timer metrics with `monitored_scope`
        let wait_for_finality_gauge = self.metrics.wait_for_finality_in_flight.clone();
        wait_for_finality_gauge.inc();
        let _wait_for_finality_gauge = scopeguard::guard(wait_for_finality_gauge, |in_flight| {
            in_flight.dec();
        });

        // Select TransactionDriver or QuorumDriver for submission.
        let (response, driver_type) = if self.transaction_driver.is_some()
            && self.should_use_transaction_driver(epoch_store, tx_digest)
        {
            // Mark that we're using TD before submitting.
            using_td.store(true, Ordering::Release);

            (
                self.submit_with_transaction_driver(
                    self.transaction_driver.as_ref().unwrap(),
                    &request,
                    client_addr,
                    &verified_transaction,
                    good_response_metrics,
                    finality_timeout,
                )
                .await?,
                "transaction_driver",
            )
        } else {
            // Submit transaction through QuorumDriver.
            using_td.store(false, Ordering::Release);

            let resp = self
                .submit_with_quorum_driver(
                    epoch_store.clone(),
                    verified_transaction.clone(),
                    request,
                    client_addr,
                )
                .await
                .map_err(|e| {
                    warn!("QuorumDriverInternalError: {e:?}");
                    QuorumDriverError::QuorumDriverInternalError(e)
                })?
                .await
                .map_err(|e| {
                    warn!("QuorumDriverInternalError: {e:?}");
                    QuorumDriverError::QuorumDriverInternalError(e)
                })??;

            (
                QuorumTransactionResponse {
                    effects: FinalizedEffects::new_from_effects_cert(resp.effects_cert.into()),
                    events: resp.events,
                    input_objects: resp.input_objects,
                    output_objects: resp.output_objects,
                    auxiliary_data: resp.auxiliary_data,
                },
                "quorum_driver",
            )
        };

        add_server_timing("wait_for_finality done");

        self.metrics.wait_for_finality_finished.inc();

        let elapsed = timer.elapsed().as_secs_f64();
        self.metrics
            .settlement_finality_latency
            .with_label_values(&[tx_type.as_str(), driver_type])
            .observe(elapsed);
        good_response_metrics.inc();

        Ok(response)
    }

    #[instrument(level = "error", skip_all, err(level = "info"))]
    async fn submit_with_transaction_driver(
        &self,
        td: &Arc<TransactionDriver<A>>,
        request: &ExecuteTransactionRequestV3,
        client_addr: Option<SocketAddr>,
        verified_transaction: &VerifiedTransaction,
        good_response_metrics: &GenericCounter<AtomicU64>,
        timeout_duration: Option<Duration>,
    ) -> Result<QuorumTransactionResponse, QuorumDriverError> {
        let tx_digest = *verified_transaction.digest();
        debug!("Using TransactionDriver for transaction {:?}", tx_digest);

        let td_response = td
            .drive_transaction(
                SubmitTxRequest::new_transaction(request.transaction.clone()),
                SubmitTransactionOptions {
                    forwarded_client_addr: client_addr,
                    allowed_validators: self.td_allowed_submission_list.clone(),
                },
                timeout_duration,
            )
            .await
            .map_err(|e| match e {
                TransactionDriverError::TimeoutWithLastRetriableError {
                    last_error,
                    attempts,
                    timeout,
                } => QuorumDriverError::TimeoutBeforeFinalityWithErrors {
                    last_error: last_error.map(|e| e.to_string()).unwrap_or_default(),
                    attempts,
                    timeout,
                },
                other => QuorumDriverError::TransactionFailed {
                    category: other.categorize(),
                    details: other.to_string(),
                },
            });

        match td_response {
            Err(e) => {
                warn!("TransactionDriver error: {e:?}");
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
        self.quorum_driver()
            .submit_transaction_no_ticket(request.clone(), client_addr)
            .await?;

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
                    debug!("Effects are available in DB, use quorum driver to get a certificate");
                    qd.submit_transaction_no_ticket(request, client_addr)
                        .await?;
                    Ok(unfinished_quorum_driver_task.await)
                }
            };
            res
        })
    }

    #[instrument(
        name = "tx_orchestrator_wait_for_finalized_tx_executed_locally_with_timeout",
        level = "debug",
        skip_all,
        err(level = "info")
    )]
    async fn wait_for_finalized_tx_executed_locally_with_timeout(
        validator_state: &Arc<AuthorityState>,
        tx_digest: TransactionDigest,
        tx_type: TxType,
        metrics: &TransactionOrchestratorMetrics,
    ) -> SuiResult {
        metrics.local_execution_in_flight.inc();
        let _metrics_guard =
            scopeguard::guard(metrics.local_execution_in_flight.clone(), |in_flight| {
                in_flight.dec();
            });

        let _latency_guard = metrics
            .local_execution_latency
            .with_label_values(&[tx_type.as_str()])
            .start_timer();
        debug!("Waiting for finalized tx to be executed locally.");
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
                    "Waiting for finalized tx to be executed locally timed out within {:?}.",
                    LOCAL_EXECUTION_TIMEOUT
                );
                metrics.local_execution_timeout.inc();
                Err(SuiErrorKind::TimeoutError.into())
            }
            Ok(_) => {
                metrics.local_execution_success.inc();
                Ok(())
            }
        }
    }

    fn should_use_transaction_driver(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        tx_digest: TransactionDigest,
    ) -> bool {
        const MAX_PERCENTAGE: u8 = 100;
        let unknown_network = epoch_store.get_chain() == Chain::Unknown;
        let v = if unknown_network {
            rand::thread_rng().gen_range(1..=MAX_PERCENTAGE)
        } else {
            let v = u32::from_le_bytes(tx_digest.inner()[..4].try_into().unwrap());
            (v % (MAX_PERCENTAGE as u32) + 1) as u8
        };
        debug!(
            "Choosing whether to use transaction driver: {} vs {}",
            v, self.td_percentage
        );
        v <= self.td_percentage
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

    fn update_metrics<'a>(
        &'a self,
        transaction: &Transaction,
    ) -> (impl Drop + use<A>, &'a GenericCounter<AtomicU64>) {
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
                    debug!("Enqueued transaction from pending_tx_log");
                    if (i + 1) % 1000 == 0 {
                        info!("Enqueued {} transactions from pending_tx_log.", i + 1);
                    }
                }
            }
            // Transactions will be cleaned up in loop_execute_finalized_tx_locally() after they
            // produce effects.
        });
    }

    pub fn load_all_pending_transactions_in_test(&self) -> SuiResult<Vec<VerifiedTransaction>> {
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

    concurrent_execution: IntCounter,

    early_validation_rejections: IntCounterVec,

    request_latency: HistogramVec,
    local_execution_latency: HistogramVec,
    settlement_finality_latency: HistogramVec,
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
            concurrent_execution: register_int_counter_with_registry!(
                "tx_orchestrator_concurrent_execution",
                "Total number of concurrent execution where effects are available locally finishing driving the transaction to finality",
                registry,
            )
            .unwrap(),
            early_validation_rejections: register_int_counter_vec_with_registry!(
                "tx_orchestrator_early_validation_rejections",
                "Total number of transactions rejected during early validation before submission, by reason",
                &["reason"],
                registry,
            )
            .unwrap(),
            request_latency: register_histogram_vec_with_registry!(
                "tx_orchestrator_request_latency",
                "Time spent in processing one Transaction Orchestrator request",
                &["tx_type", "route", "wait_for_local_execution"],
                mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            local_execution_latency: register_histogram_vec_with_registry!(
                "tx_orchestrator_local_execution_latency",
                "Time spent in waiting for one Transaction Orchestrator gets locally executed",
                &["tx_type"],
                mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            settlement_finality_latency: register_histogram_vec_with_registry!(
                "tx_orchestrator_settlement_finality_latency",
                "Time spent in waiting for one Transaction Orchestrator gets settled and finalized",
                &["tx_type", "driver_type"],
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
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
