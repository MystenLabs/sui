// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    errors::{SubscriberError, SubscriberResult},
    DEFAULT_CHANNEL_SIZE,
};
use blake2::digest::Update;
use bytes::Bytes;
use config::WorkerId;
use consensus::ConsensusOutput;
use crypto::traits::VerifyingKey;
use futures::{stream::StreamExt, SinkExt};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    time::Duration,
};
use store::Store;
use tokio::{
    net::TcpStream,
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
    time::sleep,
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::warn;
use types::BatchDigest;
use worker::{SerializedBatchMessage, WorkerMessage};

/// Download transactions data from the consensus workers and notifies the called when the job is done.
pub struct BatchLoader<PublicKey: VerifyingKey> {
    /// The temporary storage holding all transactions' data (that may be too big to hold in memory).
    store: Store<BatchDigest, SerializedBatchMessage>,
    /// Receive consensus outputs for which to download the associated transaction data.
    rx_input: Receiver<ConsensusOutput<PublicKey>>,
    /// The network addresses of the consensus workers.
    addresses: HashMap<WorkerId, SocketAddr>,
    /// A map of connections with the consensus workers.
    connections: HashMap<WorkerId, Sender<Vec<BatchDigest>>>,
}

impl<PublicKey: VerifyingKey> BatchLoader<PublicKey> {
    /// Spawn a new batch loaded in a dedicated tokio task.
    pub fn spawn(
        store: Store<BatchDigest, SerializedBatchMessage>,
        rx_input: Receiver<ConsensusOutput<PublicKey>>,
        addresses: HashMap<WorkerId, SocketAddr>,
    ) -> JoinHandle<SubscriberResult<()>> {
        tokio::spawn(async move {
            Self {
                store,
                rx_input,
                addresses,
                connections: HashMap::new(),
            }
            .run()
            .await
        })
    }

    /// Receive consensus messages for which we need to download the associated transaction data.
    async fn run(&mut self) -> SubscriberResult<()> {
        while let Some(message) = self.rx_input.recv().await {
            let certificate = &message.certificate;

            // Send a request for every batch referenced by the certificate.
            // TODO: Can we write it better without allocating a HashMap every time?
            let mut map = HashMap::with_capacity(certificate.header.payload.len());
            for (digest, worker_id) in certificate.header.payload.iter() {
                map.entry(*worker_id).or_insert_with(Vec::new).push(*digest);
            }
            for (worker_id, digests) in map {
                let address = self
                    .addresses
                    .get(&worker_id)
                    .ok_or(SubscriberError::UnexpectedWorkerId(worker_id))?;

                let sender = self.connections.entry(worker_id).or_insert_with(|| {
                    let (sender, receiver) = channel(DEFAULT_CHANNEL_SIZE);
                    SyncConnection::spawn::<PublicKey>(*address, self.store.clone(), receiver);
                    sender
                });

                sender
                    .send(digests)
                    .await
                    .expect("Sync connections are kept alive and never die");
            }
        }
        Ok(())
    }
}

/// Connect (and maintain a connection) with a specific worker. Then download batches from that
/// specific worker.
struct SyncConnection {
    /// The address of the worker to connect with.
    address: SocketAddr,
    /// The temporary storage holding all transactions' data (that may be too big to hold in memory).
    store: Store<BatchDigest, SerializedBatchMessage>,
    /// Receive the batches to download from the worker.
    rx_request: Receiver<Vec<BatchDigest>>,
    /// Keep a set of requests already made to avoid asking twice for the same batch.
    already_requested: HashSet<BatchDigest>,
}

impl SyncConnection {
    /// Spawn a new worker connection in a dedicated tokio task.
    pub fn spawn<PublicKey: VerifyingKey>(
        address: SocketAddr,
        store: Store<BatchDigest, SerializedBatchMessage>,
        rx_request: Receiver<Vec<BatchDigest>>,
    ) {
        tokio::spawn(async move {
            Self {
                address,
                store,
                rx_request,
                already_requested: HashSet::new(),
            }
            .run::<PublicKey>()
            .await;
        });
    }

    /// Main loop keeping the connection with a worker alive and receive batches to download.
    async fn run<PublicKey: VerifyingKey>(&mut self) {
        // The connection waiter ensures we do not attempt to reconnect immediately after failure.
        let mut connection_waiter = ConnectionWaiter::default();

        // Continuously connects to the worker.
        'main: loop {
            // Wait a bit before re-attempting connections.
            connection_waiter.wait().await;

            // Connect to the worker.
            let mut connection = match TcpStream::connect(self.address).await {
                Ok(x) => Framed::new(x, LengthDelimitedCodec::new()),
                Err(e) => {
                    warn!(
                        "Failed to connect to worker (retry {}): {e}",
                        connection_waiter.status(),
                    );
                    continue 'main;
                }
            };

            // Listen to sync request and update the store with the replies.
            loop {
                tokio::select! {
                    // Listen for batches to download.
                    Some(digests) = self.rx_request.recv() => {
                        // Filter digests that we already requested.
                        let mut missing = Vec::new();
                        for digest in digests {
                            if !self.already_requested.contains(&digest) {
                                missing.push(digest);
                            }
                        }

                        // Request the batch from the worker.
                        let message = WorkerMessage::<PublicKey>::ClientBatchRequest(missing.clone());
                        let serialized = bincode::serialize(&message).expect("Failed to serialize request");
                        match connection.send(Bytes::from(serialized)).await {
                            Ok(()) => {
                                for digest in missing {
                                    self.already_requested.insert(digest);
                                }
                            },
                            Err(e) => {
                                warn!("Failed to send sync request to worker {}: {e}", self.address);
                                continue 'main;
                            }
                        }
                    },

                    // Receive the batch data from the worker.
                    Some(result) = connection.next() => {
                        match result {
                            Ok(batch) => {
                                // Store the batch in the temporary store.
                                // TODO: We can probably avoid re-computing the hash of the bach since we trust the worker.
                                let digest = BatchDigest::new(crypto::blake2b_256(|hasher| hasher.update(&batch)));
                                self.store.write(digest, batch.to_vec()).await;

                                // Cleanup internal state.
                                self.already_requested.remove(&digest);

                                // Reset the connection timeout delay.
                                connection_waiter.reset();
                            },
                            Err(e) => {
                                warn!("Failed to receive batch reply from worker {}: {e}", self.address);
                                continue 'main;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Make the network client wait a bit before re-attempting network connections.
pub struct ConnectionWaiter {
    /// The minimum delay to wait before re-attempting a connection.
    min_delay: u64,
    /// The maximum delay to wait before re-attempting a connection.
    max_delay: u64,
    /// The actual delay we wait before re-attempting a connection.
    delay: u64,
    /// The number of times we attempted to make a connection.
    retry: usize,
}

impl Default for ConnectionWaiter {
    fn default() -> Self {
        Self::new(/* min_delay */ 200, /* max_delay */ 60_000)
    }
}

impl ConnectionWaiter {
    /// Create a new connection waiter.
    pub fn new(min_delay: u64, max_delay: u64) -> Self {
        Self {
            min_delay,
            max_delay,
            delay: 0,
            retry: 0,
        }
    }

    /// Return the number of failed attempts.
    pub fn status(&self) -> &usize {
        &self.retry
    }

    /// Wait for a bit (depending on the number of failed connections).
    pub async fn wait(&mut self) {
        if self.delay != 0 {
            sleep(Duration::from_millis(self.delay)).await;
        }

        self.delay = match self.delay {
            0 => self.min_delay,
            _ => std::cmp::min(2 * self.delay, self.max_delay),
        };
        self.retry += 1;
    }

    /// Reset the waiter to its initial parameters.
    pub fn reset(&mut self) {
        self.delay = 0;
        self.retry = 0;
    }
}
