// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod metrics;
pub use metrics::*;

use arc_swap::ArcSwap;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;
use sui_types::base_types::{AuthorityName, ObjectRef, TransactionDigest};
use sui_types::committee::{Committee, EpochId, StakeUnit};
use tap::TapFallible;

use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::Instrument;
use tracing::{debug, error, info, warn};

use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::AuthorityAPI;
use sui_metrics::spawn_monitored_task;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{
    CertifiedTransaction, CertifiedTransactionEffects, QuorumDriverRequest,
    QuorumDriverRequestType, QuorumDriverResponse, VerifiedTransaction,
};

const TASK_QUEUE_SIZE: usize = 5000;

pub enum QuorumTask {
    ProcessTransaction(VerifiedTransaction),
    ProcessCertificate(CertifiedTransaction),
}

/// A handler to wrap around QuorumDriver. This handler should be owned by the node with exclusive
/// mutability.
pub struct QuorumDriverHandler<A> {
    quorum_driver: Arc<QuorumDriver<A>>,
    _processor_handle: JoinHandle<()>,
    // TODO: Change to CertifiedTransactionEffects eventually.
    effects_subscriber:
        tokio::sync::broadcast::Receiver<(CertifiedTransaction, CertifiedTransactionEffects)>,
    quorum_driver_metrics: Arc<QuorumDriverMetrics>,
}

/// The core data structure of the QuorumDriver.
/// It's expected that the QuorumDriver will be wrapped in an `Arc` and shared around.
/// One copy will be used in a json-RPC server to serve transaction execution requests;
/// Another copy will be held by a QuorumDriverHandler to either send signal to update the
/// committee, or to subscribe effects generated from the QuorumDriver.
pub struct QuorumDriver<A> {
    validators: ArcSwap<AuthorityAggregator<A>>,
    task_sender: Sender<QuorumTask>,
    effects_subscribe_sender:
        tokio::sync::broadcast::Sender<(CertifiedTransaction, CertifiedTransactionEffects)>,
    metrics: Arc<QuorumDriverMetrics>,
}

impl<A> QuorumDriver<A> {
    pub fn new(
        validators: Arc<AuthorityAggregator<A>>,
        task_sender: Sender<QuorumTask>,
        effects_subscribe_sender: tokio::sync::broadcast::Sender<(
            CertifiedTransaction,
            CertifiedTransactionEffects,
        )>,
        metrics: Arc<QuorumDriverMetrics>,
    ) -> Self {
        Self {
            validators: ArcSwap::from(validators),
            task_sender,
            effects_subscribe_sender,
            metrics,
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
}

impl<A> QuorumDriver<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub async fn execute_transaction(
        &self,
        request: QuorumDriverRequest,
    ) -> SuiResult<QuorumDriverResponse> {
        let tx_digest = request.transaction.digest();
        debug!(?tx_digest, "Received transaction execution request");
        self.metrics.current_requests_in_flight.inc();
        let _metrics_guard = scopeguard::guard(self.metrics.clone(), |metrics| {
            metrics.current_requests_in_flight.dec();
        });

        let QuorumDriverRequest {
            transaction,
            request_type,
        } = request;
        let (ok_metric, result) = match request_type {
            QuorumDriverRequestType::ImmediateReturn => {
                self.metrics.total_requests_immediate_return.inc();
                let _timer = self.metrics.latency_sec_immediate_return.start_timer();

                let res = self.execute_transaction_immediate_return(transaction).await;

                (&self.metrics.total_ok_responses_immediate_return, res)
            }
            QuorumDriverRequestType::WaitForTxCert => {
                self.metrics.total_requests_wait_for_tx_cert.inc();
                let _timer = self.metrics.latency_sec_wait_for_tx_cert.start_timer();

                let res = self.execute_transaction_wait_for_tx_cert(transaction).await;

                (&self.metrics.total_ok_responses_wait_for_tx_cert, res)
            }
            QuorumDriverRequestType::WaitForEffectsCert => {
                self.metrics.total_requests_wait_for_effects_cert.inc();
                let _timer = self.metrics.latency_sec_wait_for_effects_cert.start_timer();

                let res = self
                    .execute_transaction_wait_for_effects_cert(transaction)
                    .await;

                (&self.metrics.total_ok_responses_wait_for_effects_cert, res)
            }
        };
        if result.is_ok() {
            ok_metric.inc()
        }
        result
    }

    async fn execute_transaction_immediate_return(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiResult<QuorumDriverResponse> {
        self.task_sender
            .send(QuorumTask::ProcessTransaction(transaction))
            .await
            .map_err(|err| SuiError::QuorumDriverCommunicationError {
                error: err.to_string(),
            })?;
        Ok(QuorumDriverResponse::ImmediateReturn)
    }

    async fn execute_transaction_wait_for_tx_cert(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiResult<QuorumDriverResponse> {
        let certificate = self.process_transaction(transaction).await?;
        self.task_sender
            .send(QuorumTask::ProcessCertificate(certificate.clone()))
            .await
            .map_err(|err| SuiError::QuorumDriverCommunicationError {
                error: err.to_string(),
            })?;
        Ok(QuorumDriverResponse::TxCert(Box::new(certificate)))
    }

    async fn execute_transaction_wait_for_effects_cert(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiResult<QuorumDriverResponse> {
        let certificate = self.process_transaction(transaction).await?;
        let response = self.process_certificate(certificate).await?;
        Ok(QuorumDriverResponse::EffectsCert(Box::new(response)))
    }

    pub async fn process_transaction(
        &self,
        transaction: VerifiedTransaction,
    ) -> SuiResult<CertifiedTransaction> {
        let tx_digest = *transaction.digest();
        let result = self
            .validators
            .load()
            .process_transaction(transaction)
            .instrument(tracing::debug_span!("process_tx", ?tx_digest))
            .await
            .map(|v| v.into());

        match &result {
            Err(SuiError::QuorumFailedToProcessTransaction {
                good_stake,
                errors: _errors,
                conflicting_tx_digests,
            }) if !conflicting_tx_digests.is_empty() => {
                self.metrics
                    .total_err_process_tx_responses_with_nonzero_conflicting_transactions
                    .inc();
                debug!(
                    ?tx_digest,
                    ?good_stake,
                    "Observed {} conflicting transactions: {:?}",
                    conflicting_tx_digests.len(),
                    conflicting_tx_digests
                );
                let attempt_result = self
                    .attempt_conflicting_transactions_maybe(
                        *good_stake,
                        conflicting_tx_digests,
                        &tx_digest,
                    )
                    .await;
                match attempt_result {
                    Err(err) => {
                        debug!(
                            ?tx_digest,
                            "Encountered error in attempt_conflicting_transactions_maybe: {:?}",
                            err
                        );
                    }
                    Ok(None) => {
                        debug!(?tx_digest, "Did not retry any conflicting transactions");
                    }
                    Ok(Some((retried_tx_digest, success))) => {
                        self.metrics
                            .total_attempts_retrying_conflicting_transaction
                            .inc();
                        debug!(
                            ?tx_digest,
                            ?retried_tx_digest,
                            "Retried conflicting transaction success: {}",
                            success
                        );
                        if success {
                            self.metrics
                                .total_successful_attempts_retrying_conflicting_transaction
                                .inc();
                        }
                        return Err(SuiError::QuorumFailedToProcessTransactionWithConflictingTransactionRetried { conflicting_tx_digest: retried_tx_digest, conflicting_tx_retry_success: success });
                    }
                }
            }
            _ => (),
        }
        result
    }

    pub async fn process_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> SuiResult<(CertifiedTransaction, CertifiedTransactionEffects)> {
        let effects = self
            .validators
            .load()
            .process_certificate(certificate.clone())
            .instrument(tracing::debug_span!("process_cert", tx_digest = ?certificate.digest()))
            .await?;
        let response = (certificate, effects);
        // An error to send the result to subscribers should not block returning the result.
        if let Err(err) = self.effects_subscribe_sender.send(response.clone()) {
            // TODO: We could potentially retry sending if we want.
            debug!("No subscriber found for effects: {}", err);
        }
        Ok(response)
    }

    pub async fn update_validators(
        &self,
        new_validators: Arc<AuthorityAggregator<A>>,
    ) -> SuiResult {
        self.validators.store(new_validators);
        Ok(())
    }

    // TODO currently this function is not epoch-boundary-safe. We need to make it so.
    /// Returns Ok(None) if the no conflicting transaction was retried.
    /// Returns Ok(Some((tx_digest, true))) if one conflicting transaction was retried and succeeded,
    /// Some((tx_digest, false)) otherwise.
    /// Returns Error on unexpected errors.
    #[allow(clippy::type_complexity)]
    async fn attempt_conflicting_transactions_maybe(
        &self,
        good_stake: StakeUnit,
        conflicting_tx_digests: &BTreeMap<
            TransactionDigest,
            (Vec<(AuthorityName, ObjectRef)>, StakeUnit),
        >,
        original_tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<(TransactionDigest, bool)>> {
        let validity = self.validators.load().committee.validity_threshold();

        let mut conflicting_tx_digests = Vec::from_iter(conflicting_tx_digests.iter());
        conflicting_tx_digests.sort_by(|lhs, rhs| rhs.1 .1.cmp(&lhs.1 .1));
        if conflicting_tx_digests.is_empty() {
            error!("This path in unreachable with an empty conflicting_tx_digests.");
            return Ok(None);
        }

        // we checked emptiness above, safe to unwrap.
        let (tx_digest, (validators, total_stake)) = conflicting_tx_digests.get(0).unwrap();

        if good_stake >= validity && *total_stake >= validity {
            warn!(
                ?tx_digest,
                ?original_tx_digest,
                original_tx_stake = good_stake,
                tx_stake = *total_stake,
                "Equivocation detected: {:?}",
                validators
            );
            self.metrics.total_equivocation_detected.inc();
            return Ok(None);
        }

        // if we have >= f+1 good stake on the current transaction, no point in retrying conflicting ones
        if good_stake >= validity {
            return Ok(None);
        }

        // To be more conservative and try not to actually cause full equivocation,
        // we only retry a transaction when at least f+1 validators claims this tx locks objects
        if *total_stake < validity {
            return Ok(None);
        }

        info!(
            ?tx_digest,
            ?total_stake,
            ?original_tx_digest,
            "retrying conflicting tx."
        );
        let is_tx_executed = self
            .attempt_one_conflicting_transaction(
                tx_digest,
                original_tx_digest,
                validators
                    .iter()
                    .map(|(name, _obj_ref)| *name)
                    .collect::<BTreeSet<_>>(),
            )
            .await?;

        Ok(Some((**tx_digest, is_tx_executed)))
    }

    /// Returns Some(true) if the conflicting transaction is executed successfully
    /// (or already executed), or Some(false) if it did not.
    async fn attempt_one_conflicting_transaction(
        &self,
        tx_digest: &&TransactionDigest,
        original_tx_digest: &TransactionDigest,
        validators: BTreeSet<AuthorityName>,
    ) -> SuiResult<bool> {
        let (signed_transaction, certified_transaction) = self
            .validators
            .load()
            .handle_transaction_info_request_from_some_validators(
                tx_digest,
                &validators,
                Some(Duration::from_secs(10)),
            )
            .await?;

        // If we happen to find that a validator returns TransactionCertificate:
        if let Some(certified_transaction) = certified_transaction {
            self.metrics
                .total_times_conflicting_transaction_already_finalized_when_retrying
                .inc();
            // We still want to ask validators to execute this certificate in case this certificate is not
            // known to the rest of them (e.g. when *this* validator is bad).
            let result = self
                .validators
                .load()
                .process_certificate(certified_transaction.into_inner())
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

        if let Some(signed_transaction) = signed_transaction {
            let verified_transaction = signed_transaction.into_unsigned();
            // Now ask validators to execute this transaction.
            let result = self
                .validators
                .load()
                .execute_transaction(&verified_transaction)
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
            return Ok(result.is_ok());
        }

        // This is unreachable.
        let err_str = "handle_transaction_info_request_from_some_validators shouldn't return empty SignedTransaction and empty CertifiedTransaction";
        error!(err_str);
        Err(SuiError::from(err_str))
    }
}

impl<A> QuorumDriverHandler<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(validators: Arc<AuthorityAggregator<A>>, metrics: QuorumDriverMetrics) -> Self {
        let (task_tx, task_rx) = mpsc::channel::<QuorumTask>(TASK_QUEUE_SIZE);
        let (subscriber_tx, subscriber_rx) = tokio::sync::broadcast::channel::<_>(100);
        let metrics = Arc::new(metrics);
        let quorum_driver = Arc::new(QuorumDriver::new(
            validators,
            task_tx,
            subscriber_tx,
            metrics.clone(),
        ));
        let handle = {
            let quorum_driver_copy = quorum_driver.clone();
            spawn_monitored_task!(Self::task_queue_processor(quorum_driver_copy, task_rx))
        };
        Self {
            quorum_driver,
            _processor_handle: handle,
            effects_subscriber: subscriber_rx,
            quorum_driver_metrics: metrics,
        }
    }

    /// Create a new QuorumDriverHandler based on the same AuthorityAggregator.
    /// Note: the new QuorumDriverHandler will have a new ArcSwap<AuthorityAggregator>
    /// that is NOT tied to the original one. So if there are multiple QuorumDriver(Handler)
    /// then all of them need to do reconfigs on their own.
    pub fn clone_new(&self) -> Self {
        let (task_sender, task_rx) = mpsc::channel::<QuorumTask>(TASK_QUEUE_SIZE);
        let (effects_subscribe_sender, subscriber_rx) = tokio::sync::broadcast::channel::<_>(100);
        let validators = ArcSwap::new(self.quorum_driver.authority_aggregator().load_full());
        let quorum_driver = Arc::new(QuorumDriver {
            validators,
            task_sender,
            effects_subscribe_sender,
            metrics: self.quorum_driver_metrics.clone(),
        });
        let handle = {
            let quorum_driver_copy = quorum_driver.clone();
            spawn_monitored_task!(Self::task_queue_processor(quorum_driver_copy, task_rx))
        };
        Self {
            quorum_driver,
            _processor_handle: handle,
            effects_subscriber: subscriber_rx,
            quorum_driver_metrics: self.quorum_driver_metrics.clone(),
        }
    }

    pub fn clone_quorum_driver(&self) -> Arc<QuorumDriver<A>> {
        self.quorum_driver.clone()
    }

    pub fn subscribe(
        &self,
    ) -> tokio::sync::broadcast::Receiver<(CertifiedTransaction, CertifiedTransactionEffects)> {
        self.effects_subscriber.resubscribe()
    }

    async fn task_queue_processor(
        quorum_driver: Arc<QuorumDriver<A>>,
        mut task_receiver: Receiver<QuorumTask>,
    ) {
        // TODO https://github.com/MystenLabs/sui/issues/4565
        // spawn a tokio task for each job for higher concurrency
        loop {
            if let Some(task) = task_receiver.recv().await {
                match task {
                    QuorumTask::ProcessTransaction(transaction) => {
                        let tx_digest = *transaction.digest();
                        // TODO: We entered here because callers do not want to wait for a
                        // transaction to finish execution. When this failed, we do not have a
                        // way to notify the caller. In the future, we may want to maintain
                        // some data structure for callers to come back and query the status
                        // of a transaction later.
                        match quorum_driver.process_transaction(transaction).await {
                            Ok(cert) => {
                                debug!(?tx_digest, "Transaction processing succeeded");
                                if let Err(err) = quorum_driver.process_certificate(cert).await {
                                    warn!(?tx_digest, "Certificate processing failed: {:?}", err);
                                }
                                debug!(?tx_digest, "Certificate processing succeeded");
                            }
                            Err(err) => {
                                warn!(?tx_digest, "Transaction processing failed: {:?}", err);
                            }
                        }
                    }
                    QuorumTask::ProcessCertificate(certificate) => {
                        let tx_digest = *certificate.digest();
                        // TODO: Similar to ProcessTransaction, we may want to allow callers to
                        // query the status.
                        match quorum_driver.process_certificate(certificate).await {
                            Err(err) => {
                                warn!(?tx_digest, "Certificate processing failed: {:?}", err);
                            }
                            Ok(_) => {
                                debug!(?tx_digest, "Certificate processing succeeded");
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority_aggregator::authority_aggregator_tests::init_local_authorities;

    #[tokio::test]
    async fn test_not_retry_on_object_locked() -> Result<(), anyhow::Error> {
        let (auth_agg, _, _) = init_local_authorities(4, vec![]).await;

        let quorum_driver_handler = QuorumDriverHandler::new(
            Arc::new(auth_agg.clone()),
            QuorumDriverMetrics::new_for_tests(),
        );
        let quorum_driver = quorum_driver_handler.clone_quorum_driver();
        let validity = quorum_driver
            .authority_aggregator()
            .load()
            .committee
            .validity_threshold();

        assert_eq!(auth_agg.clone_inner_clients().keys().cloned().count(), 4);

        // good stake >= validity, no transaction will be retried, expect Ok(None)
        assert_eq!(
            quorum_driver
                .attempt_conflicting_transactions_maybe(
                    validity,
                    &BTreeMap::new(),
                    &TransactionDigest::random()
                )
                .await,
            Ok(None)
        );
        assert_eq!(
            quorum_driver
                .attempt_conflicting_transactions_maybe(
                    validity + 1,
                    &BTreeMap::new(),
                    &TransactionDigest::random()
                )
                .await,
            Ok(None)
        );

        // good stake < validity, but the top transaction total stake < validaty too, no transaction will be retried, expect Ok(None)
        let conflicting_tx_digests = BTreeMap::from([
            (TransactionDigest::random(), (vec![], validity - 1)),
            (TransactionDigest::random(), (vec![], 1)),
        ]);
        assert_eq!(
            quorum_driver
                .attempt_conflicting_transactions_maybe(
                    validity - 1,
                    &conflicting_tx_digests,
                    &TransactionDigest::random()
                )
                .await,
            Ok(None)
        );

        Ok(())
    }
}
