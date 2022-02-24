// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{messages::Header, Certificate, PrimaryWorkerMessage};
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
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
    time::timeout,
};
use tracing::{error, log::debug};
use Result::*;

const BATCH_RETRIEVE_TIMEOUT: Duration = Duration::from_secs(1);

pub type Transaction = Vec<u8>;

#[cfg(test)]
#[path = "tests/block_waiter_tests.rs"]
pub mod block_waiter_tests;

pub enum BlockCommand {
    /// GetBlock dictates retrieving the block data
    /// (vector of transactions) by a given block digest.
    /// Results are sent to the provided Sender. The id is
    /// basically the Certificate digest id.
    #[allow(dead_code)]
    GetBlock {
        id: Digest,
        // The channel to send the results to.
        sender: Sender<BlockResult<GetBlockResponse>>,
    },
}

#[derive(Clone, Debug)]
pub struct GetBlockResponse {
    id: Digest,
    #[allow(dead_code)]
    batches: Vec<BatchMessage>,
}

#[derive(Clone, Default, Debug)]
pub struct BatchMessage {
    pub id: Digest,
    pub transactions: Vec<Transaction>,
}

type BlockResult<T> = Result<T, BlockError>;

#[derive(Debug, Clone)]
pub struct BlockError {
    id: Digest,
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
/// # use crypto::Hash;
/// # use std::env::temp_dir;
/// # use crypto::Digest;
/// # use crypto::ed25519::Ed25519PublicKey;
/// # use config::Committee;
/// # use std::collections::BTreeMap;
/// # use primary::Certificate;
/// # use primary::{BatchMessage, BlockWaiter, BlockCommand};
///
/// #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
///     const CERTIFICATES_CF: &str = "certificates";
///
///     // Basic setup: datastore, channels & BlockWaiter
///     let rocksdb = rocks::open_cf(temp_dir(), None, &[CERTIFICATES_CF])
///         .expect("Failed creating database");
///
///     let (certificate_map) = reopen!(&rocksdb,
///             CERTIFICATES_CF;<Digest, Certificate<Ed25519PublicKey>>);
///     let certificate_store = Store::new(certificate_map);
///
///     let (tx_commands, rx_commands) = channel(1);
///     let (tx_batches, rx_batches) = channel(1);
///     let (tx_get_block, mut rx_get_block) = channel(1);
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
///     tx_batches.send(BatchMessage{ id: Digest::default(), transactions: vec![] }).await;
///
///     // Wait to receive the block output to the provided sender channel
///     match rx_get_block.recv().await {
///         Some(Ok(result)) => {
///             println!("Successfully received a block response");
///         }
///         Some(Err(err)) => {
///             println!("Received an error {}", err);
///         }
///         _ => {
///             println!("Nothing received");
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
    certificate_store: Store<Digest, Certificate<PublicKey>>,

    /// Receive all the requests to get a block
    rx_commands: Receiver<BlockCommand>,

    /// Whenever we have a get_block request, we mark the
    /// processing as pending by adding it on the hashmap. Once
    /// we have a result back - or timeout - we expect to remove
    /// the digest from the map. The key is the block id, and
    /// the value is the corresponding certificate.
    pending_get_block: HashMap<Digest, Certificate<PublicKey>>,

    /// Network driver allowing to send messages.
    network: SimpleSender,

    /// The batch receive channel is listening for received
    /// messages for batches that have been requested
    rx_batch_receiver: Receiver<BatchMessage>,

    /// Maps batch ids to channels that "listen" for arrived batch messages.
    /// On the key we hold the batch id (we assume it's globally unique).
    /// On the value we hold a tuple of the channel to communicate the result
    /// to and also a timestamp of when the request was sent.
    tx_pending_batch: HashMap<Digest, (oneshot::Sender<BatchMessage>, u128)>,

    /// A map that holds the channels we should notify with the
    /// GetBlock responses.
    tx_get_block_map: HashMap<Digest, Vec<Sender<BlockResult<GetBlockResponse>>>>,
}

impl<PublicKey: VerifyingKey> BlockWaiter<PublicKey> {
    // Create a new waiter and start listening on incoming
    // commands to fetch a block
    pub fn spawn(
        name: PublicKey,
        committee: Committee<PublicKey>,
        certificate_store: Store<Digest, Certificate<PublicKey>>,
        rx_commands: Receiver<BlockCommand>,
        batch_receiver: Receiver<BatchMessage>,
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
            }
            .run()
            .await;
        });
    }

    async fn run(&mut self) {
        let mut waiting = FuturesUnordered::new();

        loop {
            tokio::select! {
                Some(command) = self.rx_commands.recv() => {
                    match self.handle_command(command).await {
                        Some(fut) => waiting.push(fut),
                        None => debug!("no processing for command, will not wait for any results")
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
                Some(result) = waiting.next() => {
                    self.handle_batch_waiting_result(result).await;
                },
            }
        }
    }

    // handles received commands and returns back a future if needs to
    // wait for further results. Otherwise, an empty option is returned
    // if no further waiting on processing is needed.
    async fn handle_command<'a>(
        &mut self,
        command: BlockCommand,
    ) -> Option<BoxFuture<'a, BlockResult<GetBlockResponse>>> {
        match command {
            BlockCommand::GetBlock { id, sender } => {
                match self.certificate_store.read(id.clone()).await {
                    Ok(Some(certificate)) => {
                        // If similar request is already under processing, don't start a new one
                        if self.pending_get_block.contains_key(&id.clone()) {
                            self.tx_get_block_map
                                .entry(id.clone())
                                .or_insert_with(Vec::new)
                                .push(sender);

                            debug!("Block with id {} already has a pending request", id.clone());
                            return None;
                        }

                        debug!("No pending get block for {}", id.clone());

                        // Add on a vector the receivers
                        let batch_receivers =
                            self.send_batch_requests(certificate.header.clone()).await;

                        let fut = Self::wait_for_all_batches(id.clone(), batch_receivers);

                        // Ensure that we mark this block retrieval
                        // as pending so no other can initiate the process
                        self.pending_get_block
                            .insert(id.clone(), certificate.clone());

                        self.tx_get_block_map
                            .entry(id.clone())
                            .or_insert_with(Vec::new)
                            .push(sender);

                        return Some(fut.boxed());
                    }
                    _ => {
                        sender
                            .send(Err(BlockError {
                                id: id.clone(),
                                error: BlockErrorType::BlockNotFound,
                            }))
                            .await
                            .expect("Couldn't send BlockNotFound error for a GetBlock request");
                    }
                }
            }
        }

        None
    }

    async fn handle_batch_waiting_result(&mut self, result: BlockResult<GetBlockResponse>) {
        let block_id = result.clone().map_or_else(|e| e.id, |r| r.id);

        match self.tx_get_block_map.remove(&block_id) {
            Some(senders) => {
                for sender in senders {
                    if sender.send(result.clone()).await.is_err() {
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
    ) -> Vec<(Digest, oneshot::Receiver<BatchMessage>)> {
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

            let message = PrimaryWorkerMessage::<PublicKey>::RequestBatch(digest.clone());
            let bytes = bincode::serialize(&message).expect("Failed to serialize batch request");

            self.network.send(worker_address, Bytes::from(bytes)).await;

            // mark it as pending batch. Since we assume that batches are unique
            // per block, a clean up on a block request will also clean
            // up all the pending batch requests.
            let (tx, rx) = oneshot::channel();
            self.tx_pending_batch.insert(digest.clone(), (tx, now));

            // add the receiver to a vector to poll later
            batch_receivers.push((digest.clone(), rx));
        }

        batch_receivers
    }

    async fn handle_batch_message(&mut self, message: BatchMessage) {
        match self.tx_pending_batch.remove(&message.id) {
            Some((sender, _)) => {
                debug!("Sending batch message with id {}", &message.id);
                sender
                    .send(message.clone())
                    .expect("Couldn't send BatchMessage for pending batch");
            }
            None => {
                debug!("Couldn't find pending batch with id {}", message.id);
            }
        }
    }

    /// A helper method to "wait" for all the batch responses to be received.
    /// It gets the fetched batches and creates a GetBlockResponse ready
    /// to be sent back to the request.
    async fn wait_for_all_batches(
        block_id: Digest,
        batches_receivers: Vec<(Digest, oneshot::Receiver<BatchMessage>)>,
    ) -> BlockResult<GetBlockResponse> {
        let waiting: Vec<_> = batches_receivers
            .into_iter()
            .map(|p| Self::wait_for_batch(block_id.clone(), p.1))
            .collect();

        let result = try_join_all(waiting).await?;
        Ok(GetBlockResponse {
            id: block_id,
            batches: result,
        })
    }

    /// Waits for a batch to be received. If batch is not received in time,
    /// then a timeout is yielded and an error is returned.
    async fn wait_for_batch(
        block_id: Digest,
        batch_receiver: oneshot::Receiver<BatchMessage>,
    ) -> BlockResult<BatchMessage> {
        // ensure that we won't wait forever for a batch result to come
        return match timeout(BATCH_RETRIEVE_TIMEOUT, batch_receiver).await {
            Ok(Ok(result)) => BlockResult::Ok(result),
            Ok(Err(_)) => BlockError {
                id: block_id,
                error: BlockErrorType::BatchError,
            }
            .into(),
            Err(_) => BlockError {
                id: block_id,
                error: BlockErrorType::BatchTimeout,
            }
            .into(),
        };
    }
}
