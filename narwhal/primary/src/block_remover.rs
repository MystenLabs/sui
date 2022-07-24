// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
#![allow(unused_variables)]

use crate::{utils, PayloadToken};
use config::{Committee, WorkerId};
use consensus::dag::{Dag, ValidatorDagError};
use crypto::{traits::VerifyingKey, Digest, Hash};
use futures::{
    future::{join_all, try_join_all, BoxFuture},
    stream::{futures_unordered::FuturesUnordered, StreamExt as _},
    FutureExt,
};
use itertools::Either;
use network::PrimaryToWorkerNetwork;
use std::{collections::HashMap, sync::Arc, time::Duration};
use store::{rocks::TypedStoreError, Store};
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        oneshot, watch,
    },
    task::JoinHandle,
    time::timeout,
};
use tracing::{debug, error, instrument, warn};
use types::{
    BatchDigest, BlockRemoverError, BlockRemoverErrorKind, BlockRemoverResult, Certificate,
    CertificateDigest, Header, HeaderDigest, PrimaryWorkerMessage, ReconfigureNotification,
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
    // the block ids removed
    ids: Vec<CertificateDigest>,
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
/// # use network::PrimaryToWorkerNetwork;
/// # use tokio::sync::{mpsc::{channel}, watch};
/// # use arc_swap::ArcSwap;
/// # use crypto::Hash;
/// # use std::env::temp_dir;
/// # use crypto::Digest;
/// # use crypto::ed25519::Ed25519PublicKey;
/// # use config::Committee;
/// # use consensus::dag::Dag;
/// # use futures::future::join_all;
/// # use std::collections::BTreeMap;
/// # use std::sync::Arc;
/// # use types::ReconfigureNotification;
/// # use config::WorkerId;
/// # use tempfile::tempdir;
/// # use primary::{BlockRemover, BlockRemoverCommand, DeleteBatchMessage, PayloadToken};
/// # use types::{BatchDigest, Certificate, CertificateDigest, HeaderDigest, Header};
/// # use prometheus::Registry;
/// # use consensus::metrics::ConsensusMetrics;
///
/// #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
///     const CERTIFICATES_CF: &str = "certificates";
///     const HEADERS_CF: &str = "headers";
///     const PAYLOAD_CF: &str = "payload";
///
///     let temp_dir = tempdir().expect("Failed to open temporary directory").into_path();
///
///     // Basic setup: datastore, channels & BlockWaiter
///     let rocksdb = rocks::open_cf(temp_dir, None, &[CERTIFICATES_CF, HEADERS_CF, PAYLOAD_CF])
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
///     let (tx_removed_certificates, _rx_removed_certificates) = channel(1);
///     let (tx_delete_block_result, mut rx_delete_block_result) = channel(1);
///
///     let name = Ed25519PublicKey::default();
///     let committee = Committee{ epoch: 0, authorities: BTreeMap::new() };
///     let (_tx_reconfigure, rx_reconfigure) = watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
///     let consensus_metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
///     // A dag with genesis for the committee
///     let (tx_new_certificates, rx_new_certificates) = channel(1);
///     let dag = Arc::new(Dag::new(&committee, rx_new_certificates, consensus_metrics).1);
///     // Populate genesis in the Dag
///     join_all(
///       Certificate::genesis(&committee)
///       .iter()
///       .map(|cert| dag.insert(cert.clone())),
///     )
///     .await;
///
///     BlockRemover::spawn(
///         name,
///         committee,
///         certificate_store.clone(),
///         headers_store.clone(),
///         payload_store.clone(),
///         Some(dag),
///         PrimaryToWorkerNetwork::default(),
///         rx_reconfigure,
///         rx_commands,
///         rx_delete_batches,
///         tx_removed_certificates,
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

    /// The Dag structure for managing the stored certificates
    dag: Option<Arc<Dag<PublicKey>>>,

    /// Network driver allowing to send messages.
    worker_network: PrimaryToWorkerNetwork,

    /// Watch channel to reconfigure the committee.
    rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,

    /// Receives the commands to execute against
    rx_commands: Receiver<BlockRemoverCommand>,

    /// Checks whether a pending request already exists
    pending_removal_requests: HashMap<RequestKey, Vec<Certificate<PublicKey>>>,

    /// Holds the senders that are pending to be notified for
    /// a removal request.
    map_tx_removal_results:
        HashMap<RequestKey, Vec<Sender<BlockRemoverResult<RemoveBlocksResponse>>>>,

    map_tx_worker_removal_results: HashMap<RequestKey, oneshot::Sender<DeleteBatchResult>>,

    // TODO: Change to a oneshot channel instead of an mpsc channel
    /// Receives all the responses to the requests to delete a batch.
    rx_delete_batches: Receiver<DeleteBatchResult>,

    /// Outputs all the successfully deleted certificates
    tx_removed_certificates: Sender<Certificate<PublicKey>>,
}

impl<PublicKey: VerifyingKey> BlockRemover<PublicKey> {
    pub fn spawn(
        name: PublicKey,
        committee: Committee<PublicKey>,
        certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,
        header_store: Store<HeaderDigest, Header<PublicKey>>,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        dag: Option<Arc<Dag<PublicKey>>>,
        worker_network: PrimaryToWorkerNetwork,
        rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
        rx_commands: Receiver<BlockRemoverCommand>,
        rx_delete_batches: Receiver<DeleteBatchResult>,
        removed_certificates: Sender<Certificate<PublicKey>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                certificate_store,
                header_store,
                payload_store,
                dag,
                worker_network,
                rx_reconfigure,
                rx_commands,
                pending_removal_requests: HashMap::new(),
                map_tx_removal_results: HashMap::new(),
                map_tx_worker_removal_results: HashMap::new(),
                rx_delete_batches,
                tx_removed_certificates: removed_certificates,
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
                },
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewCommittee(new_committee) => {
                            self.committee = new_committee;
                            tracing::debug!("Committee updated to {}", self.committee);
                        }
                        ReconfigureNotification::Shutdown => return
                    }
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
                            utils::map_certificate_batches_by_worker(certificates.as_slice());
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
                                .map_err(|err| ())
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
                            error: BlockRemoverErrorKind::Failed,
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
    ) -> Result<(), Either<TypedStoreError, ValidatorDagError<PublicKey>>> {
        let header_ids: Vec<HeaderDigest> = certificates.iter().map(|c| c.header.id).collect();

        self.header_store
            .remove_all(header_ids)
            .await
            .map_err(Either::Left)?;

        // delete batch from the payload store as well
        let mut batches_to_cleanup: Vec<(BatchDigest, WorkerId)> = Vec::new();
        for (worker_id, batch_ids) in batches_by_worker {
            batch_ids.into_iter().for_each(|d| {
                batches_to_cleanup.push((d, worker_id));
            })
        }
        self.payload_store
            .remove_all(batches_to_cleanup)
            .await
            .map_err(Either::Left)?;

        // NOTE: delete certificates in the end since if we need to repeat the request
        // we want to be able to find them in storage.
        let certificate_ids: Vec<CertificateDigest> =
            certificates.as_slice().iter().map(|c| c.digest()).collect();
        if let Some(dag) = &self.dag {
            dag.remove(&certificate_ids).await.map_err(Either::Right)?
        }

        self.certificate_store
            .remove_all(certificate_ids)
            .await
            .map_err(Either::Left)?;

        // Now output all the removed certificates
        for certificate in certificates.clone() {
            self.tx_removed_certificates
                .send(certificate.clone())
                .await
                .expect("Couldn't forward removed certificates to channel");
        }

        debug!("Successfully cleaned up certificates: {:?}", certificates);

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

    #[instrument(level = "debug", skip_all)]
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
                            .send(Err(BlockRemoverError {
                                ids,
                                error: BlockRemoverErrorKind::Failed,
                            }))
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
        let batches_by_worker = utils::map_certificate_batches_by_worker(certificates.as_slice());

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

            // send the request
            self.worker_network
                .send(worker_address.clone(), &message)
                .await;

            debug!(
                "Sending DeleteBatches request for batch ids {:?} to worker {}",
                batch_ids.clone(),
                worker_address
            );

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
    ) -> Result<RequestKey, BlockRemoverErrorKind> {
        match timeout(BATCH_DELETE_TIMEOUT, rx).await {
            Ok(Ok(_)) => Ok(request_key),
            Ok(Err(_)) => Err(BlockRemoverErrorKind::Failed),
            Err(_) => Err(BlockRemoverErrorKind::Timeout),
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
}
