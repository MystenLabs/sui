// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::ArcSwap;
use std::sync::Arc;

use tokio::sync::mpsc::{self, Receiver, Sender};
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

pub enum QuorumTask<A> {
    ProcessTransaction(Transaction),
    ProcessCertificate(CertifiedTransaction),
    UpdateCommittee(AuthorityAggregator<A>),
}

/// A handler to wrap around QuorumDriver. This handler should be owned by the node with exclusive
/// mutability.
pub struct QuorumDriverHandler<A> {
    quorum_driver: Arc<QuorumDriver<A>>,
    _processor_handle: JoinHandle<()>,
    // TODO: Change to CertifiedTransactionEffects eventually.
    effects_subscriber: Receiver<(CertifiedTransaction, TransactionEffects)>,
}

/// The core data structure of the QuorumDriver.
/// It's expected that the QuorumDriver will be wrapped in an `Arc` and shared around.
/// One copy will be used in a json-RPC server to serve transaction execution requests;
/// Another copy will be held by a QuorumDriverHandler to either send signal to update the
/// committee, or to subscribe effects generated from the QuorumDriver.
pub struct QuorumDriver<A> {
    validators: ArcSwap<AuthorityAggregator<A>>,
    task_sender: Sender<QuorumTask<A>>,
    effects_subscribe_sender: Sender<(CertifiedTransaction, TransactionEffects)>,
}

impl<A> QuorumDriver<A> {
    pub fn new(
        validators: AuthorityAggregator<A>,
        task_sender: Sender<QuorumTask<A>>,
        effects_subscribe_sender: Sender<(CertifiedTransaction, TransactionEffects)>,
    ) -> Self {
        Self {
            validators: ArcSwap::from(Arc::new(validators)),
            task_sender,
            effects_subscribe_sender,
        }
    }
}

impl<A> QuorumDriver<A>
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
                    .send(QuorumTask::ProcessTransaction(transaction))
                    .await
                    .map_err(|err| SuiError::QuorumDriverCommunicationError {
                        error: err.to_string(),
                    })?;
                Ok(ExecuteTransactionResponse::ImmediateReturn)
            }
            ExecuteTransactionRequestType::WaitForTxCert => {
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
                Ok(ExecuteTransactionResponse::TxCert(Box::new(certificate)))
            }
            ExecuteTransactionRequestType::WaitForEffectsCert => {
                let certificate = self
                    .process_transaction(transaction)
                    .instrument(tracing::debug_span!("process_tx"))
                    .await?;
                let response = self
                    .process_certificate(certificate)
                    .instrument(tracing::debug_span!("process_cert"))
                    .await?;
                Ok(ExecuteTransactionResponse::EffectsCert(Box::new(response)))
            }
        }
    }

    pub async fn process_transaction(
        &self,
        transaction: Transaction,
    ) -> SuiResult<CertifiedTransaction> {
        self.validators
            .load()
            .process_transaction(transaction)
            .instrument(tracing::debug_span!("process_tx"))
            .await
    }

    pub async fn process_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> SuiResult<(CertifiedTransaction, TransactionEffects)> {
        let effects = self
            .validators
            .load()
            .process_certificate(certificate.clone())
            .instrument(tracing::debug_span!("process_cert"))
            .await?;
        let response = (certificate, effects);
        // An error to send the result to subscribers should not block returning the result.
        if let Err(err) = self.effects_subscribe_sender.send(response.clone()).await {
            // TODO: We could potentially retry sending if we want.
            error!("{}", err);
        }
        Ok(response)
    }
}

impl<A> QuorumDriverHandler<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(validators: AuthorityAggregator<A>) -> Self {
        let (task_tx, task_rx) = mpsc::channel::<QuorumTask<A>>(5000);
        let (subscriber_tx, subscriber_rx) = mpsc::channel::<_>(5000);
        let quorum_driver = Arc::new(QuorumDriver::new(validators, task_tx, subscriber_tx));
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
        }
    }

    pub fn clone_quorum_driver(&self) -> Arc<QuorumDriver<A>> {
        self.quorum_driver.clone()
    }

    pub fn subscribe(&mut self) -> &mut Receiver<(CertifiedTransaction, TransactionEffects)> {
        &mut self.effects_subscriber
    }

    pub async fn update_validators(&self, new_validators: AuthorityAggregator<A>) -> SuiResult {
        self.quorum_driver
            .task_sender
            .send(QuorumTask::UpdateCommittee(new_validators))
            .await
            .map_err(|err| SuiError::QuorumDriverCommunicationError {
                error: err.to_string(),
            })
    }

    async fn task_queue_processor(
        quorum_driver: Arc<QuorumDriver<A>>,
        mut task_receiver: Receiver<QuorumTask<A>>,
    ) {
        loop {
            if let Some(task) = task_receiver.recv().await {
                match task {
                    QuorumTask::ProcessTransaction(transaction) => {
                        // TODO: We entered here because callers do not want to wait for a
                        // transaction to finish execution. When this failed, we do not have a
                        // way to notify the caller. In the future, we may want to maintain
                        // some data structure for callers to come back and query the status
                        // of a transaction latter.
                        match quorum_driver.process_transaction(transaction).await {
                            Ok(cert) => {
                                if let Err(err) = quorum_driver
                                    .task_sender
                                    .send(QuorumTask::ProcessCertificate(cert))
                                    .await
                                {
                                    error!(
                                        "Sending task to quorum driver queue failed: {}",
                                        err.to_string()
                                    );
                                }
                            }
                            Err(err) => {
                                warn!("Transaction processing failed: {:?}", err);
                            }
                        }
                    }
                    QuorumTask::ProcessCertificate(certificate) => {
                        // TODO: Similar to ProcessTransaction, we may want to allow callers to
                        // query the status.
                        if let Err(err) = quorum_driver.process_certificate(certificate).await {
                            warn!("Certificate processing failed: {:?}", err);
                        }
                    }
                    QuorumTask::UpdateCommittee(new_validators) => {
                        quorum_driver.validators.store(Arc::new(new_validators));
                    }
                }
            }
        }
    }
}
