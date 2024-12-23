// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod metrics;
pub use metrics::*;

pub mod reconfig_observer;

use arc_swap::ArcSwap;
use std::fmt::{Debug, Formatter};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::{Committee, EpochId};
use sui_types::messages_grpc::HandleCertificateRequestV3;
use sui_types::quorum_driver_types::{
    ExecuteTransactionRequestV3, QuorumDriverEffectsQueueResult, QuorumDriverError,
    QuorumDriverResponse, QuorumDriverResult,
};
use tap::TapFallible;
use tokio::sync::Semaphore;
use tokio::time::{sleep_until, Instant};

use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, instrument, trace_span, warn};

use crate::authority_aggregator::{
    AggregatorProcessCertificateError, AggregatorProcessTransactionError, AuthorityAggregator,
    ProcessTransactionResult,
};
use crate::authority_client::AuthorityAPI;
use mysten_common::sync::notify_read::{NotifyRead, Registration};
use mysten_metrics::{
    spawn_monitored_task, GaugeGuard, TX_TYPE_SHARED_OBJ_TX, TX_TYPE_SINGLE_WRITER_TX,
};
use std::fmt::Write;
use sui_macros::fail_point;
use sui_types::error::{SuiError, SuiResult};
use sui_types::transaction::{CertifiedTransaction, Transaction};

use self::reconfig_observer::ReconfigObserver;

#[cfg(test)]
mod tests;

const TASK_QUEUE_SIZE: usize = 2000;
const EFFECTS_QUEUE_SIZE: usize = 10000;
const TX_MAX_RETRY_TIMES: u32 = 10;

pub trait AuthorityAggregatorUpdatable<A: Clone>: Send + Sync + 'static {
    fn epoch(&self) -> EpochId;
    fn authority_aggregator(&self) -> Arc<AuthorityAggregator<A>>;
    fn update_authority_aggregator(&self, new_authorities: Arc<AuthorityAggregator<A>>);
}

#[derive(Clone)]
pub struct QuorumDriverTask {
    pub request: ExecuteTransactionRequestV3,
    pub tx_cert: Option<CertifiedTransaction>,
    pub retry_times: u32,
    pub next_retry_after: Instant,
    pub client_addr: Option<SocketAddr>,
    pub trace_span: Option<tracing::Span>,
}

impl Debug for QuorumDriverTask {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        write!(writer, "tx_digest={:?} ", self.request.transaction.digest())?;
        write!(writer, "has_tx_cert={} ", self.tx_cert.is_some())?;
        write!(writer, "retry_times={} ", self.retry_times)?;
        write!(writer, "next_retry_after={:?} ", self.next_retry_after)?;
        write!(f, "{}", writer)
    }
}

pub struct QuorumDriver<A: Clone> {
    validators: ArcSwap<AuthorityAggregator<A>>,
    task_sender: Sender<QuorumDriverTask>,
    effects_subscribe_sender: tokio::sync::broadcast::Sender<QuorumDriverEffectsQueueResult>,
    notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
    metrics: Arc<QuorumDriverMetrics>,
    max_retry_times: u32,
}

impl<A: Clone> QuorumDriver<A> {
    pub(crate) fn new(
        validators: ArcSwap<AuthorityAggregator<A>>,
        task_sender: Sender<QuorumDriverTask>,
        effects_subscribe_sender: tokio::sync::broadcast::Sender<QuorumDriverEffectsQueueResult>,
        notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
        metrics: Arc<QuorumDriverMetrics>,
        max_retry_times: u32,
    ) -> Self {
        Self {
            validators,
            task_sender,
            effects_subscribe_sender,
            notifier,
            metrics,
            max_retry_times,
        }
    }

    pub fn authority_aggregator(&self) -> &ArcSwap<AuthorityAggregator<A>> {
        &self.validators
    }

    pub fn clone_committee(&self) -> Arc<Committee> {
        self.validators.load().committee.clone()
    }

    pub fn current_epoch(&self) -> EpochId {
        self.validators.load().committee.epoch
    }

    async fn enqueue_task(&self, task: QuorumDriverTask) -> SuiResult<()> {
        self.task_sender
            .send(task.clone())
            .await
            .tap_err(|e| debug!(?task, "Failed to enqueue task: {:?}", e))
            .tap_ok(|_| {
                debug!(?task, "Enqueued task.");
                self.metrics.current_requests_in_flight.inc();
                self.metrics.total_enqueued.inc();
                if task.retry_times > 0 {
                    if task.retry_times == 1 {
                        self.metrics.current_transactions_in_retry.inc();
                    }
                    self.metrics
                        .transaction_retry_count
                        .observe(task.retry_times as f64);
                }
            })
            .map_err(|e| SuiError::QuorumDriverCommunicationError {
                error: e.to_string(),
            })
    }

    /// Enqueue the task again if it hasn't maxed out the total retry attempts.
    /// If it has, notify failure.
    async fn enqueue_again_maybe(
        &self,
        request: ExecuteTransactionRequestV3,
        tx_cert: Option<CertifiedTransaction>,
        old_retry_times: u32,
        client_addr: Option<SocketAddr>,
    ) -> SuiResult<()> {
        if old_retry_times >= self.max_retry_times {
            // max out the retry times, notify failure
            info!(tx_digest=?request.transaction.digest(), "Failed to reach finality after attempting for {} times", old_retry_times+1);
            self.notify(
                &request.transaction,
                &Err(
                    QuorumDriverError::FailedWithTransientErrorAfterMaximumAttempts {
                        total_attempts: old_retry_times + 1,
                    },
                ),
                old_retry_times + 1,
            );
            return Ok(());
        }
        self.backoff_and_enqueue(request, tx_cert, old_retry_times, client_addr, None)
            .await
    }

    /// Performs exponential backoff and enqueue the `transaction` to the execution queue.
    /// When `min_backoff_duration` is provided, the backoff duration will be at least `min_backoff_duration`.
    async fn backoff_and_enqueue(
        &self,
        request: ExecuteTransactionRequestV3,
        tx_cert: Option<CertifiedTransaction>,
        old_retry_times: u32,
        client_addr: Option<SocketAddr>,
        min_backoff_duration: Option<Duration>,
    ) -> SuiResult<()> {
        let next_retry_after = Instant::now()
            + Duration::from_millis(200 * u64::pow(2, old_retry_times))
                .max(min_backoff_duration.unwrap_or(Duration::from_secs(0)));
        sleep_until(next_retry_after).await;

        fail_point!("count_retry_times");

        let tx_cert = match tx_cert {
            // TxCert is only valid when its epoch matches current epoch.
            // Note, it's impossible that TxCert's epoch is larger than current epoch
            // because the TxCert will be considered invalid and cannot reach here.
            Some(tx_cert) if tx_cert.epoch() == self.current_epoch() => Some(tx_cert),
            _other => None,
        };

        self.enqueue_task(QuorumDriverTask {
            request,
            tx_cert,
            retry_times: old_retry_times + 1,
            next_retry_after,
            client_addr,
            trace_span: Some(tracing::Span::current()),
        })
        .await
    }

    pub fn notify(
        &self,
        transaction: &Transaction,
        response: &QuorumDriverResult,
        total_attempts: u32,
    ) {
        let tx_digest = transaction.digest();
        let effects_queue_result = match &response {
            Ok(resp) => {
                self.metrics.total_ok_responses.inc();
                self.metrics
                    .attempt_times_ok_response
                    .observe(total_attempts as f64);
                Ok((transaction.clone(), resp.clone()))
            }
            Err(err) => {
                self.metrics
                    .total_err_responses
                    .with_label_values(&[err.as_ref()])
                    .inc();
                Err((*tx_digest, err.clone()))
            }
        };
        if total_attempts > 1 {
            self.metrics.current_transactions_in_retry.dec();
        }
        // On fullnode we expect the send to always succeed because TransactionOrchestrator should be subscribing
        // to this queue all the time. However the if QuorumDriver is used elsewhere log may be noisy.
        if let Err(err) = self.effects_subscribe_sender.send(effects_queue_result) {
            warn!(?tx_digest, "No subscriber found for effects: {}", err);
        }
        debug!(?tx_digest, "notify QuorumDriver task result");
        self.notifier.notify(tx_digest, response);
    }
}

impl<A> QuorumDriver<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    #[instrument(level = "trace", skip_all)]
    pub async fn submit_transaction(
        &self,
        request: ExecuteTransactionRequestV3,
    ) -> SuiResult<Registration<TransactionDigest, QuorumDriverResult>> {
        let tx_digest = request.transaction.digest();
        debug!(?tx_digest, "Received transaction execution request.");
        self.metrics.total_requests.inc();

        let ticket = self.notifier.register_one(tx_digest);
        self.enqueue_task(QuorumDriverTask {
            request,
            tx_cert: None,
            retry_times: 0,
            next_retry_after: Instant::now(),
            client_addr: None,
            trace_span: Some(tracing::Span::current()),
        })
        .await?;
        Ok(ticket)
    }

    // Used when the it is called in a component holding the notifier, and a ticket is
    // already obtained prior to calling this function, for instance, TransactionOrchestrator
    #[instrument(level = "trace", skip_all)]
    pub async fn submit_transaction_no_ticket(
        &self,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> SuiResult<()> {
        let tx_digest = request.transaction.digest();
        debug!(
            ?tx_digest,
            "Received transaction execution request, no ticket."
        );
        self.metrics.total_requests.inc();

        self.enqueue_task(QuorumDriverTask {
            request,
            tx_cert: None,
            retry_times: 0,
            next_retry_after: Instant::now(),
            client_addr,
            trace_span: Some(tracing::Span::current()),
        })
        .await
    }

    #[instrument(level = "trace", skip_all)]
    pub(crate) async fn process_transaction(
        &self,
        transaction: Transaction,
        client_addr: Option<SocketAddr>,
    ) -> Result<ProcessTransactionResult, Option<QuorumDriverError>> {
        let auth_agg = self.validators.load();
        let _tx_guard = GaugeGuard::acquire(&auth_agg.metrics.inflight_transactions);
        let tx_digest = *transaction.digest();
        let result = auth_agg.process_transaction(transaction, client_addr).await;

        self.process_transaction_result(result, tx_digest).await
    }

    #[instrument(level = "trace", skip_all)]
    async fn process_transaction_result(
        &self,
        result: Result<ProcessTransactionResult, AggregatorProcessTransactionError>,
        tx_digest: TransactionDigest,
    ) -> Result<ProcessTransactionResult, Option<QuorumDriverError>> {
        match result {
            Ok(resp) => Ok(resp),

            Err(AggregatorProcessTransactionError::FatalConflictingTransaction {
                errors,
                conflicting_tx_digests,
            }) => {
                debug!(
                    ?errors,
                    "Observed Tx {tx_digest:} double spend attempted. Conflicting Txes: {conflicting_tx_digests:?}",
                );
                Err(Some(QuorumDriverError::ObjectsDoubleUsed {
                    conflicting_txes: conflicting_tx_digests,
                }))
            }

            Err(AggregatorProcessTransactionError::FatalTransaction { errors }) => {
                debug!(?tx_digest, ?errors, "Nonretryable transaction error");
                Err(Some(QuorumDriverError::NonRecoverableTransactionError {
                    errors,
                }))
            }

            Err(AggregatorProcessTransactionError::SystemOverload {
                overloaded_stake,
                errors,
            }) => {
                debug!(?tx_digest, ?errors, "System overload");
                Err(Some(QuorumDriverError::SystemOverload {
                    overloaded_stake,
                    errors,
                }))
            }

            Err(AggregatorProcessTransactionError::SystemOverloadRetryAfter {
                overload_stake,
                errors,
                retry_after_secs,
            }) => {
                self.metrics.total_retryable_overload_errors.inc();
                debug!(
                    ?tx_digest,
                    ?errors,
                    "System overload and retry after secs {retry_after_secs}",
                );
                Err(Some(QuorumDriverError::SystemOverloadRetryAfter {
                    overload_stake,
                    errors,
                    retry_after_secs,
                }))
            }

            Err(AggregatorProcessTransactionError::RetryableTransaction { errors }) => {
                debug!(?tx_digest, ?errors, "Retryable transaction error");
                Err(None)
            }

            Err(
                AggregatorProcessTransactionError::TxAlreadyFinalizedWithDifferentUserSignatures,
            ) => {
                debug!(
                    ?tx_digest,
                    "Transaction is already finalized with different user signatures"
                );
                Err(Some(
                    QuorumDriverError::TxAlreadyFinalizedWithDifferentUserSignatures,
                ))
            }
        }
    }

    #[instrument(level = "trace", skip_all, fields(tx_digest = ?request.certificate.digest()))]
    pub(crate) async fn process_certificate(
        &self,
        request: HandleCertificateRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> Result<QuorumDriverResponse, Option<QuorumDriverError>> {
        let auth_agg = self.validators.load();
        let _cert_guard = GaugeGuard::acquire(&auth_agg.metrics.inflight_certificates);
        let tx_digest = *request.certificate.digest();
        let response = auth_agg
            .process_certificate(request.clone(), client_addr)
            .await
            .map_err(|agg_err| match agg_err {
                AggregatorProcessCertificateError::FatalExecuteCertificate {
                    non_retryable_errors,
                } => {
                    // Normally a certificate shouldn't have fatal errors.
                    error!(
                        ?tx_digest,
                        ?non_retryable_errors,
                        "[WATCHOUT] Unexpected Fatal error for certificate"
                    );
                    Some(QuorumDriverError::NonRecoverableTransactionError {
                        errors: non_retryable_errors,
                    })
                }
                AggregatorProcessCertificateError::RetryableExecuteCertificate {
                    retryable_errors,
                } => {
                    debug!(?retryable_errors, "Retryable certificate");
                    None
                }
            })?;

        Ok(response)
    }
}

impl<A> AuthorityAggregatorUpdatable<A> for QuorumDriver<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    fn epoch(&self) -> EpochId {
        self.validators.load().committee.epoch
    }

    fn authority_aggregator(&self) -> Arc<AuthorityAggregator<A>> {
        self.validators.load_full()
    }

    fn update_authority_aggregator(&self, new_authorities: Arc<AuthorityAggregator<A>>) {
        info!(
            "Quorum Driver updating AuthorityAggregator with committee {}",
            new_authorities.committee
        );
        self.validators.store(new_authorities);
    }
}

pub struct QuorumDriverHandler<A: Clone> {
    quorum_driver: Arc<QuorumDriver<A>>,
    effects_subscriber: tokio::sync::broadcast::Receiver<QuorumDriverEffectsQueueResult>,
    quorum_driver_metrics: Arc<QuorumDriverMetrics>,
    reconfig_observer: Arc<dyn ReconfigObserver<A> + Sync + Send>,
    _processor_handle: JoinHandle<()>,
}

impl<A> QuorumDriverHandler<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub(crate) fn new(
        validators: Arc<AuthorityAggregator<A>>,
        notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
        reconfig_observer: Arc<dyn ReconfigObserver<A> + Sync + Send>,
        metrics: Arc<QuorumDriverMetrics>,
        max_retry_times: u32,
    ) -> Self {
        let (task_tx, task_rx) = mpsc::channel::<QuorumDriverTask>(TASK_QUEUE_SIZE);
        let (subscriber_tx, subscriber_rx) =
            tokio::sync::broadcast::channel::<_>(EFFECTS_QUEUE_SIZE);
        let quorum_driver = Arc::new(QuorumDriver::new(
            ArcSwap::new(validators),
            task_tx,
            subscriber_tx,
            notifier,
            metrics.clone(),
            max_retry_times,
        ));
        let metrics_clone = metrics.clone();
        let processor_handle = {
            let quorum_driver_clone = quorum_driver.clone();
            spawn_monitored_task!(Self::task_queue_processor(
                quorum_driver_clone,
                task_rx,
                metrics_clone
            ))
        };
        let reconfig_observer_clone = reconfig_observer.clone();
        {
            let quorum_driver_clone = quorum_driver.clone();
            spawn_monitored_task!({
                async move {
                    let mut reconfig_observer_clone = reconfig_observer_clone.clone_boxed();
                    reconfig_observer_clone.run(quorum_driver_clone).await;
                }
            });
        };
        Self {
            quorum_driver,
            effects_subscriber: subscriber_rx,
            quorum_driver_metrics: metrics,
            reconfig_observer,
            _processor_handle: processor_handle,
        }
    }

    // Used when the it is called in a component holding the notifier, and a ticket is
    // already obtained prior to calling this function, for instance, TransactionOrchestrator
    pub async fn submit_transaction_no_ticket(
        &self,
        request: ExecuteTransactionRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> SuiResult<()> {
        self.quorum_driver
            .submit_transaction_no_ticket(request, client_addr)
            .await
    }

    pub async fn submit_transaction(
        &self,
        request: ExecuteTransactionRequestV3,
    ) -> SuiResult<Registration<TransactionDigest, QuorumDriverResult>> {
        self.quorum_driver.submit_transaction(request).await
    }

    /// Create a new `QuorumDriverHandler` based on the same AuthorityAggregator.
    /// Note: the new `QuorumDriverHandler` will have a new `ArcSwap<AuthorityAggregator>`
    /// that is NOT tied to the original one. So if there are multiple QuorumDriver(Handler)
    /// then all of them need to do reconfigs on their own.
    pub fn clone_new(&self) -> Self {
        let (task_sender, task_rx) = mpsc::channel::<QuorumDriverTask>(TASK_QUEUE_SIZE);
        let (effects_subscribe_sender, subscriber_rx) =
            tokio::sync::broadcast::channel::<_>(EFFECTS_QUEUE_SIZE);
        let validators = ArcSwap::new(self.quorum_driver.authority_aggregator().load_full());
        let quorum_driver = Arc::new(QuorumDriver {
            validators,
            task_sender,
            effects_subscribe_sender,
            notifier: Arc::new(NotifyRead::new()),
            metrics: self.quorum_driver_metrics.clone(),
            max_retry_times: self.quorum_driver.max_retry_times,
        });
        let metrics = self.quorum_driver_metrics.clone();
        let processor_handle = {
            let quorum_driver_copy = quorum_driver.clone();
            spawn_monitored_task!(Self::task_queue_processor(
                quorum_driver_copy,
                task_rx,
                metrics,
            ))
        };
        {
            let quorum_driver_copy = quorum_driver.clone();
            let reconfig_observer = self.reconfig_observer.clone();
            spawn_monitored_task!({
                async move {
                    let mut reconfig_observer_clone = reconfig_observer.clone_boxed();
                    reconfig_observer_clone.run(quorum_driver_copy).await;
                }
            })
        };

        Self {
            quorum_driver,
            effects_subscriber: subscriber_rx,
            quorum_driver_metrics: self.quorum_driver_metrics.clone(),
            reconfig_observer: self.reconfig_observer.clone(),
            _processor_handle: processor_handle,
        }
    }

    pub fn clone_quorum_driver(&self) -> Arc<QuorumDriver<A>> {
        self.quorum_driver.clone()
    }

    pub fn subscribe_to_effects(
        &self,
    ) -> tokio::sync::broadcast::Receiver<QuorumDriverEffectsQueueResult> {
        self.effects_subscriber.resubscribe()
    }

    pub fn authority_aggregator(&self) -> &ArcSwap<AuthorityAggregator<A>> {
        self.quorum_driver.authority_aggregator()
    }

    pub fn current_epoch(&self) -> EpochId {
        self.quorum_driver.current_epoch()
    }

    /// Process a QuorumDriverTask.
    /// The function has no return value - the corresponding actions of task result
    /// are performed in this call.
    #[instrument(level = "trace", parent = task.trace_span.as_ref().and_then(|s| s.id()), skip_all)]
    async fn process_task(quorum_driver: Arc<QuorumDriver<A>>, task: QuorumDriverTask) {
        debug!(?task, "Quorum Driver processing task");
        let QuorumDriverTask {
            request,
            tx_cert,
            retry_times: old_retry_times,
            client_addr,
            ..
        } = task;
        let transaction = &request.transaction;
        let tx_digest = *transaction.digest();
        let is_single_writer_tx = !transaction.contains_shared_object();

        let timer = Instant::now();
        let (tx_cert, newly_formed) = match tx_cert {
            None => match quorum_driver
                .process_transaction(transaction.clone(), client_addr)
                .await
            {
                Ok(ProcessTransactionResult::Certified {
                    certificate,
                    newly_formed,
                }) => {
                    debug!(?tx_digest, "Transaction processing succeeded");
                    (certificate, newly_formed)
                }
                Ok(ProcessTransactionResult::Executed(effects_cert, events)) => {
                    debug!(
                        ?tx_digest,
                        "Transaction processing succeeded with effects directly"
                    );
                    let response = QuorumDriverResponse {
                        effects_cert,
                        events: Some(events),
                        input_objects: None,
                        output_objects: None,
                        auxiliary_data: None,
                    };
                    quorum_driver.notify(transaction, &Ok(response), old_retry_times + 1);
                    return;
                }
                Err(err) => {
                    Self::handle_error(
                        quorum_driver,
                        request,
                        err,
                        None,
                        old_retry_times,
                        "get tx cert",
                        client_addr,
                    );
                    return;
                }
            },
            Some(tx_cert) => (tx_cert, false),
        };

        let response = match quorum_driver
            .process_certificate(
                HandleCertificateRequestV3 {
                    certificate: tx_cert.clone(),
                    include_events: request.include_events,
                    include_input_objects: request.include_input_objects,
                    include_output_objects: request.include_output_objects,
                    include_auxiliary_data: request.include_auxiliary_data,
                },
                client_addr,
            )
            .await
        {
            Ok(response) => {
                debug!(?tx_digest, "Certificate processing succeeded");
                response
            }
            // Note: non retryable failure when processing a cert
            // should be very rare.
            Err(err) => {
                Self::handle_error(
                    quorum_driver,
                    request,
                    err,
                    Some(tx_cert),
                    old_retry_times,
                    "get effects cert",
                    client_addr,
                );
                return;
            }
        };
        if newly_formed {
            let settlement_finality_latency = timer.elapsed().as_secs_f64();
            quorum_driver
                .metrics
                .settlement_finality_latency
                .with_label_values(&[if is_single_writer_tx {
                    TX_TYPE_SINGLE_WRITER_TX
                } else {
                    TX_TYPE_SHARED_OBJ_TX
                }])
                .observe(settlement_finality_latency);
            let is_out_of_expected_range =
                settlement_finality_latency >= 8.0 || settlement_finality_latency <= 0.1;
            debug!(
                ?tx_digest,
                ?is_single_writer_tx,
                ?is_out_of_expected_range,
                "QuorumDriver settlement finality latency: {:.3} seconds",
                settlement_finality_latency
            );
        }

        quorum_driver.notify(transaction, &Ok(response), old_retry_times + 1);
    }

    fn handle_error(
        quorum_driver: Arc<QuorumDriver<A>>,
        request: ExecuteTransactionRequestV3,
        err: Option<QuorumDriverError>,
        tx_cert: Option<CertifiedTransaction>,
        old_retry_times: u32,
        action: &'static str,
        client_addr: Option<SocketAddr>,
    ) {
        let tx_digest = *request.transaction.digest();
        match err {
            None => {
                debug!(?tx_digest, "Failed to {action} - Retrying");
                spawn_monitored_task!(quorum_driver.enqueue_again_maybe(
                    request.clone(),
                    tx_cert,
                    old_retry_times,
                    client_addr,
                ));
            }
            Some(QuorumDriverError::SystemOverloadRetryAfter {
                retry_after_secs, ..
            }) => {
                // Special case for SystemOverloadRetryAfter error. In this case, due to that objects are already
                // locked inside validators, we need to perform continuous retry and ignore `max_retry_times`.
                // TODO: the txn can potentially be retried unlimited times, therefore, we need to bound the number
                // of on going transactions in a quorum driver. When the limit is reached, the quorum driver should
                // reject any new transaction requests.
                debug!(?tx_digest, "Failed to {action} - Retrying");
                spawn_monitored_task!(quorum_driver.backoff_and_enqueue(
                    request.clone(),
                    tx_cert,
                    old_retry_times,
                    client_addr,
                    Some(Duration::from_secs(retry_after_secs)),
                ));
            }
            Some(qd_error) => {
                debug!(?tx_digest, "Failed to {action}: {}", qd_error);
                // non-retryable failure, this task reaches terminal state for now, notify waiter.
                quorum_driver.notify(&request.transaction, &Err(qd_error), old_retry_times + 1);
            }
        }
    }

    async fn task_queue_processor(
        quorum_driver: Arc<QuorumDriver<A>>,
        mut task_receiver: Receiver<QuorumDriverTask>,
        metrics: Arc<QuorumDriverMetrics>,
    ) {
        let limit = Arc::new(Semaphore::new(TASK_QUEUE_SIZE));
        while let Some(task) = task_receiver.recv().await {
            let task_queue_span =
                trace_span!(parent: task.trace_span.as_ref().and_then(|s| s.id()), "task_queue");
            let task_span_guard = task_queue_span.enter();

            // hold semaphore permit until task completes. unwrap ok because we never close
            // the semaphore in this context.
            let limit = limit.clone();
            let permit = limit.acquire_owned().await.unwrap();

            // TODO check reconfig process here

            debug!(?task, "Dequeued task");
            if Instant::now()
                .checked_duration_since(task.next_retry_after)
                .is_none()
            {
                // Not ready for next attempt yet, re-enqueue
                let _ = quorum_driver.enqueue_task(task).await;
                continue;
            }
            metrics.current_requests_in_flight.dec();
            let qd = quorum_driver.clone();
            drop(task_span_guard);
            spawn_monitored_task!(async move {
                let _guard = permit;
                QuorumDriverHandler::process_task(qd, task).await
            });
        }
    }
}

pub struct QuorumDriverHandlerBuilder<A: Clone> {
    validators: Arc<AuthorityAggregator<A>>,
    metrics: Arc<QuorumDriverMetrics>,
    notifier: Option<Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>>,
    reconfig_observer: Option<Arc<dyn ReconfigObserver<A> + Sync + Send>>,
    max_retry_times: u32,
}

impl<A> QuorumDriverHandlerBuilder<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(validators: Arc<AuthorityAggregator<A>>, metrics: Arc<QuorumDriverMetrics>) -> Self {
        Self {
            validators,
            metrics,
            notifier: None,
            reconfig_observer: None,
            max_retry_times: TX_MAX_RETRY_TIMES,
        }
    }

    pub(crate) fn with_notifier(
        mut self,
        notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
    ) -> Self {
        self.notifier = Some(notifier);
        self
    }

    pub fn with_reconfig_observer(
        mut self,
        reconfig_observer: Arc<dyn ReconfigObserver<A> + Sync + Send>,
    ) -> Self {
        self.reconfig_observer = Some(reconfig_observer);
        self
    }

    /// Used in tests when smaller number of retries is desired
    pub fn with_max_retry_times(mut self, max_retry_times: u32) -> Self {
        self.max_retry_times = max_retry_times;
        self
    }

    pub fn start(self) -> QuorumDriverHandler<A> {
        QuorumDriverHandler::new(
            self.validators,
            self.notifier.unwrap_or_else(|| {
                Arc::new(NotifyRead::<TransactionDigest, QuorumDriverResult>::new())
            }),
            self.reconfig_observer
                .expect("Reconfig observer is missing"),
            self.metrics,
            self.max_retry_times,
        )
    }
}
