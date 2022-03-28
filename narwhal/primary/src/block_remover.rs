// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
#![allow(unused_variables)]

use crate::{
    block_remover::BlockErrorType::Failed, BatchDigest, Certificate, CertificateDigest, Header,
    HeaderDigest, PayloadToken, PrimaryWorkerMessage,
};
use bytes::Bytes;
use config::{Committee, WorkerId};
use crypto::{traits::VerifyingKey, Digest, Hash};
use futures::{
    future::{join_all, try_join_all, BoxFuture},
    stream::{futures_unordered::FuturesUnordered, StreamExt as _},
    FutureExt,
};
use network::SimpleSender;
use std::{collections::HashMap, time::Duration};
use store::{rocks::TypedStoreError, Store};
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
    task::JoinHandle,
    time::timeout,
};
use tracing::{
    error,
    log::{debug, warn},
};

const BATCH_DELETE_TIMEOUT: Duration = Duration::from_secs(2);

type RequestKey = Vec<u8>;

#[cfg(test)]
#[path = "tests/block_remover_tests.rs"]
pub mod block_remover_tests;

#[derive(Debug)]
pub enum BlockRemoverCommand {
    RemoveBlocks {
        // the block ids to remove
        ids: Vec<CertificateDigest>,
        // the channel to communicate the results
        sender: Sender<BlockRemoverResult<RemoveBlocksResponse>>,
    },
}

#[derive(Clone, Debug)]
pub struct RemoveBlocksResponse {
    // the block ids to remove
    ids: Vec<CertificateDigest>,
}

type BlockRemoverResult<T> = Result<T, BlockRemoverError>;

#[derive(Clone, Debug)]
pub struct BlockRemoverError {
    ids: Vec<CertificateDigest>,
    error: BlockErrorType,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlockErrorType {
    Timeout,
    Failed,
}

pub type DeleteBatchResult = Result<DeleteBatchMessage, DeleteBatchMessage>;

#[derive(Clone, Default, Debug)]
pub struct DeleteBatchMessage {
    pub ids: Vec<BatchDigest>,
}

/// BlockRemover is responsible for removing blocks identified by
/// their certificate id (digest) from across our system. On high level
/// It will make sure that the DAG is updated, internal storage where
/// there certificates and headers are stored, and the corresponding
/// batches as well.
///
/// # Example
///
/// Basic setup of the BlockRemover
///
/// This example shows the basic setup of the BlockRemover module. It showcases
/// the necessary components that have to be used (e.x channels, datastore etc)
/// and how a request (command) should be issued to delete a list of blocks and receive
/// the result of it.
///
/// ```rust
/// # use store::{reopen, rocks, rocks::DBMap, Store};
/// # use network::SimpleSender;
/// # use tokio::sync::mpsc::{channel};
/// # use crypto::Hash;
/// # use std::env::temp_dir;
/// # use crypto::Digest;
/// # use crypto::ed25519::Ed25519PublicKey;
/// # use config::Committee;
/// # use std::collections::BTreeMap;
/// # use primary::Certificate;
/// # use config::WorkerId;
/// # use primary::{BlockRemover, BlockRemoverCommand, DeleteBatchMessage, Header, PayloadToken};
/// # use primary::{BatchDigest, CertificateDigest, HeaderDigest};
///
/// #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// const CERTIFICATES_CF: &str = "certificates";
///     const HEADERS_CF: &str = "headers";
///     const PAYLOAD_CF: &str = "payload";
///
///     // Basic setup: datastore, channels & BlockWaiter
///     let rocksdb = rocks::open_cf(temp_dir(), None, &[CERTIFICATES_CF, HEADERS_CF, PAYLOAD_CF])
///         .expect("Failed creating database");
///
///     let (certificate_map, headers_map, payload_map) = reopen!(&rocksdb,
///             CERTIFICATES_CF;<CertificateDigest, Certificate<Ed25519PublicKey>>,
///             HEADERS_CF;<HeaderDigest, Header<Ed25519PublicKey>>,
///             PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>);
///     let certificate_store = Store::new(certificate_map);
///     let headers_store = Store::new(headers_map);
///     let payload_store = Store::new(payload_map);
///
///     let (tx_commands, rx_commands) = channel(1);
///     let (tx_delete_batches, rx_delete_batches) = channel(1);
///     let (tx_delete_block_result, mut rx_delete_block_result) = channel(1);
///
///     let name = Ed25519PublicKey::default();
///     let committee = Committee{ authorities: BTreeMap::new() };
///
///     BlockRemover::spawn(
///         name,
///         committee,
///         certificate_store.clone(),
///         headers_store.clone(),
///         payload_store.clone(),
///         SimpleSender::new(),
///         rx_commands,
///         rx_delete_batches,
///     );
///
///     // A dummy certificate
///     let certificate = Certificate::<Ed25519PublicKey>::default();
///
///     // Send a command to receive a block
///     tx_commands
///         .send(BlockRemoverCommand::RemoveBlocks {
///             ids: vec![certificate.clone().digest()],
///             sender: tx_delete_block_result,
///         })
///         .await;
///
///     // Dummy - we expect to receive the deleted batches responses via another component
///     // and get fed via the tx_delete_batches channel.
///     tx_delete_batches.send(Ok(DeleteBatchMessage{ ids: vec![BatchDigest::default()] })).await;
///
///     // Wait to receive the blocks delete output to the provided sender channel
///     match rx_delete_block_result.recv().await {
///         Some(Ok(result)) => {
///             println!("Successfully received a delete blocks response");
///         }
///         Some(Err(err)) => {
///             println!("Received an error {:?}", err);
///         }
///         _ => {
///             println!("Nothing received");
///         }
///     }
/// # }
/// ```
pub struct BlockRemover<PublicKey: VerifyingKey> {
    /// The public key of this primary.
    name: PublicKey,

    /// The committee information.
    committee: Committee<PublicKey>,

    /// Storage that keeps the Certificates by their digest id.
    certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,

    /// Storage that keeps the headers by their digest id
    header_store: Store<HeaderDigest, Header<PublicKey>>,

    /// The persistent storage for payload markers from workers.
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,

    /// Network driver allowing to send messages.
    network: SimpleSender,

    /// Receives the commands to execute against
    rx_commands: Receiver<BlockRemoverCommand>,

    /// Checks whether a pending request already exists
    pending_removal_requests: HashMap<RequestKey, Vec<Certificate<PublicKey>>>,

    /// Holds the senders that are pending to be notified for
    /// a removal request.
    map_tx_removal_results:
        HashMap<RequestKey, Vec<Sender<BlockRemoverResult<RemoveBlocksResponse>>>>,

    map_tx_worker_removal_results: HashMap<RequestKey, oneshot::Sender<DeleteBatchResult>>,

    /// Receives all the responses to the requests to delete a batch.
    rx_delete_batches: Receiver<DeleteBatchResult>,
}

impl<PublicKey: VerifyingKey> BlockRemover<PublicKey> {
    pub fn spawn(
        name: PublicKey,
        committee: Committee<PublicKey>,
        certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,
        header_store: Store<HeaderDigest, Header<PublicKey>>,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        network: SimpleSender,
        rx_commands: Receiver<BlockRemoverCommand>,
        rx_delete_batches: Receiver<DeleteBatchResult>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                certificate_store,
                header_store,
                payload_store,
                network,
                rx_commands,
                pending_removal_requests: HashMap::new(),
                map_tx_removal_results: HashMap::new(),
                map_tx_worker_removal_results: HashMap::new(),
                rx_delete_batches,
            }
            .run()
            .await;
        })
    }

    async fn run(&mut self) {
        let mut waiting = FuturesUnordered::new();

        loop {
            tokio::select! {
                Some(command) = self.rx_commands.recv() => {
                    if let Some(fut) = self.handle_command(command).await {
                        waiting.push(fut);
                    }
                },
                Some(batch_result) = self.rx_delete_batches.recv() => {
                    self.handle_delete_batch_result(batch_result).await;
                },
                Some(result) = waiting.next() => {
                    self.handle_remove_waiting_result(result).await;
                }
            }
        }
    }

    async fn handle_remove_waiting_result(
        &mut self,
        result: BlockRemoverResult<RemoveBlocksResponse>,
    ) {
        let block_ids = result.clone().map_or_else(|e| e.ids, |r| r.ids);
        let key = Self::construct_blocks_request_key(&block_ids);

        match self.map_tx_removal_results.remove(&key) {
            None => {
                error!(
                    "We should expect to have a channel to send the results for key {:?}",
                    key
                );
            }
            Some(senders) => {
                let cleanup_successful = match self.pending_removal_requests.remove(&key) {
                    None => {
                        error!("Expected to find pending request for this key {:?}", &key);
                        Err(())
                    }
                    Some(certificates) => {
                        let batches_by_worker =
                            Self::map_batches_by_worker(certificates.as_slice());
                        // Clean up any possible pending result channel (e.x in case of a timeout channels are not cleaned up)
                        // So we ensure that we "unlock" the pending request and give the opportunity
                        // a request to re-execute if the downstream clean up operations fail.
                        for batch_ids in batches_by_worker.values() {
                            let request_key = Self::construct_batches_request_key(batch_ids);

                            self.map_tx_worker_removal_results.remove(&request_key);
                        }

                        // Do the further clean up only if the result is successful
                        if result.is_ok() {
                            self.cleanup_internal_state(certificates, batches_by_worker)
                                .await
                                .map_err(|e| ())
                        } else {
                            Ok(())
                        }
                    }
                };

                // if the clean up is successful we want to send the result
                // whatever this is. Otherwise, we are sending back an error
                // response.
                if cleanup_successful.is_ok() {
                    Self::broadcast(senders, result).await;
                } else {
                    Self::broadcast(
                        senders,
                        Err(BlockRemoverError {
                            ids: block_ids,
                            error: BlockErrorType::Failed,
                        }),
                    )
                    .await;
                }
            }
        }
    }

    /// Helper method to broadcast the result_to_send to all the senders.
    async fn broadcast(
        senders: Vec<Sender<BlockRemoverResult<RemoveBlocksResponse>>>,
        result_to_send: BlockRemoverResult<RemoveBlocksResponse>,
    ) {
        let futures: Vec<_> = senders
            .iter()
            .map(|s| s.send(result_to_send.clone()))
            .collect();

        for r in join_all(futures).await {
            if r.is_err() {
                error!("Couldn't send message to channel [{:?}]", r.err().unwrap());
            }
        }
    }

    async fn cleanup_internal_state(
        &mut self,
        certificates: Vec<Certificate<PublicKey>>,
        batches_by_worker: HashMap<WorkerId, Vec<BatchDigest>>,
    ) -> Result<(), TypedStoreError> {
        let header_ids: Vec<HeaderDigest> = certificates
            .clone()
            .into_iter()
            .map(|c| c.header.id)
            .collect();

        self.header_store.remove_all(header_ids).await?;

        // delete batch from the payload store as well
        let mut batches_to_cleanup: Vec<(BatchDigest, WorkerId)> = Vec::new();
        for (worker_id, batch_ids) in batches_by_worker {
            batch_ids.into_iter().for_each(|d| {
                batches_to_cleanup.push((d, worker_id));
            })
        }
        self.payload_store.remove_all(batches_to_cleanup).await?;

        /* * * * * * * * * * * * * * * * * * * * * * * * *
         *         TODO: DAG deletion could go here?
         * * * * * * * * * * * * * * * * * * * * * * * * */

        // NOTE: delete certificates in the end since if we need to repeat the request
        // we want to be able to find them in storage.
        let certificate_ids: Vec<CertificateDigest> =
            certificates.as_slice().iter().map(|c| c.digest()).collect();
        self.certificate_store.remove_all(certificate_ids).await?;

        Ok(())
    }

    async fn handle_delete_batch_result(&mut self, batch_result: DeleteBatchResult) {
        let ids = batch_result.clone().map_or_else(|e| e.ids, |r| r.ids);
        let key = Self::construct_batches_request_key(&ids);

        if let Some(sender) = self.map_tx_worker_removal_results.remove(&key) {
            sender
                .send(batch_result)
                .expect("couldn't send delete batch result to channel");
        } else {
            error!("no pending delete request has been found for key {:?}", key);
        }
    }

    async fn handle_command<'a>(
        &mut self,
        command: BlockRemoverCommand,
    ) -> Option<BoxFuture<'a, BlockRemoverResult<RemoveBlocksResponse>>> {
        match command {
            BlockRemoverCommand::RemoveBlocks { ids, sender } => {
                // check whether we have a similar request pending
                // to make the check easy we sort the digests in asc order,
                // and then we merge all the bytes to form a key
                let key = Self::construct_blocks_request_key(&ids);

                if self.pending_removal_requests.contains_key(&key) {
                    // request already pending, nothing to do, just add the sender to the list
                    // of pending to be notified ones.
                    self.map_tx_removal_results
                        .entry(key)
                        .or_insert_with(Vec::new)
                        .push(sender);

                    debug!("Removal blocks has an already pending request for the provided ids");
                    return None;
                }

                // find the blocks in certificates store
                match self.certificate_store.read_all(ids.clone()).await {
                    Ok(certificates) => {
                        let non_found_certificates: Vec<(
                            Option<Certificate<PublicKey>>,
                            CertificateDigest,
                        )> = certificates
                            .clone()
                            .into_iter()
                            .zip(ids.clone())
                            .filter(|(c, digest)| c.is_none())
                            .collect();

                        if !non_found_certificates.is_empty() {
                            let c: Vec<CertificateDigest> =
                                non_found_certificates.into_iter().map(|(c, d)| d).collect();
                            warn!("Some certificates are missing, will ignore them {:?}", c);
                        }

                        // ensure that we store only the found certificates
                        let found_certificates: Vec<Certificate<PublicKey>> =
                            certificates.into_iter().flatten().collect();

                        let receivers = self
                            .send_delete_requests_to_workers(found_certificates.clone())
                            .await;

                        // now spin up a waiter
                        let fut = Self::wait_for_responses(ids, receivers);

                        // add the certificates on the pending map
                        self.pending_removal_requests
                            .insert(key.clone(), found_certificates);

                        // add the sender to the pending map
                        self.map_tx_removal_results
                            .entry(key)
                            .or_insert_with(Vec::new)
                            .push(sender);

                        return Some(fut.boxed());
                    }
                    Err(err) => {
                        error!("Error while reading certificate {:?}", err);

                        sender
                            .send(Err(BlockRemoverError { ids, error: Failed }))
                            .await
                            .expect("Couldn't send error to channel");
                    }
                }
            }
        }

        None
    }

    async fn send_delete_requests_to_workers(
        &mut self,
        certificates: Vec<Certificate<PublicKey>>,
    ) -> Vec<(RequestKey, oneshot::Receiver<DeleteBatchResult>)> {
        // For each certificate, batch the requests by worker
        // and send the requests
        let batches_by_worker = Self::map_batches_by_worker(certificates.as_slice());

        let mut receivers: Vec<(RequestKey, oneshot::Receiver<DeleteBatchResult>)> = Vec::new();

        // now send the requests
        for (worker_id, batch_ids) in batches_by_worker {
            // send the batches to each worker id
            let worker_address = self
                .committee
                .worker(&self.name, &worker_id)
                .expect("Worker id not found")
                .primary_to_worker;

            let message = PrimaryWorkerMessage::<PublicKey>::DeleteBatches(batch_ids.clone());
            let serialised_message =
                bincode::serialize(&message).expect("Failed to serialize delete batch request");

            // send the request
            self.network
                .send(worker_address, Bytes::from(serialised_message))
                .await;

            // create a key based on the provided batch ids and use it as a request
            // key to identify the channel to forward the response once the delete
            // response is received.
            let worker_request_key = Self::construct_batches_request_key(&batch_ids);

            let (tx, rx) = oneshot::channel();
            receivers.push((worker_request_key.clone(), rx));

            self.map_tx_worker_removal_results
                .insert(worker_request_key, tx);
        }

        receivers
    }

    async fn wait_for_responses(
        block_ids: Vec<CertificateDigest>,
        receivers: Vec<(RequestKey, oneshot::Receiver<DeleteBatchResult>)>,
    ) -> BlockRemoverResult<RemoveBlocksResponse> {
        let waiting: Vec<_> = receivers
            .into_iter()
            .map(|p| Self::wait_for_delete_response(p.0, p.1))
            .collect();

        let result = try_join_all(waiting).await;
        if result.as_ref().is_ok() {
            return Ok(RemoveBlocksResponse { ids: block_ids });
        }

        Err(BlockRemoverError {
            ids: block_ids,
            error: result.err().unwrap(),
        })
    }

    // this method waits to receive the result to a DeleteBatchRequest. We only care to report
    // a successful delete or not. It returns true if the batches have been successfully deleted,
    // otherwise false in any other case.
    async fn wait_for_delete_response(
        request_key: RequestKey,
        rx: oneshot::Receiver<DeleteBatchResult>,
    ) -> Result<RequestKey, BlockErrorType> {
        match timeout(BATCH_DELETE_TIMEOUT, rx).await {
            Ok(Ok(_)) => Ok(request_key),
            Ok(Err(_)) => Err(BlockErrorType::Failed),
            Err(_) => Err(BlockErrorType::Timeout),
        }
    }

    fn construct_blocks_request_key(ids: &[CertificateDigest]) -> RequestKey {
        let mut ids_cloned = ids.to_vec();
        ids_cloned.sort();

        // TODO: find a better way to construct request keys
        // Issue: https://github.com/MystenLabs/narwhal/issues/94

        let result: RequestKey = ids_cloned
            .into_iter()
            .flat_map(|d| Digest::from(d).to_vec())
            .collect();

        result
    }

    fn construct_batches_request_key(ids: &[BatchDigest]) -> RequestKey {
        let mut ids_cloned = ids.to_vec();
        ids_cloned.sort();

        // TODO: find a better way to construct request keys
        // Issue: https://github.com/MystenLabs/narwhal/issues/94

        let result: RequestKey = ids_cloned
            .into_iter()
            .flat_map(|d| Digest::from(d).to_vec())
            .collect();

        result
    }

    // a helper method that collects all the batches from each certificate and maps
    // them by the worker id.
    fn map_batches_by_worker(
        certificates: &[Certificate<PublicKey>],
    ) -> HashMap<WorkerId, Vec<BatchDigest>> {
        let mut batches_by_worker: HashMap<WorkerId, Vec<BatchDigest>> = HashMap::new();
        for certificate in certificates.iter() {
            for (batch_id, worker_id) in &certificate.header.payload {
                batches_by_worker
                    .entry(*worker_id)
                    .or_insert_with(Vec::new)
                    .push(*batch_id);
            }
        }

        batches_by_worker
    }
}
