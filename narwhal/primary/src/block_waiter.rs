// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::block_synchronizer::handler::Handler;
use config::Committee;
use crypto::{traits::VerifyingKey, Digest, Hash};
use futures::{
    future::{try_join_all, BoxFuture},
    stream::{futures_unordered::FuturesUnordered, StreamExt as _},
    FutureExt,
};
use network::PrimaryToWorkerNetwork;
use std::{
    collections::{HashMap, HashSet},
    fmt,
    fmt::Formatter,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{mpsc::Receiver, oneshot, watch},
    task::JoinHandle,
    time::timeout,
};
use tracing::{debug, error, instrument, warn};
use types::{
    BatchDigest, BatchMessage, BlockError, BlockErrorKind, BlockResult, Certificate,
    CertificateDigest, Header, PrimaryWorkerMessage, ReconfigureNotification,
};
use Result::*;

const BATCH_RETRIEVE_TIMEOUT: Duration = Duration::from_secs(1);

#[cfg(test)]
#[path = "tests/block_waiter_tests.rs"]
pub mod block_waiter_tests;

#[derive(Debug)]
pub enum BlockCommand {
    /// GetBlock dictates retrieving the block data
    /// (vector of transactions) by a given block digest.
    /// Results are sent to the provided Sender. The id is
    /// basically the Certificate digest id.
    GetBlock {
        id: CertificateDigest,
        // The channel to send the results to.
        sender: oneshot::Sender<BlockResult<GetBlockResponse>>,
    },

    /// GetBlocks will initiate the process of retrieving the
    /// block data for multiple provided block ids. The results
    /// will be returned in the same order that the ids were
    /// provided.
    GetBlocks {
        ids: Vec<CertificateDigest>,
        sender: oneshot::Sender<BlocksResult>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetBlockResponse {
    pub id: CertificateDigest,
    #[allow(dead_code)]
    pub batches: Vec<BatchMessage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetBlocksResponse {
    pub blocks: Vec<BlockResult<GetBlockResponse>>,
}

pub type BatchResult = Result<BatchMessage, BatchMessageError>;

#[derive(Clone, Default, Debug)]
// If worker couldn't send us a batch, this error message
// should be passed to BlockWaiter.
pub struct BatchMessageError {
    pub id: BatchDigest,
}

type BlocksResult = Result<GetBlocksResponse, BlocksError>;

#[derive(Debug, Clone)]
pub struct BlocksError {
    ids: Vec<CertificateDigest>,
    #[allow(dead_code)]
    error: BlocksErrorType,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BlocksErrorType {
    Error,
}

impl fmt::Display for BlocksErrorType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

type RequestKey = Vec<u8>;

/// BlockWaiter is responsible for fetching the block data from the
/// downstream worker nodes. A block is basically the aggregate
/// of batches of transactions for a given certificate.
///
/// # Example
///
/// Basic setup of the BlockWaiter
///
/// This example shows the basic setup of the BlockWaiter module. It showcases
/// the necessary components that have to be used (e.x channels, datastore etc)
/// and how a request (command) should be issued to get a block and receive
/// the result of it.
///
/// ```rust
/// # use tokio::sync::{mpsc::{channel}, watch, oneshot};
/// # use arc_swap::ArcSwap;
/// # use crypto::Hash;
/// # use std::env::temp_dir;
/// # use crypto::ed25519::Ed25519PublicKey;
/// # use config::Committee;
/// # use std::collections::BTreeMap;
/// # use types::Certificate;
/// # use primary::{BlockWaiter, BlockHeader, BlockCommand, block_synchronizer::{BlockSynchronizeResult, handler::{Error, Handler}}};
/// # use types::{BatchMessage, BatchDigest, CertificateDigest, Batch};
/// # use mockall::*;
/// # use types::ReconfigureNotification;
/// # use crypto::traits::VerifyingKey;
/// # use async_trait::async_trait;
/// # use std::sync::Arc;
///
/// # // A mock implementation of the BlockSynchronizerHandler
/// struct BlockSynchronizerHandler;
///
/// #[async_trait]
/// impl<PublicKey: VerifyingKey> Handler<PublicKey> for BlockSynchronizerHandler {
///
///     async fn get_and_synchronize_block_headers(&self, block_ids: Vec<CertificateDigest>) -> Vec<Result<Certificate<PublicKey>, Error>> {
///         vec![]
///     }
///
///     async fn get_block_headers(&self, block_ids: Vec<CertificateDigest>) -> Vec<BlockSynchronizeResult<BlockHeader<PublicKey>>> {
///         vec![]
///     }
///
///     async fn synchronize_block_payloads(&self, certificates: Vec<Certificate<PublicKey>>) -> Vec<Result<Certificate<PublicKey>, Error>> {
///         vec![]
///     }
///
/// }
///
/// #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
///     let (tx_commands, rx_commands) = channel(1);
///     let (tx_batches, rx_batches) = channel(1);
///     let (tx_get_block, mut rx_get_block) = oneshot::channel();
///
///     let name = Ed25519PublicKey::default();
///     let committee = Committee{ epoch: 0, authorities: BTreeMap::new() };
///     let (_tx_reconfigure, rx_reconfigure) = watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
///
///     // A dummy certificate
///     let certificate = Certificate::<Ed25519PublicKey>::default();
///
///     // Dummy - we expect the BlockSynchronizer to actually respond, here
///     // we are using a mock
///     BlockWaiter::spawn(
///         name,
///         committee,
///         rx_reconfigure,
///         rx_commands,
///         rx_batches,
///         Arc::new(BlockSynchronizerHandler{}),
///     );
///
///     // Send a command to receive a block
///     tx_commands
///         .send(BlockCommand::GetBlock {
///             id: certificate.digest(),
///             sender: tx_get_block,
///         })
///         .await;
///
///     // Dummy - we expect to receive the requested batches via another component
///     // and get fed via the tx_batches channel.
///     tx_batches.send(Ok(BatchMessage{ id: BatchDigest::default(), transactions: Batch(vec![]) })).await;
///
///     // Wait to receive the block output to the provided sender channel
///     match rx_get_block.await.unwrap() {
///         Ok(result) => {
///             println!("Successfully received a block response");
///         }
///         Err(err) => {
///             println!("Received an error {}", err);
///         }
///     }
/// # }
/// ```
pub struct BlockWaiter<
    PublicKey: VerifyingKey,
    SynchronizerHandler: Handler<PublicKey> + Send + Sync + 'static,
> {
    /// The public key of this primary.
    name: PublicKey,

    /// The committee information.
    committee: Committee<PublicKey>,

    /// Receive all the requests to get a block
    rx_commands: Receiver<BlockCommand>,

    /// Whenever we have a get_block request, we mark the
    /// processing as pending by adding it on the hashmap. Once
    /// we have a result back - or timeout - we expect to remove
    /// the digest from the map. The key is the block id, and
    /// the value is the corresponding certificate.
    pending_get_block: HashMap<CertificateDigest, Certificate<PublicKey>>,

    /// Network driver allowing to send messages.
    worker_network: PrimaryToWorkerNetwork,

    /// Watch channel to reconfigure the committee.
    rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,

    /// The batch receive channel is listening for received
    /// messages for batches that have been requested
    rx_batch_receiver: Receiver<BatchResult>,

    /// Maps batch ids to channels that "listen" for arrived batch messages.
    /// On the key we hold the batch id (we assume it's globally unique).
    /// On the value we hold a tuple of the channel to communicate the result
    /// to and also a timestamp of when the request was sent.
    tx_pending_batch: HashMap<BatchDigest, (oneshot::Sender<BatchResult>, u128)>,

    /// A map that holds the channels we should notify with the
    /// GetBlock responses.
    tx_get_block_map:
        HashMap<CertificateDigest, Vec<oneshot::Sender<BlockResult<GetBlockResponse>>>>,

    /// A map that holds the channels we should notify with the
    /// GetBlocks responses.
    tx_get_blocks_map: HashMap<RequestKey, Vec<oneshot::Sender<BlocksResult>>>,

    /// We use the handler of the block synchronizer to interact with the
    /// block synchronizer in a synchronous way. Share a reference of this
    /// between components.
    block_synchronizer_handler: Arc<SynchronizerHandler>,
}

impl<PublicKey: VerifyingKey, SynchronizerHandler: Handler<PublicKey> + Send + Sync + 'static>
    BlockWaiter<PublicKey, SynchronizerHandler>
{
    // Create a new waiter and start listening on incoming
    // commands to fetch a block
    pub fn spawn(
        name: PublicKey,
        committee: Committee<PublicKey>,
        rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
        rx_commands: Receiver<BlockCommand>,
        batch_receiver: Receiver<BatchResult>,
        block_synchronizer_handler: Arc<SynchronizerHandler>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                rx_commands,
                pending_get_block: HashMap::new(),
                worker_network: PrimaryToWorkerNetwork::default(),
                rx_reconfigure,
                rx_batch_receiver: batch_receiver,
                tx_pending_batch: HashMap::new(),
                tx_get_block_map: HashMap::new(),
                tx_get_blocks_map: HashMap::new(),
                block_synchronizer_handler,
            }
            .run()
            .await;
        })
    }

    async fn run(&mut self) {
        let mut waiting_get_block = FuturesUnordered::new();
        let mut waiting_get_blocks = FuturesUnordered::new();

        loop {
            tokio::select! {
                Some(command) = self.rx_commands.recv() => {
                    match command {
                        BlockCommand::GetBlocks { ids, sender } => {
                            match self.handle_get_blocks_command(ids, sender).await {
                                Some((get_block_futures, get_blocks_future)) => {
                                    for fut in get_block_futures {
                                        waiting_get_block.push(fut);
                                    }
                                    waiting_get_blocks.push(get_blocks_future);
                                },
                                _ => debug!("no processing for command get blocks, will not wait for any result")
                            }
                        },
                        BlockCommand::GetBlock { id, sender } => {
                            match self.handle_get_block_command(id, sender).await {
                                Some(fut) => waiting_get_block.push(fut),
                                None => debug!("no processing for command, will not wait for any results")
                            }
                        }
                    }
                },
                // When we receive a BatchMessage (from a worker), this is
                // this is captured by the rx_batch_receiver channel and
                // handled appropriately.
                Some(batch_message) = self.rx_batch_receiver.recv() => {
                    self.handle_batch_message(batch_message).await;
                },
                // When we send a request to fetch a block's batches
                // we wait on the results to come back before we proceed.
                // By iterating the waiting vector it allow us to proceed
                // whenever waiting has been finished for a request.
                Some(result) = waiting_get_block.next() => {
                    self.handle_batch_waiting_result(result).await;
                },
                Some(result) = waiting_get_blocks.next() => {
                    self.handle_get_blocks_waiting_result(result).await;
                }

                // Check whether the committee changed. If the network address of our workers changed upon trying
                // to send them a request, we will timeout and the caller will have to retry.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewCommittee(new_committee) => {
                            self.committee = new_committee;
                            tracing::debug!("Committee updated to {}", self.committee);
                        }
                        ReconfigureNotification::Shutdown => return,
                    }
                }
            }
        }
    }

    async fn handle_get_blocks_waiting_result(&mut self, result: BlocksResult) {
        let ids = result.clone().map_or_else(
            |err| err.ids,
            |ok| {
                ok.blocks
                    .into_iter()
                    .map(|res| res.map_or_else(|e| e.id, |r| r.id))
                    .collect()
            },
        );

        let key = Self::construct_get_blocks_request_key(&ids);

        match self.tx_get_blocks_map.remove(&key) {
            Some(senders) => {
                for sender in senders {
                    if sender.send(result.clone()).is_err() {
                        error!("Couldn't forward results for blocks {:?} to sender", ids)
                    }
                }
            }
            None => {
                error!("We should expect to find channels to respond for {:?}", ids);
            }
        }
    }

    #[instrument(level="debug", skip_all, fields(num_block_ids = ids.len()))]
    async fn handle_get_blocks_command<'a>(
        &mut self,
        ids: Vec<CertificateDigest>,
        sender: oneshot::Sender<BlocksResult>,
    ) -> Option<(
        Vec<BoxFuture<'a, BlockResult<GetBlockResponse>>>,
        BoxFuture<'a, BlocksResult>,
    )> {
        // check whether we have a similar request pending
        // to make the check easy we sort the digests in asc order,
        // and then we merge all the bytes to form a key
        let key = Self::construct_get_blocks_request_key(&ids);

        if self.tx_get_blocks_map.contains_key(&key) {
            // request already pending, nothing to do, just add the sender to the list
            // of pending to be notified ones.
            self.tx_get_blocks_map
                .entry(key)
                .or_insert_with(Vec::new)
                .push(sender);

            debug!("GetBlocks has an already pending request for the provided ids");
            return None;
        }

        // fetch the certificates
        let certificates = self.get_certificates(ids.clone()).await;

        let (get_block_futures, get_blocks_future) = self.get_blocks(certificates).await;

        // mark the request as pending
        self.tx_get_blocks_map
            .entry(key)
            .or_insert_with(Vec::new)
            .push(sender);

        Some((get_block_futures, get_blocks_future))
    }

    /// Helper method to retrieve a single certificate.
    #[instrument(level = "debug", skip_all, fields(certificate_id = ?id))]
    async fn get_certificate(&mut self, id: CertificateDigest) -> Option<Certificate<PublicKey>> {
        if let Some((_, c)) = self.get_certificates(vec![id]).await.first() {
            return c.to_owned();
        }
        None
    }

    /// Will fetch the certificates via the block_synchronizer. If the
    /// certificate is missing then we expect the synchronizer to
    /// fetch it via the peers. Otherwise if available on the storage
    /// should return the result immediately. The method is blocking to
    /// retrieve all the results.
    #[instrument(level = "debug", skip_all, fields(num_certificate_ids = ids.len()))]
    async fn get_certificates(
        &mut self,
        ids: Vec<CertificateDigest>,
    ) -> Vec<(CertificateDigest, Option<Certificate<PublicKey>>)> {
        let mut results = Vec::new();

        let block_header_results = self
            .block_synchronizer_handler
            .get_and_synchronize_block_headers(ids)
            .await;

        for result in block_header_results {
            if let Ok(certificate) = result {
                results.push((certificate.digest(), Some(certificate)));
            } else {
                results.push((result.err().unwrap().block_id(), None));
            }
        }

        results
    }

    /// It triggers fetching the blocks for each provided certificate. The
    /// method receives the `certificates` vector which is a tuple of the
    /// certificate id and an Optional with the certificate. If the certificate
    /// doesn't exist then the Optional will be empty (None) which means that
    /// we haven't managed to retrieve/find the certificate an error result
    /// will immediately be sent to the consumer.
    async fn get_blocks<'a>(
        &mut self,
        certificates: Vec<(CertificateDigest, Option<Certificate<PublicKey>>)>,
    ) -> (
        Vec<BoxFuture<'a, BlockResult<GetBlockResponse>>>,
        BoxFuture<'a, BlocksResult>,
    ) {
        let mut get_block_receivers = Vec::new();
        let mut futures = Vec::new();
        let mut ids = Vec::new();

        // ensure payloads are synchronized for the found certificates
        let found_certificates: Vec<Certificate<PublicKey>> = certificates
            .clone()
            .into_iter()
            .filter(|(_, c)| c.is_some())
            .map(|(_, c)| c.unwrap())
            .collect();

        let sync_result = self
            .block_synchronizer_handler
            .synchronize_block_payloads(found_certificates)
            .await;
        let successful_payload_sync_set = sync_result
            .clone()
            .into_iter()
            .flatten()
            .map(|c| c.digest())
            .collect::<HashSet<CertificateDigest>>();

        for (id, c) in certificates {
            let (get_block_sender, get_block_receiver) = oneshot::channel();
            ids.push(id);

            // certificate has been found
            if let Some(certificate) = c {
                // Proceed on getting the block only if the payload has
                // been successfully synced.
                if successful_payload_sync_set.contains(&id) {
                    let fut = self.get_block(id, certificate, get_block_sender).await;

                    if let Some(f) = fut {
                        futures.push(f.boxed());
                    }
                } else {
                    // Send a batch error in this case
                    get_block_sender
                        .send(Err(BlockError {
                            id,
                            error: BlockErrorKind::BatchError,
                        }))
                        .expect("Couldn't send BatchError error for a GetBlocks request");
                }
            } else {
                // if certificate has not been found , we just want to send directly a non-found block response
                get_block_sender
                    .send(Err(BlockError {
                        id,
                        error: BlockErrorKind::BlockNotFound,
                    }))
                    .expect("Couldn't send BlockNotFound error for a GetBlocks request");
            }

            get_block_receivers.push(get_block_receiver);
        }

        // create a waiter to fetch them all and send the response
        let fut = Self::wait_for_all_blocks(ids.clone(), get_block_receivers);

        return (futures, fut.boxed());
    }

    // handles received commands and returns back a future if needs to
    // wait for further results. Otherwise, an empty option is returned
    // if no further waiting on processing is needed.
    #[instrument(level="debug", skip_all, fields(block_id = ?id))]
    async fn handle_get_block_command<'a>(
        &mut self,
        id: CertificateDigest,
        sender: oneshot::Sender<BlockResult<GetBlockResponse>>,
    ) -> Option<BoxFuture<'a, BlockResult<GetBlockResponse>>> {
        match self.get_certificate(id).await {
            Some(certificate) => {
                // Before sending a request to fetch the block's batches, ensure that
                // those are synchronized and available.
                if !self
                    .ensure_payload_is_synchronized(certificate.clone())
                    .await
                {
                    // If the payload is not available or didn't manage to successfully
                    // sync, then we want to reply with an error and return.
                    sender
                        .send(Err(BlockError {
                            id,
                            error: BlockErrorKind::BatchError,
                        }))
                        .expect("Couldn't send message back to sender");

                    return None;
                }

                self.get_block(id, certificate, sender).await
            }
            None => {
                sender
                    .send(Err(BlockError {
                        id,
                        error: BlockErrorKind::BlockNotFound,
                    }))
                    .expect("Couldn't send BlockNotFound error for a GetBlock request");

                None
            }
        }
    }

    async fn get_block<'a>(
        &mut self,
        id: CertificateDigest,
        certificate: Certificate<PublicKey>,
        sender: oneshot::Sender<BlockResult<GetBlockResponse>>,
    ) -> Option<BoxFuture<'a, BlockResult<GetBlockResponse>>> {
        // If similar request is already under processing, don't start a new one
        if self.pending_get_block.contains_key(&id.clone()) {
            self.tx_get_block_map
                .entry(id)
                .or_insert_with(Vec::new)
                .push(sender);

            debug!("Block with id {} already has a pending request", id.clone());
            return None;
        }

        debug!("No pending get block for {}", id);

        // Add on a vector the receivers
        let batch_receivers = self.send_batch_requests(certificate.header.clone()).await;

        let fut = Self::wait_for_all_batches(id, batch_receivers);

        // Ensure that we mark this block retrieval
        // as pending so no other can initiate the process
        self.pending_get_block.insert(id, certificate.clone());

        self.tx_get_block_map
            .entry(id)
            .or_insert_with(Vec::new)
            .push(sender);

        return Some(fut.boxed());
    }

    /// This method will ensure that the payload for a block is available to
    /// the worker before going ahead to retrieve. This is done via the
    /// block synchronizer handler which will trigger the process of syncing
    /// if the payload is missing.
    ///
    /// # Returns
    ///
    /// `true`: If the payload was already synchronized or the synchronization
    /// was successful
    /// `false`: When synchronization failed
    async fn ensure_payload_is_synchronized(&self, certificate: Certificate<PublicKey>) -> bool {
        let sync_result = self
            .block_synchronizer_handler
            .synchronize_block_payloads(vec![certificate.clone()])
            .await;

        sync_result
            .first()
            .expect("Expected at least one result back")
            .is_ok()
    }

    async fn wait_for_all_blocks(
        ids: Vec<CertificateDigest>,
        get_block_receivers: Vec<oneshot::Receiver<BlockResult<GetBlockResponse>>>,
    ) -> BlocksResult {
        let result = try_join_all(get_block_receivers).await;

        if let Ok(res) = result {
            Ok(GetBlocksResponse { blocks: res })
        } else {
            Err(BlocksError {
                ids,
                error: BlocksErrorType::Error,
            })
        }
    }

    async fn handle_batch_waiting_result(&mut self, result: BlockResult<GetBlockResponse>) {
        let block_id = result.clone().map_or_else(|e| e.id, |r| r.id);

        match self.tx_get_block_map.remove(&block_id) {
            Some(senders) => {
                for sender in senders {
                    if sender.send(result.clone()).is_err() {
                        error!(
                            "Couldn't forward results for block {} to sender",
                            block_id.clone()
                        )
                    }
                }
            }
            None => {
                error!(
                    "We should expect to find channels to respond for {}",
                    block_id.clone()
                );
            }
        }

        // unlock the pending request & batches.
        match self.pending_get_block.remove(&block_id) {
            Some(certificate) => {
                for (digest, _) in certificate.header.payload {
                    // unlock the pending request - mostly about the
                    // timed out requests.
                    self.tx_pending_batch.remove(&digest);
                }
            }
            None => {
                // TODO: handle panic here
                error!(
                    "Expected to find certificate with id {} for pending processing",
                    &block_id
                );
            }
        }
    }

    // Sends requests to fetch the batches from the corresponding workers.
    // It returns a vector of tuples of the batch digest and a Receiver
    // channel of the fetched batch.
    async fn send_batch_requests(
        &mut self,
        header: Header<PublicKey>,
    ) -> Vec<(BatchDigest, oneshot::Receiver<BatchResult>)> {
        // Get the "now" time
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Failed to measure time")
            .as_millis();

        // Add the receivers to a vector
        let mut batch_receivers = Vec::new();

        // otherwise we send requests to all workers to send us their batches
        for (digest, worker_id) in header.payload {
            debug!(
                "Sending batch {} request to worker id {}",
                digest.clone(),
                worker_id
            );

            let worker_address = self
                .committee
                .worker(&self.name, &worker_id)
                .expect("Worker id not found")
                .primary_to_worker;

            let message = PrimaryWorkerMessage::<PublicKey>::RequestBatch(digest);

            self.worker_network.send(worker_address, &message).await;

            // mark it as pending batch. Since we assume that batches are unique
            // per block, a clean up on a block request will also clean
            // up all the pending batch requests.
            let (tx, rx) = oneshot::channel();
            self.tx_pending_batch.insert(digest, (tx, now));

            // add the receiver to a vector to poll later
            batch_receivers.push((digest, rx));
        }

        batch_receivers
    }

    async fn handle_batch_message(&mut self, result: BatchResult) {
        let batch_id: BatchDigest = result.clone().map_or_else(|e| e.id, |r| r.id);

        match self.tx_pending_batch.remove(&batch_id) {
            Some((sender, _)) => {
                debug!("Sending BatchResult with id {}", &batch_id);
                sender
                    .send(result)
                    .expect("Couldn't send BatchResult for pending batch");
            }
            None => {
                warn!("Couldn't find pending batch with id {}", &batch_id);
            }
        }
    }

    /// A helper method to "wait" for all the batch responses to be received.
    /// It gets the fetched batches and creates a GetBlockResponse ready
    /// to be sent back to the request.
    async fn wait_for_all_batches(
        block_id: CertificateDigest,
        batches_receivers: Vec<(BatchDigest, oneshot::Receiver<BatchResult>)>,
    ) -> BlockResult<GetBlockResponse> {
        let waiting: Vec<_> = batches_receivers
            .into_iter()
            .map(|p| Self::wait_for_batch(block_id, p.1))
            .collect();

        let mut result = try_join_all(waiting).await?;

        // to make deterministic the response, let's make sure that we'll serve the
        // batch results in the same order. Sort by batch_id ascending order.
        result.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(GetBlockResponse {
            id: block_id,
            batches: result,
        })
    }

    /// Waits for a batch to be received. If batch is not received in time,
    /// then a timeout is yielded and an error is returned.
    async fn wait_for_batch(
        block_id: CertificateDigest,
        batch_receiver: oneshot::Receiver<BatchResult>,
    ) -> BlockResult<BatchMessage> {
        // ensure that we won't wait forever for a batch result to come
        let r = match timeout(BATCH_RETRIEVE_TIMEOUT, batch_receiver).await {
            Ok(Ok(result)) => result.or(Err(BlockErrorKind::BatchError)),
            Ok(Err(_)) => Err(BlockErrorKind::BatchError),
            Err(_) => Err(BlockErrorKind::BatchTimeout),
        };

        r.map_err(|e| BlockError {
            id: block_id,
            error: e,
        })
    }

    fn construct_get_blocks_request_key(ids: &[CertificateDigest]) -> RequestKey {
        let mut ids_cloned = ids.to_vec();
        ids_cloned.sort();

        let result: RequestKey = ids_cloned
            .into_iter()
            .flat_map(|d| Digest::from(d).to_vec())
            .collect();

        result
    }
}
