// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod metrics;
pub use metrics::*;

pub mod reconfig_observer;

use arc_swap::ArcSwap;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use std::time::Duration;
use sui_types::base_types::{AuthorityName, ObjectRef, TransactionDigest};
use sui_types::committee::{Committee, EpochId, StakeUnit};
use sui_types::quorum_driver_types::{
    QuorumDriverEffectsQueueResult, QuorumDriverError, QuorumDriverResponse, QuorumDriverResult,
};
use tap::TapFallible;
use tokio::time::{sleep_until, Instant};

use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::Instrument;
use tracing::{debug, error, info, warn};

use crate::authority_aggregator::{
    AggregatorProcessCertificateError, AggregatorProcessTransactionError, AuthorityAggregator,
    ProcessTransactionResult,
};
use crate::authority_client::AuthorityAPI;
use mysten_common::sync::notify_read::{NotifyRead, Registration};
use mysten_metrics::{spawn_monitored_task, GaugeGuard};
use std::fmt::Write;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{PlainTransactionInfoResponse, VerifiedCertificate, VerifiedTransaction};

use self::reconfig_observer::ReconfigObserver;

#[cfg(test)]
mod tests;

const TASK_QUEUE_SIZE: usize = 10000;
const EFFECTS_QUEUE_SIZE: usize = 10000;
const TX_MAX_RETRY_TIMES: u8 = 10;

#[derive(Clone)]
pub struct QuorumDriverTask {
    pub transaction: VerifiedTransaction,
    pub tx_cert: Option<VerifiedCertificate>,
    pub retry_times: u8,
    pub next_retry_after: Instant,
}

impl Debug for QuorumDriverTask {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        write!(writer, "tx_digest={:?} ", self.transaction.digest())?;
        write!(writer, "has_tx_cert={} ", self.tx_cert.is_some())?;
        write!(writer, "retry_times={} ", self.retry_times)?;
        write!(writer, "next_retry_after={:?} ", self.next_retry_after)?;
        write!(f, "{}", writer)
    }
}

pub struct QuorumDriver<A> {
    validators: ArcSwap<AuthorityAggregator<A>>,
    task_sender: Sender<QuorumDriverTask>,
    effects_subscribe_sender: tokio::sync::broadcast::Sender<QuorumDriverEffectsQueueResult>,
    notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
    metrics: Arc<QuorumDriverMetrics>,
    max_retry_times: u8,
}

impl<A> QuorumDriver<A> {
    pub(crate) fn new(
        validators: ArcSwap<AuthorityAggregator<A>>,
        task_sender: Sender<QuorumDriverTask>,
        effects_subscribe_sender: tokio::sync::broadcast::Sender<QuorumDriverEffectsQueueResult>,
        notifier: Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>,
        metrics: Arc<QuorumDriverMetrics>,
        max_retry_times: u8,
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

    pub fn clone_committee(&self) -> Committee {
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
            })
            .map_err(|e| SuiError::QuorumDriverCommunicationError {
                error: e.to_string(),
            })
    }

    /// Enqueue the task again if it hasn't maxed out the total retry attempts.
    /// If it has, notify failure.
    /// Enqueuing happens only after the `next_retry_after`, if not, wait until that instant
    async fn enqueue_again_maybe(
        &self,
        transaction: VerifiedTransaction,
        tx_cert: Option<VerifiedCertificate>,
        old_retry_times: u8,
    ) -> SuiResult<()> {
        if old_retry_times >= self.max_retry_times {
            // max out the retry times, notify failure
            info!(tx_digest=?transaction.digest(), "Failed to reach finality after attempting for {} times", old_retry_times+1);
            self.notify(
                &transaction,
                &Err(
                    QuorumDriverError::FailedWithTransientErrorAfterMaximumAttempts {
                        total_attempts: old_retry_times + 1,
                    },
                ),
                old_retry_times + 1,
            );
            return Ok(());
        }
        let next_retry_after =
            Instant::now() + Duration::from_millis(200 * u64::pow(2, old_retry_times.into()));
        sleep_until(next_retry_after).await;

        let tx_cert = match tx_cert {
            // TxCert is only valid when its epoch matches current epoch.
            // Note, it's impossible that TxCert's epoch is larger than current epoch
            // because the TxCert will be considered invalid and cannot reach here.
            Some(tx_cert) if tx_cert.epoch() == self.current_epoch() => Some(tx_cert),
            _other => None,
        };

        self.enqueue_task(QuorumDriverTask {
            transaction,
            tx_cert,
            retry_times: old_retry_times + 1,
            next_retry_after,
        })
        .await
    }

    pub fn notify(
        &self,
        transaction: &VerifiedTransaction,
        response: &QuorumDriverResult,
        total_attempts: u8,
    ) {
        let tx_digest = transaction.digest();
        let effects_queue_result = match &response {
            Ok(resp) => {
                self.metrics.total_ok_responses.inc();
                self.metrics
                    .attempt_times_ok_response
                    .report(total_attempts as u64);
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
    pub async fn submit_transaction(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiResult<Registration<TransactionDigest, QuorumDriverResult>> {
        let tx_digest = transaction.digest();
        debug!(?tx_digest, "Received transaction execution request.");
        self.metrics.total_requests.inc();

        let ticket = self.notifier.register_one(tx_digest);
        self.enqueue_task(QuorumDriverTask {
            transaction,
            tx_cert: None,
            retry_times: 0,
            next_retry_after: Instant::now(),
        })
        .await?;
        Ok(ticket)
    }

    // Used when the it is called in a component holding the notifier, and a ticket is
    // already obtained prior to calling this function, for instance, TransactionOrchestrator
    pub async fn submit_transaction_no_ticket(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiResult<()> {
        let tx_digest = transaction.digest();
        debug!(
            ?tx_digest,
            "Received transaction execution request, no ticket."
        );
        self.metrics.total_requests.inc();

        self.enqueue_task(QuorumDriverTask {
            transaction,
            tx_cert: None,
            retry_times: 0,
            next_retry_after: Instant::now(),
        })
        .await
    }

    pub(crate) async fn process_transaction(
        &self,
        transaction: VerifiedTransaction,
    ) -> Result<ProcessTransactionResult, Option<QuorumDriverError>> {
        let auth_agg = self.validators.load();
        let _tx_guard = GaugeGuard::acquire(&auth_agg.metrics.inflight_transactions);
        let tx_digest = *transaction.digest();
        let result = auth_agg
            .process_transaction(transaction)
            .instrument(tracing::debug_span!("aggregator_process_tx", ?tx_digest))
            .await;

        self.process_transaction_result(result, tx_digest).await
    }

    async fn process_transaction_result(
        &self,
        result: Result<ProcessTransactionResult, AggregatorProcessTransactionError>,
        tx_digest: TransactionDigest,
    ) -> Result<ProcessTransactionResult, Option<QuorumDriverError>> {
        match result {
            Ok(resp) => Ok(resp),
            Err(AggregatorProcessTransactionError::RetryableConflictingTransaction {
                conflicting_tx_digest_to_retry,
                errors,
                conflicting_tx_digests,
            }) => {
                self.metrics
                    .total_err_process_tx_responses_with_nonzero_conflicting_transactions
                    .inc();
                debug!(
                    ?tx_digest,
                    "Observed {} conflicting transactions: {:?}",
                    conflicting_tx_digests.len(),
                    conflicting_tx_digests
                );

                if let Some(conflicting_tx_digest) = conflicting_tx_digest_to_retry {
                    self.process_conflicting_tx(
                        tx_digest,
                        conflicting_tx_digest,
                        conflicting_tx_digests,
                    )
                    .await
                } else {
                    // If no retryable conflicting transaction was returned that means we have >= 2f+1 good stake for
                    // the original transaction + retryable stake. Will continue to retry the original transaction.
                    debug!(
                        ?errors,
                        "Observed Tx {tx_digest:} is still in retryable state. Conflicting Txes: {conflicting_tx_digests:?}", 
                    );
                    Err(None)
                }
            }

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
                    retried_tx: None,
                    retried_tx_success: None,
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

            Err(AggregatorProcessTransactionError::RetryableTransaction { errors }) => {
                debug!(?tx_digest, ?errors, "Retryable transaction error");
                Err(None)
            }
        }
    }

    async fn process_conflicting_tx(
        &self,
        tx_digest: TransactionDigest,
        conflicting_tx_digest: TransactionDigest,
        conflicting_tx_digests: BTreeMap<
            TransactionDigest,
            (Vec<(AuthorityName, ObjectRef)>, StakeUnit),
        >,
    ) -> Result<ProcessTransactionResult, Option<QuorumDriverError>> {
        // Safe to unwrap because tx_digest_to_retry is generated from conflicting_tx_digests
        // in ProcessTransactionState::conflicting_tx_digest_with_most_stake()
        let (validators, _) = conflicting_tx_digests.get(&conflicting_tx_digest).unwrap();
        let attempt_result = self
            .attempt_conflicting_transaction(
                &conflicting_tx_digest,
                &tx_digest,
                validators.iter().map(|(pub_key, _)| *pub_key).collect(),
            )
            .await;
        self.metrics
            .total_attempts_retrying_conflicting_transaction
            .inc();

        match attempt_result {
            Err(err) => {
                debug!(
                    ?tx_digest,
                    "Encountered error while attemptting conflicting transaction: {:?}", err
                );
                let err = Err(Some(QuorumDriverError::ObjectsDoubleUsed {
                    conflicting_txes: conflicting_tx_digests,
                    retried_tx: None,
                    retried_tx_success: None,
                }));
                debug!(
                    ?tx_digest,
                    "Non retryable error when getting original tx cert: {err:?}"
                );
                err
            }
            Ok(success) => {
                debug!(
                    ?tx_digest,
                    ?conflicting_tx_digest,
                    "Retried conflicting transaction success: {}",
                    success
                );
                if success {
                    self.metrics
                        .total_successful_attempts_retrying_conflicting_transaction
                        .inc();
                }
                Err(Some(QuorumDriverError::ObjectsDoubleUsed {
                    conflicting_txes: conflicting_tx_digests,
                    retried_tx: Some(conflicting_tx_digest),
                    retried_tx_success: Some(success),
                }))
            }
        }
    }

    pub(crate) async fn process_certificate(
        &self,
        certificate: VerifiedCertificate,
    ) -> Result<QuorumDriverResponse, Option<QuorumDriverError>> {
        let auth_agg = self.validators.load();
        let _cert_guard = GaugeGuard::acquire(&auth_agg.metrics.inflight_certificates);
        let tx_digest = *certificate.digest();
        let (effects, events, objects) = auth_agg
            .process_certificate(certificate.clone().into_inner())
            .instrument(tracing::debug_span!("aggregator_process_cert", ?tx_digest))
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
        let response = QuorumDriverResponse {
            effects_cert: effects,
            events,
            objects,
        };

        Ok(response)
    }

    pub async fn update_validators(&self, new_validators: Arc<AuthorityAggregator<A>>) {
        info!(
            "Quorum Driver updating AuthorityAggregator with committee {}",
            new_validators.committee
        );
        self.validators.store(new_validators);
    }

    /// Returns Some(true) if the conflicting transaction is executed successfully
    /// (or already executed), or Some(false) if it did not.
    async fn attempt_conflicting_transaction(
        &self,
        tx_digest: &TransactionDigest,
        original_tx_digest: &TransactionDigest,
        validators: BTreeSet<AuthorityName>,
    ) -> SuiResult<bool> {
        let response = self
            .validators
            .load()
            .handle_transaction_info_request_from_some_validators(
                tx_digest,
                &validators,
                Some(Duration::from_secs(10)),
            )
            .await?;

        // If we are able to get a certificate right away, we use it and execute the cert;
        // otherwise, we have to re-form a cert and execute it.
        let verified_transaction = match response {
            PlainTransactionInfoResponse::ExecutedWithCert(cert, _, _) => {
                self.metrics
                    .total_times_conflicting_transaction_already_finalized_when_retrying
                    .inc();
                // We still want to ask validators to execute this certificate in case this certificate is not
                // known to the rest of them (e.g. when *this* validator is bad).
                let result = self
                    .validators
                    .load()
                    .process_certificate(cert.into_inner())
                    .await
                    .tap_ok(|_resp| {
                        debug!(
                            ?tx_digest,
                            ?original_tx_digest,
                            "Retry conflicting transaction certificate succeeded."
                        );
                    })
                    .tap_err(|err| {
                        debug!(
                            ?tx_digest,
                            ?original_tx_digest,
                            "Retry conflicting transaction certificate got an error: {:?}",
                            err
                        );
                    });
                // We only try it once.
                return Ok(result.is_ok());
            }
            PlainTransactionInfoResponse::Signed(signed) => {
                signed.verify(&self.clone_committee())?.into_unsigned()
            }
            PlainTransactionInfoResponse::ExecutedWithoutCert(transaction, _, _) => transaction,
        };
        // Now ask validators to execute this transaction.
        let result = self
            .validators
            .load()
            .execute_transaction_block(&verified_transaction)
            .await
            .tap_ok(|_resp| {
                debug!(
                    ?tx_digest,
                    ?original_tx_digest,
                    "Retry conflicting transaction succeeded."
                );
            })
            .tap_err(|err| {
                debug!(
                    ?tx_digest,
                    ?original_tx_digest,
                    "Retry conflicting transaction got an error: {:?}",
                    err
                );
            });
        // We only try it once
        Ok(result.is_ok())
    }
}

pub struct QuorumDriverHandler<A> {
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
        max_retry_times: u8,
    ) -> Self {
        let (task_tx, task_rx) = mpsc::channel::<QuorumDriverTask>(TASK_QUEUE_SIZE);
        let (subscriber_tx, subscriber_rx) =
            tokio::sync::broadcast::channel::<_>(EFFECTS_QUEUE_SIZE);
        let quorum_driver = Arc::new(QuorumDriver::new(
            ArcSwap::from(validators),
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
        transaction: VerifiedTransaction,
    ) -> SuiResult<()> {
        self.quorum_driver
            .submit_transaction_no_ticket(transaction)
            .await
    }

    pub async fn submit_transaction(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiResult<Registration<TransactionDigest, QuorumDriverResult>> {
        self.quorum_driver.submit_transaction(transaction).await
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
    async fn process_task(quorum_driver: Arc<QuorumDriver<A>>, task: QuorumDriverTask) {
        debug!(?task, "Quorum Driver processing task");
        let QuorumDriverTask {
            transaction,
            tx_cert,
            retry_times: old_retry_times,
            ..
        } = task;
        let tx_digest = *transaction.digest();

        let tx_cert = match tx_cert {
            None => match quorum_driver.process_transaction(transaction.clone()).await {
                Ok(ProcessTransactionResult::Certified(tx_cert)) => {
                    debug!(?tx_digest, "Transaction processing succeeded");
                    tx_cert
                }
                Ok(ProcessTransactionResult::Executed(effects_cert, events)) => {
                    debug!(
                        ?tx_digest,
                        "Transaction processing succeeded with effects directly"
                    );
                    let response = QuorumDriverResponse {
                        effects_cert,
                        events,
                        objects: vec![],
                    };
                    quorum_driver.notify(&transaction, &Ok(response), old_retry_times + 1);
                    return;
                }
                Err(err) => {
                    Self::handle_error(
                        quorum_driver,
                        transaction,
                        err,
                        None,
                        old_retry_times,
                        "get tx cert",
                    );
                    return;
                }
            },
            Some(tx_cert) => tx_cert,
        };

        let response = match quorum_driver.process_certificate(tx_cert.clone()).await {
            Ok(response) => {
                debug!(?tx_digest, "Certificate processing succeeded");
                response
            }
            // Note: non retryable failure when processing a cert
            // should be very rare.
            Err(err) => {
                Self::handle_error(
                    quorum_driver,
                    transaction,
                    err,
                    Some(tx_cert),
                    old_retry_times,
                    "get effects cert",
                );
                return;
            }
        };

        quorum_driver.notify(&transaction, &Ok(response), old_retry_times + 1);
    }

    fn handle_error(
        quorum_driver: Arc<QuorumDriver<A>>,
        transaction: VerifiedTransaction,
        err: Option<QuorumDriverError>,
        tx_cert: Option<VerifiedCertificate>,
        old_retry_times: u8,
        action: &'static str,
    ) {
        let tx_digest = *transaction.digest();
        if let Some(qd_error) = err {
            debug!(?tx_digest, "Failed to {action}: {}", qd_error);
            // non-retryable failure, this task reaches terminal state for now, notify waiter.
            quorum_driver.notify(&transaction, &Err(qd_error), old_retry_times + 1);
        } else {
            debug!(?tx_digest, "Failed to {action} - Retrying");
            spawn_monitored_task!(quorum_driver.enqueue_again_maybe(
                transaction.clone(),
                tx_cert,
                old_retry_times
            ));
        }
    }

    async fn task_queue_processor(
        quorum_driver: Arc<QuorumDriver<A>>,
        mut task_receiver: Receiver<QuorumDriverTask>,
        metrics: Arc<QuorumDriverMetrics>,
    ) {
        while let Some(task) = task_receiver.recv().await {
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
            spawn_monitored_task!(QuorumDriverHandler::process_task(qd, task));
        }
    }
}

pub struct QuorumDriverHandlerBuilder<A> {
    validators: Arc<AuthorityAggregator<A>>,
    metrics: Arc<QuorumDriverMetrics>,
    notifier: Option<Arc<NotifyRead<TransactionDigest, QuorumDriverResult>>>,
    reconfig_observer: Option<Arc<dyn ReconfigObserver<A> + Sync + Send>>,
    max_retry_times: u8,
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
    pub fn with_max_retry_times(mut self, max_retry_times: u8) -> Self {
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
