// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::ArcSwap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::log::{error, warn};
use tracing::Instrument;

use sui_core::authority_aggregator::AuthorityAggregator;
use sui_core::authority_client::AuthorityAPI;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{
    CertifiedTransaction, ExecuteTransactionRequest, ExecuteTransactionRequestType,
    ExecuteTransactionResponse, Transaction, TransactionEffects,
};

const DEFAULT_PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

enum QuorumTask<A> {
    ProcessTransaction(Transaction),
    ProcessCertificate(CertifiedTransaction),
    UpdateValidators(AuthorityAggregator<A>),
}

pub struct QuorumDriverHandler<A> {
    quorum_driver: Arc<QuorumDriver<A>>,
    _processor_handle: JoinHandle<()>,
    task_sender: Mutex<Sender<QuorumTask<A>>>,
    // TODO: Change to CertifiedTransactionEffects eventually.
    effects_subscriber: Mutex<Receiver<(CertifiedTransaction, TransactionEffects)>>,
}

struct QuorumDriver<A> {
    validators: ArcSwap<AuthorityAggregator<A>>,
}

impl<A> QuorumDriver<A> {
    pub fn new(validators: AuthorityAggregator<A>) -> Self {
        Self {
            validators: ArcSwap::from(Arc::new(validators)),
        }
    }
}

impl<A> QuorumDriverHandler<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(validators: AuthorityAggregator<A>) -> Self {
        let quorum_driver = Arc::new(QuorumDriver::new(validators));
        let (task_tx, task_rx) = mpsc::channel::<QuorumTask<A>>(5000);
        let (subscriber_tx, subscriber_rx) = mpsc::channel::<_>(5000);
        let handle = {
            let task_tx_copy = task_tx.clone();
            let quorum_driver_copy = quorum_driver.clone();
            tokio::task::spawn(async move {
                Self::task_queue_processor(
                    quorum_driver_copy,
                    task_rx,
                    task_tx_copy,
                    subscriber_tx,
                )
                .await;
            })
        };
        Self {
            quorum_driver,
            _processor_handle: handle,
            task_sender: Mutex::new(task_tx),
            effects_subscriber: Mutex::new(subscriber_rx),
        }
    }

    pub async fn next_effects(&self) -> Option<(CertifiedTransaction, TransactionEffects)> {
        self.effects_subscriber.lock().await.recv().await
    }

    pub async fn update_validators(&self, new_validators: AuthorityAggregator<A>) -> SuiResult {
        self.task_sender
            .lock()
            .await
            .send(QuorumTask::UpdateValidators(new_validators))
            .await
            .map_err(|err| SuiError::QuorumDriverCommunicationError {
                error: err.to_string(),
            })
    }

    async fn task_queue_processor(
        quorum_driver: Arc<QuorumDriver<A>>,
        mut task_receiver: Receiver<QuorumTask<A>>,
        task_sender: Sender<QuorumTask<A>>,
        subscriber_tx: Sender<(CertifiedTransaction, TransactionEffects)>,
    ) {
        loop {
            if let Some(task) = task_receiver.recv().await {
                match task {
                    QuorumTask::ProcessTransaction(transaction) => {
                        match Self::process_transaction(
                            &quorum_driver,
                            transaction,
                            DEFAULT_PROCESS_TIMEOUT,
                        )
                        .await
                        {
                            Ok(cert) => {
                                if let Err(err) =
                                    task_sender.send(QuorumTask::ProcessCertificate(cert)).await
                                {
                                    // TODO: Is this sufficient? Should we retry sending?
                                    error!(
                                        "Sending task to quorum driver queue failed: {}",
                                        err.to_string()
                                    );
                                }
                            }
                            Err(err) => {
                                // TODO: Is there a way to notify the sender?
                                warn!("Transaction processing failed: {:?}", err);
                            }
                        }
                    }
                    QuorumTask::ProcessCertificate(certificate) => {
                        match Self::process_certificate(
                            &quorum_driver,
                            certificate,
                            DEFAULT_PROCESS_TIMEOUT,
                        )
                        .await
                        {
                            Ok(result) => {
                                if let Err(err) = subscriber_tx.send(result).await {
                                    // TODO: Is this sufficient? Should we retry sending?
                                    error!(
                                        "Sending effects to the subscriber channel failed: {:?}",
                                        err
                                    );
                                }
                            }
                            Err(err) => {
                                // TODO: Is there a way to notify the sender?
                                warn!("Certificate processing failed: {:?}", err);
                            }
                        }
                    }
                    QuorumTask::UpdateValidators(new_validators) => {
                        quorum_driver.validators.store(Arc::new(new_validators));
                    }
                }
            }
        }
    }

    async fn process_transaction(
        quorum_driver: &Arc<QuorumDriver<A>>,
        transaction: Transaction,
        timeout: Duration,
    ) -> SuiResult<CertifiedTransaction> {
        quorum_driver
            .validators
            .load()
            .process_transaction(transaction, timeout)
            .instrument(tracing::debug_span!("process_tx"))
            .await
    }

    async fn process_certificate(
        quorum_driver: &Arc<QuorumDriver<A>>,
        certificate: CertifiedTransaction,
        timeout: Duration,
    ) -> SuiResult<(CertifiedTransaction, TransactionEffects)> {
        let effects = quorum_driver
            .validators
            .load()
            .process_certificate(certificate.clone(), timeout)
            .instrument(tracing::debug_span!("process_cert"))
            .await?;
        Ok((certificate, effects))
    }
}

impl<A> QuorumDriverHandler<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub async fn execute_transaction(
        &self,
        request: ExecuteTransactionRequest,
    ) -> SuiResult<ExecuteTransactionResponse> {
        let ExecuteTransactionRequest {
            transaction,
            request_type,
        } = request;
        match request_type {
            ExecuteTransactionRequestType::ImmediateReturn => {
                self.task_sender
                    .lock()
                    .await
                    .send(QuorumTask::ProcessTransaction(transaction))
                    .await
                    .map_err(|err| SuiError::QuorumDriverCommunicationError {
                        error: err.to_string(),
                    })?;
                Ok(ExecuteTransactionResponse::ImmediateReturn)
            }
            ExecuteTransactionRequestType::WaitForTxCert(timeout) => {
                let certificate = QuorumDriverHandler::process_transaction(
                    &self.quorum_driver,
                    transaction,
                    timeout,
                )
                .instrument(tracing::debug_span!("process_tx"))
                .await?;
                self.task_sender
                    .lock()
                    .await
                    .send(QuorumTask::ProcessCertificate(certificate.clone()))
                    .await
                    .map_err(|err| SuiError::QuorumDriverCommunicationError {
                        error: err.to_string(),
                    })?;
                Ok(ExecuteTransactionResponse::TxCert(Box::new(certificate)))
            }
            ExecuteTransactionRequestType::WaitForEffectsCert(timeout) => {
                let instant = Instant::now();
                let certificate = QuorumDriverHandler::process_transaction(
                    &self.quorum_driver,
                    transaction,
                    timeout,
                )
                .instrument(tracing::debug_span!("process_tx"))
                .await?;
                let response = QuorumDriverHandler::process_certificate(
                    &self.quorum_driver,
                    certificate,
                    timeout - instant.elapsed(),
                )
                .instrument(tracing::debug_span!("process_cert"))
                .await?;
                Ok(ExecuteTransactionResponse::EffectsCert(Box::new(response)))
            }
        }
    }
}
