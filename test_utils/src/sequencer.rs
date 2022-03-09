// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use futures::stream::futures_unordered::FuturesUnordered;
use futures::stream::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc::{Sender, channel, Receiver};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use std::net::SocketAddr;
use async_trait::async_trait;
use network_utils::NetworkClient;
use network_utils::transport::{SpawnedServer, RwChannel, MessageHandler};

/// A mock single-process sequencer. It is not crash-safe (it has no storage) and only used
/// for testing. It will be replaced by a proper consensus protocol.
pub struct MockSequencer {
    /// The delay to wait before sequencing a message. This parameter is only required to
    /// emulates the consensus' latency.
    consensus_delay: Duration,
    network_clients: Vec<(SocketAddr, NetworkClient)>,
    buffer_size: usize,
    message_handler: SequencerMessageHandler,
    rx_input: Receiver<Bytes>
}

impl MockSequencer {
    /// Create a new mock sequencer.
    pub fn new(
        consensus_delay: Duration,
        authorities_addresses: Vec<SocketAddr>,
        buffer_size: usize,
        send_timeout: Duration,
        recv_timeout: Duration,
    ) -> Self {
        // Create one network client per authority.
        let network_clients = authorities_addresses.iter().map(|address| {
            (
                address,
                NetworkClient::new(
                    base_address: address.ip(),
                    base_port: address.port(),
                    buffer_size,
                    send_timeout,
                    recv_timeout,
                )
            )
        }).collect(); 

        // Create a message handler (to be used by the network receiver).
        let (tx_input, rx_input) = channel(100);
        let message_handler = SequencerMessageHandler {tx_input};

        // Create 
        Self {
            consensus_delay,
            network_clients,
            buffer_size,
            message_handler,
            rx_input,
        }
    }

    /// Spawn the sequencer and its network server (each in a separate tokio task).
    pub async fn spawn(&mut self) -> Result<SpawnedServer, std::io::Error> {
        self.spawn_sequencer().await;
        network_utils::transport::spawn_server(address, message_handler, buffer_size).await
    }

    /// Helper function. It simply waits for a fixed delay and then returns the input.
    async fn waiter(deliver: Bytes, delay: Duration) -> Bytes {
        sleep(delay).await;
        deliver
    }

    /// Main loop ordering input bytes.
    async fn spawn_sequencer(&mut self) {
        tokio::spawn(async move {
            let mut waiting = FuturesUnordered::new();
            loop {
                tokio::select! {
                    // Receive bytes to order.
                    Some(message) = self.rx_input.recv() => {
                        waiting.push(Self::waiter(message, self.consensus_delay));
                    },

                    // Bytes are ready to be delivered.
                    Some(message) = waiting.next() => {
                        for (address, client) in &self.network_clients {
                            // A real consensus implementation would make sure to receive an 
                            // ack from the authorities and retry sending until the message is 
                            // delivered. This is safety-critical.
                            if let Err(e) = client.send_recv_bytes(message) {
                                log::warn!("Failed to send output sequence to {}: {}", address, e);
                            }
                        }
                    }
                }
            }
        });
    }
}

struct SequencerMessageHandler {
    /// Send user transactions to the sequencer.
    pub tx_input: Sender<Bytes>
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
            let bytes = buffer;
            self.tx_input.send(bytes).await.expect("Failed to sequence input bytes");

            // Send an acknowledgement to the user. The meaning of this acknowledgement depends
            // on the consensus protocol. In this mock, it simply means that the transaction will 
            // sequenced and sent to the authorities with 100% confidence as long as neither the
            // sequencer or the authorities crash.
            let reply = Bytes::from("Ok");
            if let Err(e) = channel.sink().send(reply.into()).await {
                log::error!("Failed to send query response: {}", e);
            }
        }
    }
}
