// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use futures::stream::{FuturesUnordered, StreamExt};
use mysten_common::{backoff, in_antithesis};
use mysten_metrics::{TX_TYPE_SHARED_OBJ_TX, TX_TYPE_SINGLE_WRITER_TX, spawn_monitored_task};
use mysten_metrics::{add_server_timing, spawn_logged_monitored_task};
use prometheus::core::{AtomicI64, AtomicU64, GenericCounter, GenericGauge};
use prometheus::{
    HistogramVec, IntCounter, IntCounterVec, IntGauge, Registry,
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry,
};
use rand::Rng;
use sui_config::NodeConfig;
use sui_storage::write_path_pending_tx_log::WritePathPendingTransactionLog;
use sui_types::base_types::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::{ErrorCategory, SuiError, SuiErrorKind, SuiResult};
use sui_types::messages_grpc::{SubmitTxRequest, TxType};
use sui_types::quorum_driver_types::{
    EffectsFinalityInfo, ExecuteTransactionRequestType, ExecuteTransactionRequestV3,
    ExecuteTransactionResponseV3, FinalizedEffects, IsTransactionExecutedLocally,
    QuorumDriverError,
};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::transaction::{Transaction, TransactionData, VerifiedTransaction};
use sui_types::transaction_executor::{SimulateTransactionResult, TransactionChecks};
use tokio::sync::broadcast::Receiver;
use tokio::time::{Instant, sleep, timeout};
use tracing::{Instrument, debug, error_span, info, instrument, warn};

use crate::authority::AuthorityState;
use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::{AuthorityAPI, NetworkAuthorityClient};
use crate::quorum_driver::reconfig_observer::{OnsiteReconfigObserver, ReconfigObserver};
use crate::transaction_driver::{
    QuorumTransactionResponse, SubmitTransactionOptions, TransactionDriver, TransactionDriverError,
    TransactionDriverMetrics,
};

// How long to wait for local execution (including parents) before a timeout
// is returned to client.
const LOCAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(10);

// Timeout for waiting for finality for each transaction.
const WAIT_FOR_FINALITY_TIMEOUT: Duration = Duration::from_secs(90);

pub type QuorumTransactionEffectsResult =
    Result<(Transaction, QuorumTransactionResponse), (TransactionDigest, QuorumDriverError)>;

/// Transaction Orchestrator is a Node component that utilizes Transaction Driver to
/// submit transactions to validators for finality. It adds inflight deduplication,
/// waiting for local execution, recovery, early validation, and epoch change handling
/// on top of Transaction Driver.
/// This is used by node RPC service to support transaction submission and finality waiting.
pub struct TransactionOrchestrator<A: Clone> {
    inner: Arc<Inner<A>>,
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
        let metrics = Arc::new(TransactionOrchestratorMetrics::new(prometheus_registry));
        let td_metrics = Arc::new(TransactionDriverMetrics::new(prometheus_registry));
        let client_metrics = Arc::new(
            crate::validator_client_monitor::ValidatorClientMetrics::new(prometheus_registry),
        );
        let reconfig_observer = Arc::new(reconfig_observer);

        let transaction_driver = TransactionDriver::new(
            validators.clone(),
            reconfig_observer.clone(),
            td_metrics,
            Some(node_config),
            client_metrics,
        );

        let pending_tx_log = Arc::new(WritePathPendingTransactionLog::new(
            parent_path.join("fullnode_pending_transactions"),
        ));
        Inner::start_task_to_recover_txes_in_log(
            pending_tx_log.clone(),
            transaction_driver.clone(),
        );

        let td_allowed_submission_list = node_config
            .transaction_driver_config
            .as_ref()
            .map(|config| config.allowed_submission_validators.clone())
            .unwrap_or_default();

        let td_blocked_submission_list = node_config
            .transaction_driver_config
            .as_ref()
            .map(|config| config.blocked_submission_validators.clone())
            .unwrap_or_default();

        if !td_allowed_submission_list.is_empty() && !td_blocked_submission_list.is_empty() {
            panic!(
                "Both allowed and blocked submission lists are set, this is not allowed, {:?} {:?}",
                td_allowed_submission_list, td_blocked_submission_list
            );
        }

        let enable_early_validation = node_config
            .transaction_driver_config
            .as_ref()
            .map(|config| config.enable_early_validation)
            .unwrap_or(true);

        let inner = Arc::new(Inner {
            validator_state,
            pending_tx_log,
            metrics,
            transaction_driver,
            td_allowed_submission_list,
            td_blocked_submission_list,
            enable_early_validation,
        });
        Self { inner }
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

        let inner = self.inner.clone();
        let (response, mut executed_locally) = spawn_monitored_task!(
            Inner::<A>::execute_transaction_with_retry(inner, request, client_addr)
        )
        .await
        .map_err(|e| QuorumDriverError::TransactionFailed {
            category: ErrorCategory::Internal,
            details: e.to_string(),
        })??;

        if !executed_locally {
            executed_locally = if matches!(
                request_type,
                ExecuteTransactionRequestType::WaitForLocalExecution
            ) {
                let executed_locally =
                    Inner::<A>::wait_for_finalized_tx_executed_locally_with_timeout(
                        &self.inner.validator_state,
                        tx_digest,
                        tx_type,
                        &self.inner.metrics,
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

        self.inner
            .metrics
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

        let inner = self.inner.clone();
        let (response, _) = spawn_monitored_task!(Inner::<A>::execute_transaction_with_retry(
            inner,
            request,
            client_addr
        ))
        .await
        .map_err(|e| QuorumDriverError::TransactionFailed {
            category: ErrorCategory::Internal,
            details: e.to_string(),
        })??;

        self.inner
            .metrics
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

    pub fn authority_state(&self) -> &Arc<AuthorityState> {
        &self.inner.validator_state
    }

    pub fn transaction_driver(&self) -> &Arc<TransactionDriver<A>> {
        &self.inner.transaction_driver
    }

    pub fn clone_authority_aggregator(&self) -> Arc<AuthorityAggregator<A>> {
        self.inner
            .transaction_driver
            .authority_aggregator()
            .load_full()
    }

    pub fn load_all_pending_transactions_in_test(&self) -> SuiResult<Vec<VerifiedTransaction>> {
        self.inner.pending_tx_log.load_all_pending_transactions()
    }

    pub fn empty_pending_tx_log_in_test(&self) -> bool {
        self.inner.pending_tx_log.is_empty()
    }
}

struct Inner<A: Clone> {
    validator_state: Arc<AuthorityState>,
    pending_tx_log: Arc<WritePathPendingTransactionLog>,
    metrics: Arc<TransactionOrchestratorMetrics>,
    transaction_driver: Arc<TransactionDriver<A>>,
    td_allowed_submission_list: Vec<String>,
    td_blocked_submission_list: Vec<String>,
    enable_early_validation: bool,
}

impl<A> Inner<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    async fn execute_transaction_with_retry(
        inner: Arc<Inner<A>>,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> Result<(QuorumTransactionResponse, IsTransactionExecutedLocally), QuorumDriverError> {
        let result = inner
            .execute_transaction_with_effects_waiting(
                request.clone(),
                client_addr,
                /* enforce_live_input_objects */ false,
            )
            .await;

        // If the error is retriable, retry the transaction sufficiently long.
        if let Err(e) = &result
            && e.is_retriable()
        {
            spawn_monitored_task!(async move {
                inner.metrics.background_retry_started.inc();
                let backoff = backoff::ExponentialBackoff::new(
                    Duration::from_secs(1),
                    Duration::from_secs(300),
                );
                const MAX_RETRIES: usize = 10;
                for (i, delay) in backoff.enumerate() {
                    if i == MAX_RETRIES {
                        break;
                    }
                    // Start to enforce live input after 3 retries,
                    // to avoid excessively retrying transactions with non-existent input objects.
                    let result = inner
                        .execute_transaction_with_effects_waiting(
                            request.clone(),
                            client_addr,
                            i > 3,
                        )
                        .await;
                    match result {
                        Ok(_) => {
                            inner
                                .metrics
                                .background_retry_attempts
                                .with_label_values(&["success"])
                                .inc();
                            debug!(
                                "Background retry {i} for transaction {} succeeded",
                                request.transaction.digest()
                            );
                            break;
                        }
                        Err(e) => {
                            if !e.is_retriable() {
                                inner
                                    .metrics
                                    .background_retry_attempts
                                    .with_label_values(&["non-retriable"])
                                    .inc();
                                debug!(
                                    "Background retry {i} for transaction {} has non-retriable error: {e:?}. Terminating...",
                                    request.transaction.digest()
                                );
                                break;
                            }
                            inner
                                .metrics
                                .background_retry_attempts
                                .with_label_values(&["retriable"])
                                .inc();
                            debug!(
                                "Background retry {i} for transaction {} has retriable error: {e:?}. Continuing...",
                                request.transaction.digest()
                            );
                        }
                    };
                    sleep(delay).await;
                }
            });
        }

        result
    }

    /// Shared implementation for executing transactions with parallel local effects waiting
    async fn execute_transaction_with_effects_waiting(
        &self,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<SocketAddr>,
        enforce_live_input_objects: bool,
    ) -> Result<(QuorumTransactionResponse, IsTransactionExecutedLocally), QuorumDriverError> {
        let epoch_store = self.validator_state.load_epoch_store_one_call_per_task();
        let verified_transaction = epoch_store
            .verify_transaction_with_current_aliases(request.transaction.clone())
            .map_err(QuorumDriverError::InvalidUserSignature)?
            .into_tx();
        let tx_digest = *verified_transaction.digest();

        // Early validation check against local state before submission to catch non-retriable errors
        // TODO: Consider moving this check to TransactionDriver for per-retry validation
        if self.enable_early_validation
            && let Err(e) = self.validator_state.check_transaction_validity(
                &epoch_store,
                &verified_transaction,
                enforce_live_input_objects,
            )
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
        let guard =
            TransactionSubmissionGuard::new(self.pending_tx_log.clone(), &verified_transaction);
        let is_new_transaction = guard.is_new_transaction();

        let include_events = request.include_events;
        let include_input_objects = request.include_input_objects;
        let include_output_objects = request.include_output_objects;
        let include_auxiliary_data = request.include_auxiliary_data;

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

            let request = request.clone();
            let verified_transaction = verified_transaction.clone();

            let future = async move {
                if delay_ms > 0 {
                    // Add jitters to duplicated submissions.
                    sleep(Duration::from_millis(delay_ms)).await;
                }
                self.execute_transaction_impl(
                    request,
                    verified_transaction,
                    client_addr,
                    Some(finality_timeout),
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

        loop {
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
                    if let Some(e) = last_execution_error {
                        debug!("Timeout waiting for transaction finality. Last execution error: {e}");
                    } else {
                        debug!("Timeout waiting for transaction finality.");
                    }
                    self.metrics.wait_for_finality_timeout.inc();

                    // TODO: Return the last execution error.
                    break Err(QuorumDriverError::TimeoutBeforeFinality);
                }
            }
        }
    }

    #[instrument(level = "error", skip_all)]
    async fn execute_transaction_impl(
        &self,
        request: ExecuteTransactionRequestV3,
        verified_transaction: VerifiedTransaction,
        client_addr: Option<SocketAddr>,
        finality_timeout: Option<Duration>,
    ) -> Result<QuorumTransactionResponse, QuorumDriverError> {
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

        let response = self
            .submit_with_transaction_driver(
                &self.transaction_driver,
                &request,
                client_addr,
                &verified_transaction,
                good_response_metrics,
                finality_timeout,
            )
            .await?;
        let driver_type = "transaction_driver";

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
                    blocked_validators: self.td_blocked_submission_list.clone(),
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

    fn start_task_to_recover_txes_in_log(
        pending_tx_log: Arc<WritePathPendingTransactionLog>,
        transaction_driver: Arc<TransactionDriver<A>>,
    ) {
        spawn_logged_monitored_task!(async move {
            if std::env::var("SKIP_LOADING_FROM_PENDING_TX_LOG").is_ok() {
                info!("Skipping loading pending transactions from pending_tx_log.");
                return;
            }
            let pending_txes = pending_tx_log
                .load_all_pending_transactions()
                .expect("failed to load all pending transactions");
            let num_pending_txes = pending_txes.len();
            info!(
                "Recovering {} pending transactions from pending_tx_log.",
                num_pending_txes
            );
            let mut recovery = pending_txes
                .into_iter()
                .map(|tx| {
                    let pending_tx_log = pending_tx_log.clone();
                    let transaction_driver = transaction_driver.clone();
                    async move {
                        // TODO: ideally pending_tx_log would not contain VerifiedTransaction, but that
                        // requires a migration.
                        let tx = tx.into_inner();
                        let tx_digest = *tx.digest();
                        // It's not impossible we fail to enqueue a task but that's not the end of world.
                        // TODO(william) correctly extract client_addr from logs
                        if let Err(err) = transaction_driver
                            .drive_transaction(
                                SubmitTxRequest::new_transaction(tx),
                                SubmitTransactionOptions::default(),
                                Some(Duration::from_secs(60)),
                            )
                            .await
                        {
                            warn!(?tx_digest, "Failed to execute recovered transaction: {err}");
                        } else {
                            debug!(?tx_digest, "Executed recovered transaction");
                        }
                        if let Err(err) = pending_tx_log.finish_transaction(&tx_digest) {
                            warn!(
                                ?tx_digest,
                                "Failed to clean up transaction in pending log: {err}"
                            );
                        } else {
                            debug!(?tx_digest, "Cleaned up transaction in pending log");
                        }
                    }
                })
                .collect::<FuturesUnordered<_>>();

            let mut num_recovered = 0;
            while recovery.next().await.is_some() {
                num_recovered += 1;
                if num_recovered % 1000 == 0 {
                    info!(
                        "Recovered {} out of {} transactions from pending_tx_log.",
                        num_recovered, num_pending_txes
                    );
                }
            }
            info!(
                "Recovery finished. Recovered {} out of {} transactions from pending_tx_log.",
                num_recovered, num_pending_txes
            );
        });
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

    background_retry_started: IntGauge,
    background_retry_attempts: IntCounterVec,

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
            background_retry_started: register_int_gauge_with_registry!(
                "tx_orchestrator_background_retry_started",
                "Number of background retry tasks kicked off for transactions with retriable errors",
                registry,
            )
            .unwrap(),
            background_retry_attempts: register_int_counter_vec_with_registry!(
                "tx_orchestrator_background_retry_attempts",
                "Total number of background retry attempts, by status",
                &["status"],
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
        self.inner
            .validator_state
            .simulate_transaction(transaction, checks)
    }
}

/// Keeps track of inflight transactions being submitted, and helps recover transactions
/// on restart.
struct TransactionSubmissionGuard {
    pending_tx_log: Arc<WritePathPendingTransactionLog>,
    tx_digest: TransactionDigest,
    is_new_transaction: bool,
}

impl TransactionSubmissionGuard {
    pub fn new(
        pending_tx_log: Arc<WritePathPendingTransactionLog>,
        tx: &VerifiedTransaction,
    ) -> Self {
        let is_new_transaction = pending_tx_log.write_pending_transaction_maybe(tx);
        let tx_digest = *tx.digest();
        if is_new_transaction {
            debug!(?tx_digest, "Added transaction to inflight set");
        } else {
            debug!(
                ?tx_digest,
                "Transaction already being processed, no new submission will be made"
            );
        };
        Self {
            pending_tx_log,
            tx_digest,
            is_new_transaction,
        }
    }

    fn is_new_transaction(&self) -> bool {
        self.is_new_transaction
    }
}

impl Drop for TransactionSubmissionGuard {
    fn drop(&mut self) {
        if let Err(err) = self.pending_tx_log.finish_transaction(&self.tx_digest) {
            warn!(?self.tx_digest, "Failed to clean up transaction in pending log: {err}");
        } else {
            debug!(?self.tx_digest, "Cleaned up transaction in pending log");
        }
    }
}
