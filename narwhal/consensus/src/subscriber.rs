// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    store::{ConsensusStore, StoreResult},
    ConsensusOutput, ConsensusSyncRequest,
};
use bytes::Bytes;
use crypto::traits::VerifyingKey;
use futures::{stream::StreamExt, SinkExt};
use primary::{Certificate, CertificateDigest};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::{debug, error};

#[cfg(test)]
#[path = "tests/subscriber_tests.rs"]
pub mod subscriber_tests;

/// The messages sent by the subscriber server to the sequencer core to notify the manager
/// of a new subscriber.
type NewSubscriber<PublicKey> = Sender<ConsensusOutput<PublicKey>>;

/// Convenience alias indicating the persistent storage holding certificates.
type CertificateStore<PublicKey> = store::Store<CertificateDigest, Certificate<PublicKey>>;

/// Pushes the consensus output to subscriber clients and helps them to remain up to date.
pub struct SubscriberHandler;

impl SubscriberHandler {
    /// Spawn a new subscriber handler.
    pub fn spawn<PublicKey: VerifyingKey>(
        // The network address of the subscriber server.
        address: SocketAddr,
        // The persistent store holding the consensus state.
        consensus_store: Arc<ConsensusStore<PublicKey>>,
        // The persistent store holding the certificates.
        certificate_store: CertificateStore<PublicKey>,
        // A channel to receive the output of consensus.
        rx_sequence: Receiver<ConsensusOutput<PublicKey>>,
        // The maximum number of subscribers that this node can handle.
        _max_subscribers: usize,
    ) {
        let (tx_subscriber, rx_subscriber) = channel(crate::DEFAULT_CHANNEL_SIZE);
        SubscriberManager::spawn(rx_sequence, rx_subscriber);
        SubscriberServer::spawn(address, tx_subscriber, consensus_store, certificate_store);
    }
}

/// Receive sequenced certificates from consensus and ships them to any listening subscriber.
pub struct SubscriberManager<PublicKey: VerifyingKey> {
    /// Receive output sequence from consensus.
    rx_sequence: Receiver<ConsensusOutput<PublicKey>>,
    /// Communicate with subscribers to update with the output of the sequence.
    rx_subscriber: Receiver<NewSubscriber<PublicKey>>,
    /// Hold a channel to communicate with each subscriber.
    subscribers: Vec<Sender<ConsensusOutput<PublicKey>>>,
}

impl<PublicKey: VerifyingKey> SubscriberManager<PublicKey> {
    /// Create a new subscriber manager and spawn it in a separate tokio task.
    pub fn spawn(
        rx_sequence: Receiver<ConsensusOutput<PublicKey>>,
        rx_subscriber: Receiver<NewSubscriber<PublicKey>>,
    ) {
        tokio::spawn(async move {
            Self {
                rx_sequence,
                rx_subscriber,
                subscribers: Vec::new(),
            }
            .run()
            .await;
        });
    }

    /// Update all subscribers with the latest certificate.
    async fn update_subscribers(&mut self, message: ConsensusOutput<PublicKey>) {
        // TODO: Could this be better written through `join_all`?

        // Notify the subscribers of the new output. If a subscriber's channel is full (the subscriber
        // is slow), we simply skip this output. The subscriber will eventually sync to catch up.
        let mut to_drop = Vec::new();
        for (i, subscriber) in self.subscribers.iter().enumerate() {
            if subscriber.is_closed() {
                to_drop.push(i);
                continue;
            }
            if subscriber.capacity() > 0 && subscriber.send(message.clone()).await.is_err() {
                to_drop.push(i);
            }
        }

        // Cleanup the list subscribers that dropped the connection.
        for i in to_drop {
            self.subscribers.remove(i);
        }
    }

    /// Main loop registering new subscribers and listening to new sequenced certificates to update subscribers.
    async fn run(&mut self) {
        loop {
            tokio::select! {
                // Receive ordered certificates.
                Some(message) = self.rx_sequence.recv() => self.update_subscribers(message).await,

                // Receive subscribers to update with the consensus' output.
                Some(subscriber) = self.rx_subscriber.recv() => self.subscribers.push(subscriber),
            }
        }
    }
}

/// For each incoming request, we spawn a new runner responsible to receive messages and forward them
/// through the provided deliver channel.
pub struct SubscriberServer<PublicKey: VerifyingKey> {
    /// Address to listen to.
    address: SocketAddr,
    /// Channel to notify the sequencer's core of a new subscriber.
    tx_subscriber: Sender<NewSubscriber<PublicKey>>,
    /// The persistent storage holding the consensus state. It is only used to help subscribers to sync,
    /// this task never writes to the store.
    consensus_store: Arc<ConsensusStore<PublicKey>>,
    /// The persistent storage holding certificates. It is only used to help subscribers to sync, this
    /// task never writes to the store.
    certificate_store: CertificateStore<PublicKey>,
}

impl<PublicKey: VerifyingKey> SubscriberServer<PublicKey> {
    /// Spawn a new network receiver handling connections from any incoming peer.
    pub fn spawn(
        address: SocketAddr,
        tx_subscriber: Sender<NewSubscriber<PublicKey>>,
        consensus_store: Arc<ConsensusStore<PublicKey>>,
        certificate_store: CertificateStore<PublicKey>,
    ) {
        tokio::spawn(async move {
            Self {
                address,
                tx_subscriber,
                consensus_store,
                certificate_store,
            }
            .run()
            .await;
        });
    }

    /// Main loop responsible to accept incoming connections and spawn a new runner to handle it.
    async fn run(&self) {
        let listener = TcpListener::bind(&self.address)
            .await
            .expect("Failed to bind TCP port");

        debug!("Listening on {}", self.address);
        loop {
            let (socket, peer) = match listener.accept().await {
                Ok(value) => value,
                Err(e) => {
                    debug!("Failed to establish connection with subscriber {}", e);
                    continue;
                }
            };

            // TODO [issue #109]: Limit the number of subscribers here rather than in the core.
            debug!("Incoming connection established with {}", peer);
            let core_channel = self.tx_subscriber.clone();
            let consensus_store = self.consensus_store.clone();
            let certificate_store = self.certificate_store.clone();
            let socket = Framed::new(socket, LengthDelimitedCodec::new());
            SubscriberConnection::spawn(
                core_channel,
                consensus_store,
                certificate_store,
                socket,
                peer,
            );
        }
    }
}

/// A connection with a single subscriber.
struct SubscriberConnection<PublicKey: VerifyingKey> {
    /// Notify the sequencer's core of a new subscriber.
    tx_subscriber: Sender<NewSubscriber<PublicKey>>,
    /// The persistent storage holding the consensus state. It is only used to help subscribers to sync,
    /// this task never writes to the store.
    consensus_store: Arc<ConsensusStore<PublicKey>>,
    /// The persistent storage holding certificates. It is only used to help subscribers to sync, this
    /// task never writes to the store.
    certificate_store: CertificateStore<PublicKey>,
    /// The TCP socket connection to the subscriber.
    socket: Framed<TcpStream, LengthDelimitedCodec>,
    /// The identifier of the subscriber.
    peer: SocketAddr,
}

impl<PublicKey: VerifyingKey> SubscriberConnection<PublicKey> {
    /// The number of pending updates that the subscriber can hold in memory.
    const MAX_PENDING_UPDATES: usize = 1_000;

    /// Create a new subscriber server.
    pub fn spawn(
        tx_subscriber: Sender<NewSubscriber<PublicKey>>,
        consensus_store: Arc<ConsensusStore<PublicKey>>,
        certificate_store: CertificateStore<PublicKey>,
        socket: Framed<TcpStream, LengthDelimitedCodec>,
        peer: SocketAddr,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                tx_subscriber,
                consensus_store,
                certificate_store,
                socket,
                peer,
            }
            .run()
            .await
        })
    }

    /// Help the subscriber missing chunks of the output sequence to get up to speed.
    async fn synchronize(&mut self, request: ConsensusSyncRequest) -> StoreResult<()> {
        // Load the digests from the consensus store.
        let digests = self
            .consensus_store
            .read_sequenced_certificates(&request.missing)?
            .into_iter()
            .take_while(|x| x.is_some())
            .map(|x| x.unwrap());

        // Load the actual certificates from the certificate store.
        let certificates = self.certificate_store.read_all(digests).await?;

        // Transmit each certificate to the subscriber (in the right order).
        for (certificate, consensus_index) in certificates.into_iter().zip(request.missing) {
            match certificate {
                Some(certificate) => {
                    let message = ConsensusOutput {
                        certificate,
                        consensus_index,
                    };
                    let serialized =
                        bincode::serialize(&message).expect("Failed to serialize update");
                    if self.socket.send(Bytes::from(serialized)).await.is_err() {
                        debug!("Connection dropped by subscriber {}", self.peer);
                        break;
                    }
                }
                None => {
                    error!("Inconsistency between consensus and certificates store");
                    break;
                }
            }
        }
        Ok(())
    }

    /// Main loop interacting with the subscriber.
    async fn run(&mut self) {
        let (tx_output, mut rx_output) = channel(Self::MAX_PENDING_UPDATES);

        // Notify the core of a new subscriber.
        self.tx_subscriber
            .send(tx_output)
            .await
            .expect("Failed to send new subscriber to core");

        // Interact with the subscriber.
        // TODO [issue #120]: Better error handling (we have a lot of prints and breaks here).
        loop {
            tokio::select! {
                // Update the subscriber every time a certificate is sequenced.
                Some(message) = rx_output.recv() => {
                    let serialized = bincode::serialize(&message).expect("Failed to serialize update");
                    if self.socket.send(Bytes::from(serialized)).await.is_err() {
                        debug!("Connection dropped by subscriber {}", self.peer);
                        break;
                    }
                },

                // Receive sync requests form the subscriber.
                Some(buffer) = self.socket.next() => match buffer {
                    Ok(bytes) => match bincode::deserialize(&bytes) {
                        Ok(request) => if let Err(e) = self.synchronize(request).await {
                            error!("{}", e);
                        },
                        Err(e) => {
                            debug!("subscriber {} sent malformed sync request: {}", self.peer, e);
                            break;
                        }
                    },
                    Err(e) => {
                        debug!("Error while reading TCP stream: {}", e);
                        break;
                    }
                }
            }
        }
        debug!("Connection with subscriber {} closed", self.peer);
    }
}
