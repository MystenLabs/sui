// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::futures_unordered::FuturesUnordered;
use futures::stream::StreamExt;
use futures::SinkExt;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::Hasher;
use std::net::SocketAddr;
use std::time::Duration;
use sui_network::network::NetworkClient;
use sui_network::transport::{MessageHandler, RwChannel, SpawnedServer};
use sui_types::serialize::{serialize_message, SerializedMessage};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;
use tokio::time::sleep;

/// Convenience type used to send acknowledgements from the sequencer to the network task.
type Replier = oneshot::Sender<SerializedMessage>;

/// A mock single-process sequencer. It is not crash-safe (it has no storage) and should
/// only be used for testing.
pub struct MockSequencer {
    /// The delay to wait before sequencing a message. This parameter is only required to
    /// emulates the consensus' latency.
    consensus_delay: Duration,
    /// The network client to send the output of consensus to each authorities. This stores
    /// one network client per authority.
    network_clients: Vec<(SocketAddr, NetworkClient)>,
    /// Internal channel to sequence users' transactions.
    rx_input: Receiver<(Bytes, Replier)>,
}

impl MockSequencer {
    /// Create a new mock sequencer.
    pub async fn spawn(
        consensus_delay: Duration,
        sequencer_address: SocketAddr,
        authorities_addresses: Vec<SocketAddr>,
        buffer_size: usize,
        send_timeout: Duration,
        recv_timeout: Duration,
    ) -> Result<SpawnedServer, std::io::Error> {
        // Spawn a network receiver in a separate tokio task.
        let (tx_input, rx_input) = channel(100);
        let message_handler = SequencerMessageHandler { tx_input };
        let spawned_server = sui_network::transport::spawn_server(
            &sequencer_address.to_string(),
            message_handler,
            buffer_size,
        )
        .await;

        // Create one network client per authority.
        let network_clients = authorities_addresses
            .into_iter()
            .map(|address| {
                (
                    address,
                    NetworkClient::new(
                        address.ip().to_string(),
                        address.port(),
                        buffer_size,
                        send_timeout,
                        recv_timeout,
                    ),
                )
            })
            .collect();

        // Spawn the core sequencer in a new tokio task.
        tokio::spawn(async move {
            Self {
                consensus_delay,
                network_clients,
                rx_input,
            }
            .run()
            .await;
        });

        spawned_server
    }

    /// Simply wait for a fixed delay and then returns the input.
    async fn waiter(deliver: Bytes, delay: Duration) -> Bytes {
        sleep(delay).await;
        deliver
    }

    /// Hash a message (not a cryptographic hash).
    fn hash_message(message: &Bytes) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(message);
        hasher.finish()
    }

    /// Main loop ordering input bytes.
    async fn run(&mut self) {
        let mut waiting = FuturesUnordered::new();
        let mut repliers = HashMap::new();
        loop {
            tokio::select! {
                // Receive bytes to order.
                Some((bytes, replier)) = self.rx_input.recv() => {
                    repliers.insert(Self::hash_message(&bytes), replier);
                    waiting.push(Self::waiter(bytes, self.consensus_delay));
                },

                // Bytes are ready to be delivered.
                Some(bytes) = waiting.next() => {
                    let digest = Self::hash_message(&bytes);
                    for (address, client) in &self.network_clients {
                        // A real consensus implementation would make sure to receive an
                        // ack from the authorities and retry sending until the message is
                        // delivered. This is safety-critical.
                        let reply = match client.send_recv_bytes(bytes.to_vec()).await {
                            Ok(reply) => reply,
                            Err(e) => {
                                log::warn!("Failed to send output sequence to {}: {}", address, e);
                                SerializedMessage::Error(Box::new(e))
                            }
                        };
                        if let Some(replier) = repliers.remove(&digest) {
                            if replier.send(reply).is_err() {
                                panic!("Failed to reply to network server");
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Define how the network server should handle incoming messages. This is not got to
/// stream many input transactions (benchmarks) as the task handling the TCP connection
/// blocks until a reply is ready.
struct SequencerMessageHandler {
    /// Send user transactions to the sequencer.
    pub tx_input: Sender<(Bytes, Replier)>,
}

#[async_trait]
impl<'a, A> MessageHandler<A> for SequencerMessageHandler
where
    A: 'static + RwChannel<'a> + Unpin + Send,
{
    async fn handle_messages(&self, mut channel: A) {
        loop {
            // Read the user's transaction.
            let buffer = match channel.stream().next().await {
                Some(Ok(buffer)) => buffer,
                Some(Err(e)) => {
                    // We expect some EOF or disconnect error at the end.
                    log::error!("Error while reading TCP stream: {}", e);
                    break;
                }
                None => break,
            };

            // Send the transaction to the sequencer.
            let (sender, receiver) = oneshot::channel();
            if self.tx_input.send((buffer.freeze(), sender)).await.is_err() {
                panic!("Failed to sequence input bytes");
            }

            // Send an acknowledgement to the user. The meaning of this acknowledgement depends
            // on the consensus protocol. In this mock, it simply means that the transaction will
            // sequenced and sent to the authorities with 100% confidence as long as neither the
            // sequencer or the authorities crash.
            let reply = receiver
                .await
                .expect("Failed to receive reply from sequencer");
            let bytes = serialize_message(&reply);
            if let Err(e) = channel.sink().send(bytes.into()).await {
                log::error!("Failed to send query response: {}", e);
            }
        }
    }
}
