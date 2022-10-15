// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod metrics;
pub use metrics::*;

use arc_swap::ArcSwap;
use std::sync::Arc;
use sui_types::committee::{Committee, EpochId};

use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::Instrument;
use tracing::{debug, warn};

use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::AuthorityAPI;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{
    CertifiedTransaction, CertifiedTransactionEffects, QuorumDriverRequest,
    QuorumDriverRequestType, QuorumDriverResponse, Transaction,
};

const TASK_QUEUE_SIZE: usize = 5000;

pub enum QuorumTask {
    ProcessTransaction(Transaction),
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
        debug!(?tx_digest, "Received tranasction execution request");
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
        transaction: Transaction,
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
        transaction: Transaction,
    ) -> SuiResult<QuorumDriverResponse> {
        let certificate = self
            .process_transaction(transaction)
            .instrument(tracing::debug_span!("process_tx"))
            .await?;
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
        transaction: Transaction,
    ) -> SuiResult<QuorumDriverResponse> {
        let certificate = self
            .process_transaction(transaction)
            .instrument(tracing::debug_span!("process_tx"))
            .await?;
        let response = self
            .process_certificate(certificate)
            .instrument(tracing::debug_span!("process_cert"))
            .await?;
        Ok(QuorumDriverResponse::EffectsCert(Box::new(response)))
    }

    pub async fn process_transaction(
        &self,
        transaction: Transaction,
    ) -> SuiResult<CertifiedTransaction> {
        let tx_digest = *transaction.digest();
        self.validators
            .load()
            .process_transaction(transaction)
            .instrument(tracing::debug_span!("process_tx", ?tx_digest))
            .await
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
            tokio::task::spawn(async move {
                Self::task_queue_processor(quorum_driver_copy, task_rx).await;
            })
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
    /// then all of them needs to do reconfigs on their own.
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
            tokio::task::spawn(async move {
                Self::task_queue_processor(quorum_driver_copy, task_rx).await;
            })
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
