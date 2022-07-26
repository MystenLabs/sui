// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_synchronizer::{
        handler::Error::{BlockDeliveryTimeout, BlockNotFound, Internal, PayloadSyncError},
        BlockSynchronizeResult, Command, SyncError,
    },
    BlockHeader,
};
use async_trait::async_trait;
use crypto::{traits::VerifyingKey, Hash};
use futures::future::join_all;
#[cfg(test)]
use mockall::*;
use std::time::Duration;
use store::Store;
use thiserror::Error;
use tokio::{
    sync::mpsc::{channel, Sender},
    time::timeout,
};
use tracing::{debug, error, instrument, trace};
use types::{Certificate, CertificateDigest, PrimaryMessage};

#[cfg(test)]
#[path = "tests/handler_tests.rs"]
mod handler_tests;

/// The errors returned by the Handler. It translates
/// also the errors returned from the block_synchronizer.
#[derive(Debug, Error, Copy, Clone)]
pub enum Error {
    #[error("Block with id {block_id} not found")]
    BlockNotFound { block_id: CertificateDigest },

    #[error("Block with id {block_id} couldn't be retrieved, internal error occurred")]
    Internal { block_id: CertificateDigest },

    #[error("Timed out while waiting for {block_id} to become available after submitting for processing")]
    BlockDeliveryTimeout { block_id: CertificateDigest },

    #[error("Payload for block with {block_id} couldn't be synchronized: {error}")]
    PayloadSyncError {
        block_id: CertificateDigest,
        error: SyncError,
    },
}

impl Error {
    pub fn block_id(&self) -> CertificateDigest {
        match *self {
            BlockNotFound { block_id }
            | Internal { block_id }
            | BlockDeliveryTimeout { block_id }
            | PayloadSyncError { block_id, .. } => block_id,
        }
    }
}

/// Handler defines an interface to allow us access the BlockSycnhronizer's
/// functionality in a synchronous way without having to deal with message
/// emission. The BlockSynchronizer on its own for the certificates that
/// fetches on the fly from peers doesn't care/deal about any other validation
/// checks than the basic verification offered via the certificate entity
/// it self. For that reason the Handler offers methods to submit the fetched
/// from peers certificates for further validation & processing (e.x ensure
/// parents history is causally complete) to the core and wait until the
/// certificate has been processed, before it returns it back as result.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait Handler<PublicKey: VerifyingKey> {
    /// It retrieves the requested blocks via the block_synchronizer making
    /// sure though that they are fully validated. The certificates will only
    /// be returned when they have properly processed via the core module
    /// and made sure all the requirements have been fulfilled.
    async fn get_and_synchronize_block_headers(
        &self,
        block_ids: Vec<CertificateDigest>,
    ) -> Vec<Result<Certificate<PublicKey>, Error>>;

    /// It retrieves the requested blocks via the block_synchronizer, but it
    /// doesn't synchronize the fetched headers, meaning that no processing
    /// will take place (causal completion etc).
    async fn get_block_headers(
        &self,
        block_ids: Vec<CertificateDigest>,
    ) -> Vec<BlockSynchronizeResult<BlockHeader<PublicKey>>>;

    /// Synchronizes the block payload for the provided certificates via the
    /// block synchronizer and returns the result back.
    async fn synchronize_block_payloads(
        &self,
        certificates: Vec<Certificate<PublicKey>>,
    ) -> Vec<Result<Certificate<PublicKey>, Error>>;
}

/// A helper struct to allow us access the block_synchronizer in a synchronous
/// way. It also offers methods to both fetch the certificates and way to
/// process them and causally complete their history.
pub struct BlockSynchronizerHandler<PublicKey: VerifyingKey> {
    /// Channel to send commands to the block_synchronizer.
    tx_block_synchronizer: Sender<Command<PublicKey>>,

    /// Channel to send the fetched certificates to Core for
    /// further processing, validation and possibly causal
    /// completion.
    tx_core: Sender<PrimaryMessage<PublicKey>>,

    /// The store that holds the certificates.
    certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,

    /// The timeout while waiting for a certificate to become available
    /// after submitting for processing to core.
    certificate_deliver_timeout: Duration,
}

impl<PublicKey: VerifyingKey> BlockSynchronizerHandler<PublicKey> {
    pub fn new(
        tx_block_synchronizer: Sender<Command<PublicKey>>,
        tx_core: Sender<PrimaryMessage<PublicKey>>,
        certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,
        certificate_deliver_timeout: Duration,
    ) -> Self {
        Self {
            tx_block_synchronizer,
            tx_core,
            certificate_store,
            certificate_deliver_timeout,
        }
    }

    #[instrument(level = "debug", skip_all)]
    async fn wait_all(
        &self,
        certificates: Vec<Certificate<PublicKey>>,
    ) -> Vec<Result<Certificate<PublicKey>, Error>> {
        let futures: Vec<_> = certificates
            .into_iter()
            .map(|c| self.wait(c.digest()))
            .collect();

        join_all(futures).await
    }

    #[instrument(level = "debug", skip_all, err)]
    async fn wait(&self, block_id: CertificateDigest) -> Result<Certificate<PublicKey>, Error> {
        if let Ok(result) = timeout(
            self.certificate_deliver_timeout,
            self.certificate_store.notify_read(block_id),
        )
        .await
        {
            result
                .map_err(|_| Error::Internal { block_id })?
                .ok_or(Error::BlockNotFound { block_id })
        } else {
            Err(Error::BlockDeliveryTimeout { block_id })
        }
    }
}

#[async_trait]
impl<PublicKey: VerifyingKey> Handler<PublicKey> for BlockSynchronizerHandler<PublicKey> {
    /// The method will return a separate result for each requested block id.
    /// If a certificate has been successfully retrieved (and processed via core
    /// if has been fetched from peers) then an OK result will be returned with the
    /// certificate value.
    /// In case of error, the following outcomes are possible:
    /// * BlockNotFound: Failed to retrieve the certificate either via the store or via the peers
    /// * Internal: An internal error caused
    /// * BlockDeliveryTimeout: Timed out while waiting for the certificate to become available
    /// after submitting it for processing to core
    #[instrument(level="debug", skip_all, fields(num_block_ids = block_ids.len()))]
    async fn get_and_synchronize_block_headers(
        &self,
        block_ids: Vec<CertificateDigest>,
    ) -> Vec<Result<Certificate<PublicKey>, Error>> {
        if block_ids.is_empty() {
            trace!("No blocks were provided, will now return an empty list");
            return vec![];
        }

        let sync_results = self.get_block_headers(block_ids).await;
        let mut results: Vec<Result<Certificate<PublicKey>, Error>> = Vec::new();

        // send certificates to core for processing and potential
        // causal completion
        let mut wait_for = Vec::new();

        for result in sync_results {
            match result {
                Ok(block_header) => {
                    if !block_header.fetched_from_storage {
                        // we need to perform causal completion since this
                        // entity has not been fetched from storage.
                        self.tx_core
                            .send(PrimaryMessage::Certificate(
                                block_header.certificate.clone(),
                            ))
                            .await
                            .expect("Couldn't send certificate to core");
                        wait_for.push(block_header.certificate.clone());

                        debug!(
                            "Need to causally complete {}",
                            block_header.certificate.digest()
                        );
                    } else {
                        // Otherwise, if certificate fetched from storage, just
                        // add directly the certificate to the results - no need
                        // for further processing, validation, causal completion
                        // as all that have already happened.
                        results.push(Ok(block_header.certificate));
                    }
                }
                Err(err) => {
                    error!(
                        "Error occurred while synchronizing requested certificate {:?}",
                        err
                    );
                    results.push(Err(BlockNotFound {
                        block_id: err.block_id(),
                    }));
                }
            }
        }

        // now wait for the certificates to become available - timeout so we can
        // serve requests.
        let mut wait_results = self.wait_all(wait_for).await;

        // append the results we were waiting for
        results.append(&mut wait_results);

        results
    }

    #[instrument(level="debug", skip_all, fields(num_block_ids = block_ids.len()))]
    async fn get_block_headers(
        &self,
        block_ids: Vec<CertificateDigest>,
    ) -> Vec<BlockSynchronizeResult<BlockHeader<PublicKey>>> {
        if block_ids.is_empty() {
            trace!("No blocks were provided, will now return an empty list");
            return vec![];
        }

        let (tx, mut rx) = channel(block_ids.len());

        self.tx_block_synchronizer
            .send(Command::SynchronizeBlockHeaders {
                block_ids,
                respond_to: tx,
            })
            .await
            .expect("Couldn't send message to block synchronizer");

        // now wait to retrieve all the results
        let mut results = Vec::new();

        // We want to block and wait until we get all the results back.
        loop {
            match rx.recv().await {
                None => {
                    trace!("Channel closed when getting certificates, no more messages to get");
                    break;
                }
                Some(result) => results.push(result),
            }
        }

        results
    }

    #[instrument(level = "debug", skip_all)]
    async fn synchronize_block_payloads(
        &self,
        certificates: Vec<Certificate<PublicKey>>,
    ) -> Vec<Result<Certificate<PublicKey>, Error>> {
        if certificates.is_empty() {
            trace!("No certificates were provided, will now return an empty list");
            return vec![];
        }

        let (tx, mut rx) = channel(certificates.len());

        self.tx_block_synchronizer
            .send(Command::SynchronizeBlockPayload {
                certificates,
                respond_to: tx,
            })
            .await
            .expect("Couldn't send message to block synchronizer");

        // now wait to retrieve all the results
        let mut results = Vec::new();

        // We want to block and wait until we get all the results back.
        loop {
            match rx.recv().await {
                None => {
                    trace!("Channel closed when getting results, no more messages to get");
                    break;
                }
                Some(result) => {
                    let r = result
                        .map(|h| h.certificate)
                        .map_err(|e| Error::PayloadSyncError {
                            block_id: e.block_id(),
                            error: e,
                        });

                    if let Err(err) = r {
                        error!(
                            "Error for payload synchronization with block id {}, error: {err}",
                            err.block_id()
                        );
                    }

                    results.push(r)
                }
            }
        }

        results
    }
}
