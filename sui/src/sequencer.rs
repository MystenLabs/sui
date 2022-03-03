// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::stream::futures_unordered::FuturesUnordered;
use futures::stream::StreamExt;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio::time::sleep;

/// A mock single-process sequencer. This will be replaced by a proper consensus protocol.
pub struct MockSequencer<Message> {
    /// The delay to wait before sequencing a message. This parameter emulates latency.
    delay: Duration,
    /// Receive input messages to sequence.
    rx_input: mpsc::Receiver<Message>,
    /// Deliver a sequence of messages.
    tx_output: broadcast::Sender<Message>,
}

impl<Message> MockSequencer<Message>
where
    Message: std::fmt::Debug + Send + Sync + 'static,
{
    /// Spawn a new `Sequencer` in a separate tokio task.
    pub fn spawn(
        delay: Duration,
        rx_input: mpsc::Receiver<Message>,
        tx_output: broadcast::Sender<Message>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                delay,
                rx_input,
                tx_output,
            }
            .run()
            .await;
        })
    }

    /// Helper function. It simply waits for a fixed delay and then returns the input.
    async fn waiter(deliver: Message, delay: Duration) -> Message {
        sleep(delay).await;
        deliver
    }

    /// Main loop ordering input bytes.
    async fn run(&mut self) {
        let mut waiting = FuturesUnordered::new();
        loop {
            tokio::select! {
                // Receive bytes to order.
                Some(message) = self.rx_input.recv() => {
                    waiting.push(Self::waiter(message, self.delay));
                },

                // Bytes are ready to be delivered.
                Some(message) = waiting.next() => {
                    if let Err(e) = self.tx_output.send(message) {
                        log::warn!("Failed to output sequence: {}", e);
                    }
                }
            }
        }
    }
}
