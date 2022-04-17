// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    messages::{BatchDigest, CertificateDigest, Header},
    Batch, Certificate, PrimaryWorkerMessage,
};
use bytes::Bytes;
use config::Committee;
use crypto::{traits::VerifyingKey, Digest};
use futures::{
    future::{try_join_all, BoxFuture},
    stream::{futures_unordered::FuturesUnordered, StreamExt as _},
    FutureExt,
};
use network::SimpleSender;
use std::{
    collections::HashMap,
    fmt,
    fmt::Formatter,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use store::Store;
use tokio::{
    sync::{mpsc::Receiver, oneshot, oneshot::error::RecvError},
    time::timeout,
};
use tracing::{error, log::debug};
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

#[derive(Clone, Debug, PartialEq)]
pub struct GetBlockResponse {
    id: CertificateDigest,
    #[allow(dead_code)]
    pub batches: Vec<BatchMessage>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GetBlocksResponse {
    pub blocks: Vec<BlockResult<GetBlockResponse>>,
}

pub type BatchResult = Result<BatchMessage, BatchMessageError>;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct BatchMessage {
    pub id: BatchDigest,
    pub transactions: Batch,
}

#[derive(Clone, Default, Debug)]
// If worker couldn't send us a batch, this error message
// should be passed to BlockWaiter.
pub struct BatchMessageError {
    pub id: BatchDigest,
}

pub type BlockResult<T> = Result<T, BlockError>;

#[derive(Debug, Clone, PartialEq)]
pub struct BlockError {
    id: CertificateDigest,
    error: BlockErrorType,
}

impl<T> From<BlockError> for BlockResult<T> {
    fn from(error: BlockError) -> Self {
        BlockResult::Err(error)
    }
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "block id: {}, error type: {}", self.id, self.error)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockErrorType {
    BlockNotFound,
    BatchTimeout,
    BatchError,
}

impl fmt::Display for BlockErrorType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

type BlocksResult = Result<GetBlocksResponse, BlocksError>;

#[derive(Debug, Clone)]
pub struct BlocksError {
    ids: Vec<CertificateDigest>,
    #[allow(dead_code)]
    error: BlocksErrorType,
}

#[derive(Debug, Clone, PartialEq)]
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
/// # use store::{reopen, rocks, rocks::DBMap, Store};
/// # use tokio::sync::mpsc::{channel};
/// # use tokio::sync::oneshot;
/// # use crypto::Hash;
/// # use std::env::temp_dir;
/// # use crypto::ed25519::Ed25519PublicKey;
/// # use config::Committee;
/// # use std::collections::BTreeMap;
/// # use primary::Certificate;
/// # use tempfile::tempdir;
/// # use primary::{BatchMessage, BlockWaiter, BlockCommand,BatchDigest, CertificateDigest, Batch};
///
/// #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
///     const CERTIFICATES_CF: &str = "certificates";
///
///     let temp_dir = tempdir().expect("Failed to open temporary directory").into_path();
///
///     // Basic setup: datastore, channels & BlockWaiter
///     let rocksdb = rocks::open_cf(temp_dir, None, &[CERTIFICATES_CF])
///         .expect("Failed creating database");
///
///     let (certificate_map) = reopen!(&rocksdb,
///             CERTIFICATES_CF;<CertificateDigest, Certificate<Ed25519PublicKey>>);
///     let certificate_store = Store::new(certificate_map);
///
///     let (tx_commands, rx_commands) = channel(1);
///     let (tx_batches, rx_batches) = channel(1);
///     let (tx_get_block, mut rx_get_block) = oneshot::channel();
///
///     let name = Ed25519PublicKey::default();
///     let committee = Committee{ authorities: BTreeMap::new() };
///
///     BlockWaiter::spawn(
///         name,
///         committee,
///         certificate_store.clone(),
///         rx_commands,
///         rx_batches,
///     );
///
///     // A dummy certificate
///     let certificate = Certificate::<Ed25519PublicKey>::default();
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
pub struct BlockWaiter<PublicKey: VerifyingKey> {
    /// The public key of this primary.
    name: PublicKey,

    /// The committee information.
    committee: Committee<PublicKey>,

    /// Storage that keeps the Certificates by their digest id.
    certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,

    /// Receive all the requests to get a block
    rx_commands: Receiver<BlockCommand>,

    /// Whenever we have a get_block request, we mark the
    /// processing as pending by adding it on the hashmap. Once
    /// we have a result back - or timeout - we expect to remove
    /// the digest from the map. The key is the block id, and
    /// the value is the corresponding certificate.
    pending_get_block: HashMap<CertificateDigest, Certificate<PublicKey>>,

    /// Network driver allowing to send messages.
    network: SimpleSender,

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
}

impl<PublicKey: VerifyingKey> BlockWaiter<PublicKey> {
    // Create a new waiter and start listening on incoming
    // commands to fetch a block
    pub fn spawn(
        name: PublicKey,
        committee: Committee<PublicKey>,
        certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,
        rx_commands: Receiver<BlockCommand>,
        batch_receiver: Receiver<BatchResult>,
    ) {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                certificate_store,
                rx_commands,
                pending_get_block: HashMap::new(),
                network: SimpleSender::new(),
                rx_batch_receiver: batch_receiver,
                tx_pending_batch: HashMap::new(),
                tx_get_block_map: HashMap::new(),
                tx_get_blocks_map: HashMap::new(),
            }
            .run()
            .await;
        });
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

        match self.certificate_store.read_all(ids.clone()).await {
            Ok(certificates) => {
                let (get_block_futures, get_blocks_future) =
                    self.get_blocks(ids, certificates).await;

                // mark the request as pending
                self.tx_get_blocks_map
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .push(sender);

                return Some((get_block_futures, get_blocks_future));
            }
            Err(err) => {
                error!("{err}");
            }
        }

        None
    }

    async fn get_blocks<'a>(
        &mut self,
        ids: Vec<CertificateDigest>,
        certificates: Vec<Option<Certificate<PublicKey>>>,
    ) -> (
        Vec<BoxFuture<'a, BlockResult<GetBlockResponse>>>,
        BoxFuture<'a, BlocksResult>,
    ) {
        let mut get_block_receivers = Vec::new();
        let mut futures = Vec::new();

        for (i, c) in certificates.into_iter().enumerate() {
            let (get_block_sender, get_block_receiver) = oneshot::channel();
            let id = *ids.get(i).unwrap();

            // certificate has been found
            if c.is_some() {
                let certificate = c.unwrap();
                let fut = self.get_block(id, certificate, get_block_sender).await;

                if fut.is_some() {
                    futures.push(fut.unwrap().boxed());
                }
            } else {
                // if certificate has not been found , we just want to send directly a non-found block response
                get_block_sender
                    .send(Err(BlockError {
                        id,
                        error: BlockErrorType::BlockNotFound,
                    }))
                    .expect("Couldn't send BlockNotFound error for a GetBlock request");
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
    async fn handle_get_block_command<'a>(
        &mut self,
        id: CertificateDigest,
        sender: oneshot::Sender<BlockResult<GetBlockResponse>>,
    ) -> Option<BoxFuture<'a, BlockResult<GetBlockResponse>>> {
        return match self.certificate_store.read(id).await {
            Ok(Some(certificate)) => self.get_block(id, certificate, sender).await,
            _ => {
                sender
                    .send(Err(BlockError {
                        id,
                        error: BlockErrorType::BlockNotFound,
                    }))
                    .expect("Couldn't send BlockNotFound error for a GetBlock request");

                None
            }
        };
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

        debug!("No pending get block for {}", id.clone());

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

    async fn wait_for_all_blocks(
        ids: Vec<CertificateDigest>,
        get_block_receivers: Vec<oneshot::Receiver<BlockResult<GetBlockResponse>>>,
    ) -> BlocksResult {
        let receivers: Vec<_> = get_block_receivers
            .into_iter()
            .map(|r| Self::wait_to_receive(r))
            .collect();

        let result = try_join_all(receivers).await;

        if result.is_err() {
            Err(BlocksError {
                ids,
                error: BlocksErrorType::Error,
            })
        } else {
            Ok(GetBlocksResponse {
                blocks: result.unwrap(),
            })
        }
    }

    async fn wait_to_receive(
        receiver: oneshot::Receiver<BlockResult<GetBlockResponse>>,
    ) -> Result<BlockResult<GetBlockResponse>, RecvError> {
        receiver.await
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
            let bytes = bincode::serialize(&message).expect("Failed to serialize batch request");

            self.network.send(worker_address, Bytes::from(bytes)).await;

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
                println!("Couldn't find pending batch with id {}", &batch_id);
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
            Ok(Ok(result)) => result.or(Err(BlockErrorType::BatchError)),
            Ok(Err(err)) => {
                println!("Receiver error: {err}");
                Err(BlockErrorType::BatchError)
            }
            Err(_) => Err(BlockErrorType::BatchTimeout),
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
