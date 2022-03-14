// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::futures_unordered::FuturesUnordered;
use futures::stream::StreamExt;
use futures::SinkExt;
use std::net::SocketAddr;
use std::time::Duration;
use sui_network::transport::{MessageHandler, RwChannel};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::time::sleep;

/// A mock single-process sequencer. It is not crash-safe (it has no storage) and should
/// only be used for testing.
pub struct Sequencer {
    /// The network address where to receive input messages.
    pub input_address: SocketAddr,
    /// The network address where to receive subscriber requests.
    pub subscriber_address: SocketAddr,
    /// The network buffer size.
    pub buffer_size: usize,
    /// The delay to wait before sequencing a message. This parameter is only required to
    /// emulates the consensus' latency.
    pub consensus_delay: Duration,
}

impl Sequencer {
    pub async fn spawn(sequencer: Self) {
        let (tx_input, rx_input) = channel(100);
        let (tx_subscriber, rx_subscriber) = channel(100);

        // Spawn the sequencer core.
        let core = SequencerCore {
            rx_input,
            rx_subscriber,
        };
        SequencerCore::spawn(core, sequencer.consensus_delay);

        // Spawn the server receiving input messages to order.
        let input_server = InputServer { tx_input };
        sui_network::transport::spawn_server(
            &sequencer.input_address.to_string(),
            input_server,
            sequencer.buffer_size,
        )
        .await
        .unwrap();

        // Spawn the server receiving subscribers to the output of the sequencer.
        let subscriber_server = SubscriberServer { tx_subscriber };
        sui_network::transport::spawn_server(
            &sequencer.subscriber_address.to_string(),
            subscriber_server,
            sequencer.buffer_size,
        )
        .await
        .unwrap();
    }
}

/// The core of the sequencer, totally ordering input bytes.
pub struct SequencerCore<Message> {
    /// Receive users' certificates to sequence
    pub rx_input: Receiver<Message>,
    /// Receive subscribers to update with the output of the sequence.
    pub rx_subscriber: Receiver<Sender<Message>>,
}

impl<Message> SequencerCore<Message>
where
    Message: Clone + Send + Sync + 'static,
{
    /// Spawn the core sequencer in a new tokio task.
    pub fn spawn(mut core: Self, consensus_delay: Duration) {
        tokio::spawn(async move {
            core.run(consensus_delay).await;
        });
    }

    /// Simply wait for a fixed delay and then returns the input.
    async fn waiter(deliver: Message, delay: Duration) -> Message {
        sleep(delay).await;
        deliver
    }

    /// Main loop ordering input bytes.
    async fn run(&mut self, consensus_delay: Duration) {
        let mut waiting = FuturesUnordered::new();
        let mut subscribers = Vec::new();
        loop {
            tokio::select! {
                // Receive bytes to order.
                Some(message) = self.rx_input.recv() => {
                    waiting.push(Self::waiter(message, consensus_delay));
                },

                // Receive subscribers to update with the sequencer's output.
                Some(sender) = self.rx_subscriber.recv() => {
                    subscribers.push(sender);
                }

                // Bytes are ready to be delivered, notify the subscribers.
                Some(message) = waiting.next() => {
                    let mut dropped = Vec::new();
                    for (i, subscriber) in subscribers.iter().enumerate() {
                        if subscriber.send(message.clone()).await.is_err() {
                            dropped.push(i);
                        }
                    }
                    for index in dropped {
                        subscribers.remove(index);
                    }
                }
            }
        }
    }
}

/// Define how the network server should handle incoming clients' certificates. This
/// is not got to stream many input transactions (benchmarks) as the task handling the
/// TCP connection blocks until a reply is ready.
struct InputServer {
    /// Send user transactions to the sequencer.
    pub tx_input: Sender<Bytes>,
}

#[async_trait]
impl<'a, Stream> MessageHandler<Stream> for InputServer
where
    Stream: 'static + RwChannel<'a> + Unpin + Send,
{
    async fn handle_messages(&self, mut stream: Stream) {
        loop {
            // Read the user's certificate.
            let buffer = match stream.stream().next().await {
                Some(Ok(buffer)) => buffer,
                Some(Err(e)) => {
                    log::warn!("Error while reading TCP stream: {}", e);
                    break;
                }
                None => {
                    log::debug!("Connection dropped by the client");
                    break;
                }
            };

            // Send the certificate to the sequencer.
            if self.tx_input.send(buffer.freeze()).await.is_err() {
                panic!("Failed to sequence input bytes");
            }
        }
    }
}

/// Define how the network server should handle incoming authorities sync requests.
/// The authorities are basically light clients of the sequencer. A real consensus
/// implementation would make sure to receive an ack from the authorities and retry
/// sending until the message is delivered. This is safety-critical.
struct SubscriberServer {
    /// Notify the sequencer's core of a new subscriber.
    pub tx_subscriber: Sender<Sender<Bytes>>,
}

#[async_trait]
impl<'a, Stream> MessageHandler<Stream> for SubscriberServer
where
    Stream: 'static + RwChannel<'a> + Unpin + Send,
{
    async fn handle_messages(&self, mut stream: Stream) {
        let (tx_output, mut rx_output) = channel(100);

        // Notify the core of a new subscriber.
        self.tx_subscriber
            .send(tx_output)
            .await
            .expect("Failed to send new subscriber to core");

        // Update the subscriber every time a certificate is sequenced.
        while let Some(certificate) = rx_output.recv().await {
            if stream.sink().send(certificate.into()).await.is_err() {
                log::debug!("Connection dropped by subscriber");
                break;
            }
        }
    }
}
