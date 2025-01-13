// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::SubscriptionMetrics;
use crate::proto::node::v2::GetFullCheckpointOptions;
use crate::proto::node::v2::GetFullCheckpointResponse;
use std::sync::Arc;
use sui_types::full_checkpoint_content::CheckpointData;
use tap::Pipe;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::error;
use tracing::info;
use tracing::trace;

const CHECKPOINT_MAILBOX_SIZE: usize = 1024;
const MAILBOX_SIZE: usize = 128;
const SUBSCRIPTION_CHANNEL_SIZE: usize = 256;
const MAX_SUBSCRIBERS: usize = 1024;

struct SubscriptionRequest {
    sender: oneshot::Sender<mpsc::Receiver<Arc<GetFullCheckpointResponse>>>,
}

#[derive(Clone)]
pub struct SubscriptionServiceHandle {
    sender: mpsc::Sender<SubscriptionRequest>,
}

impl SubscriptionServiceHandle {
    pub async fn register_subscription(
        &self,
    ) -> Option<mpsc::Receiver<Arc<GetFullCheckpointResponse>>> {
        let (sender, reciever) = oneshot::channel();
        let request = SubscriptionRequest { sender };
        self.sender.send(request).await.ok()?;

        reciever.await.ok()
    }
}

pub struct SubscriptionService {
    // Mailbox for recieving `CheckpointData` from the Checkpoint Executor
    //
    // Expectation is that checkpoints are recieved in-order
    checkpoint_mailbox: mpsc::Receiver<CheckpointData>,
    mailbox: mpsc::Receiver<SubscriptionRequest>,
    subscribers: Vec<mpsc::Sender<Arc<GetFullCheckpointResponse>>>,

    metrics: SubscriptionMetrics,
}

impl SubscriptionService {
    pub fn build(
        registry: &prometheus::Registry,
    ) -> (mpsc::Sender<CheckpointData>, SubscriptionServiceHandle) {
        let metrics = SubscriptionMetrics::new(registry);
        let (checkpoint_sender, checkpoint_mailbox) = mpsc::channel(CHECKPOINT_MAILBOX_SIZE);
        let (subscription_request_sender, mailbox) = mpsc::channel(MAILBOX_SIZE);

        tokio::spawn(
            Self {
                checkpoint_mailbox,
                mailbox,
                subscribers: Vec::new(),
                metrics,
            }
            .start(),
        );

        (
            checkpoint_sender,
            SubscriptionServiceHandle {
                sender: subscription_request_sender,
            },
        )
    }

    async fn start(mut self) {
        // Start main loop.
        loop {
            tokio::select! {
                maybe_checkpoint = self.checkpoint_mailbox.recv() => {
                    // Once all handles to our checkpoint_mailbox have been dropped this
                    // will yield `None` and we can terminate the event loop
                    if let Some(checkpoint) = maybe_checkpoint {
                        self.handle_checkpoint(checkpoint);
                    } else {
                        break;
                    }
                },
                maybe_message = self.mailbox.recv() => {
                    // Once all handles to our mailbox have been dropped this
                    // will yield `None` and we can terminate the event loop
                    if let Some(message) = maybe_message {
                        self.handle_message(message);
                    } else {
                        break;
                    }
                },
            }
        }

        info!("RPC Subscription Services ended");
    }

    fn handle_checkpoint(&mut self, checkpoint: CheckpointData) {
        // Check that we recieved checkpoints in-order
        {
            let last_sequence_number = self.metrics.last_recieved_checkpoint.get();
            let sequence_number = *checkpoint.checkpoint_summary.sequence_number() as i64;

            if last_sequence_number != 0 && (last_sequence_number + 1) != sequence_number {
                panic!(
                    "recieved checkpoint out-of-order. expected checkpoint {}, recieved {}",
                    last_sequence_number + 1,
                    sequence_number
                );
            }

            // Update the metric marking the latest checkpoint we've seen
            self.metrics.last_recieved_checkpoint.set(sequence_number);
        }

        let checkpoint =
            match crate::service::checkpoints::checkpoint_data_to_full_checkpoint_response(
                checkpoint,
                &GetFullCheckpointOptions::all().into(),
            ) {
                Ok(checkpoint) => GetFullCheckpointResponse::from(checkpoint).pipe(Arc::new),
                Err(e) => {
                    error!("unable to convert checkpoint to proto: {e:?}");
                    return;
                }
            };

        // Try to send the latest checkpoint to all subscribers. If a subscriber's channel is full
        // then they are likely too slow so we drop them.
        self.subscribers.retain(|subscriber| {
            match subscriber.try_send(Arc::clone(&checkpoint)) {
                Ok(()) => {
                    trace!("succesfully enqueued checkpont for subscriber");
                    true // Retain this subscriber
                }
                Err(e) => {
                    // It does not matter what the error is - channel full or closed, we drop the subscriber.
                    trace!("unable to enqueue checkpoint for subscriber: {e}");
                    self.metrics.inflight_subscribers.dec();
                    false // Drop this subscriber
                }
            }
        });
    }

    fn handle_message(&mut self, request: SubscriptionRequest) {
        // Check if we've reached the limit to the number of subscribers we can have at one time.
        if self.subscribers.len() >= MAX_SUBSCRIBERS {
            trace!(
                "failed to register new subscriber: hit maximum number of subscribers {}",
                MAX_SUBSCRIBERS
            );
            return;
        }

        let (sender, reciever) = mpsc::channel(SUBSCRIPTION_CHANNEL_SIZE);
        match request.sender.send(reciever) {
            Ok(()) => {
                trace!("succesfully registered new subscriber");
                self.metrics.inflight_subscribers.inc();
                self.subscribers.push(sender);
            }
            Err(e) => {
                trace!("failed to register new subscriber: {e:?}");
            }
        }
    }
}
