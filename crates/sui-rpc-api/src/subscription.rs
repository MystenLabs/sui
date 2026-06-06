// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::SubscriptionMetrics;
use std::sync::Arc;
use sui_types::full_checkpoint_content::Checkpoint;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::info;
use tracing::trace;
use tracing::warn;

const CHECKPOINT_MAILBOX_SIZE: usize = 1024;
const MAILBOX_SIZE: usize = 128;
const SUBSCRIPTION_CHANNEL_SIZE: usize = 256;
const MAX_SUBSCRIBERS: usize = 1024;

struct SubscriptionRequest {
    sender: oneshot::Sender<mpsc::Receiver<Arc<Checkpoint>>>,
}

#[derive(Clone)]
pub struct SubscriptionServiceHandle {
    sender: mpsc::Sender<SubscriptionRequest>,
}

impl SubscriptionServiceHandle {
    pub async fn register_subscription(&self) -> Option<mpsc::Receiver<Arc<Checkpoint>>> {
        let (sender, receiver) = oneshot::channel();
        let request = SubscriptionRequest { sender };
        self.sender.send(request).await.ok()?;

        receiver.await.ok()
    }
}

pub struct SubscriptionService {
    // Broadcast receiver for `Checkpoint`s published by the Checkpoint Executor.
    //
    // The executor publishes non-blocking, so a slow service can fall behind
    // and observe `RecvError::Lagged`; checkpoints delivered between lags arrive
    // in-order.
    checkpoint_mailbox: broadcast::Receiver<Arc<Checkpoint>>,
    mailbox: mpsc::Receiver<SubscriptionRequest>,
    subscribers: Vec<mpsc::Sender<Arc<Checkpoint>>>,

    metrics: SubscriptionMetrics,
}

impl SubscriptionService {
    pub fn build(
        registry: &prometheus::Registry,
    ) -> (
        broadcast::Sender<Arc<Checkpoint>>,
        SubscriptionServiceHandle,
    ) {
        let metrics = SubscriptionMetrics::new(registry);
        let (checkpoint_sender, checkpoint_mailbox) = broadcast::channel(CHECKPOINT_MAILBOX_SIZE);
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
                result = self.checkpoint_mailbox.recv() => {
                    match result {
                        Ok(checkpoint) => self.handle_checkpoint(checkpoint),
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            self.handle_lag(skipped);
                        }
                        // Once the executor drops the sender this yields `Closed`
                        // and we can terminate the event loop.
                        Err(broadcast::error::RecvError::Closed) => break,
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

    fn handle_checkpoint(&mut self, checkpoint: Arc<Checkpoint>) {
        // Check that we recieved checkpoints in-order. The broadcast stream
        // preserves send order, and any gap surfaces separately as `Lagged`
        // (see `handle_lag`), so reaching here out-of-order indicates an
        // executor bug.
        {
            let last_sequence_number = self.metrics.last_recieved_checkpoint.get();
            let sequence_number = *checkpoint.summary.sequence_number() as i64;

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

        // Try to send the latest checkpoint to all subscribers. If a subscriber's channel is full
        // then they are likely too slow so we drop them.
        self.subscribers.retain(|subscriber| {
            match subscriber.try_send(Arc::clone(&checkpoint)) {
                Ok(()) => {
                    trace!("successfully enqueued checkpont for subscriber");
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

    /// Drop every in-progress subscription after the service fell behind the
    /// broadcast stream. Having missed `skipped` checkpoints we can no longer
    /// deliver an in-order, gap-free stream to any subscriber, and clients
    /// already tolerate connection breaks and reconnect, so tearing them all
    /// down is cheaper than trying to resynchronize each one.
    fn handle_lag(&mut self, skipped: u64) {
        warn!(
            skipped,
            "subscription service lagged behind the checkpoint stream; \
             dropping all in-progress subscriptions"
        );
        let dropped = self.subscribers.len() as i64;
        self.subscribers.clear();
        self.metrics.inflight_subscribers.sub(dropped);
        // The next delivered checkpoint jumps ahead by `skipped`; reset the
        // in-order tracker so it is not mistaken for an out-of-order delivery.
        self.metrics.last_recieved_checkpoint.set(0);
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
                trace!("successfully registered new subscriber");
                self.metrics.inflight_subscribers.inc();
                self.subscribers.push(sender);
            }
            Err(e) => {
                trace!("failed to register new subscriber: {e:?}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    fn test_service() -> SubscriptionService {
        let (_checkpoint_sender, checkpoint_mailbox) = broadcast::channel(16);
        let (_request_sender, mailbox) = mpsc::channel(16);
        SubscriptionService {
            checkpoint_mailbox,
            mailbox,
            subscribers: Vec::new(),
            metrics: SubscriptionMetrics::new(&prometheus::Registry::new()),
        }
    }

    fn checkpoint(sequence_number: u64) -> Arc<Checkpoint> {
        Arc::new(TestCheckpointBuilder::new(sequence_number).build_checkpoint())
    }

    /// Register a subscriber the same way `handle_message` would, returning the
    /// receiving half a client would hold.
    fn add_subscriber(service: &mut SubscriptionService) -> mpsc::Receiver<Arc<Checkpoint>> {
        let (sender, receiver) = mpsc::channel(SUBSCRIPTION_CHANNEL_SIZE);
        service.metrics.inflight_subscribers.inc();
        service.subscribers.push(sender);
        receiver
    }

    #[tokio::test]
    async fn handle_checkpoint_fans_out_in_order() {
        let mut service = test_service();
        let mut receiver = add_subscriber(&mut service);

        service.handle_checkpoint(checkpoint(1));
        service.handle_checkpoint(checkpoint(2));

        assert_eq!(*receiver.recv().await.unwrap().summary.sequence_number(), 1);
        assert_eq!(*receiver.recv().await.unwrap().summary.sequence_number(), 2);
        assert_eq!(service.subscribers.len(), 1);
    }

    #[tokio::test]
    async fn handle_checkpoint_drops_departed_subscriber() {
        let mut service = test_service();
        let receiver = add_subscriber(&mut service);
        drop(receiver);

        service.handle_checkpoint(checkpoint(1));

        assert!(service.subscribers.is_empty());
        assert_eq!(service.metrics.inflight_subscribers.get(), 0);
    }

    #[tokio::test]
    async fn handle_lag_drops_all_subscribers_and_resets_tracker() {
        let mut service = test_service();
        let mut receiver_1 = add_subscriber(&mut service);
        let mut receiver_2 = add_subscriber(&mut service);

        service.handle_checkpoint(checkpoint(5));
        assert_eq!(service.metrics.last_recieved_checkpoint.get(), 5);

        service.handle_lag(10);

        assert!(service.subscribers.is_empty());
        assert_eq!(service.metrics.inflight_subscribers.get(), 0);
        // Both subscriptions are torn down, so the client streams close.
        assert!(receiver_1.recv().await.is_some()); // checkpoint 5, then closed
        assert!(receiver_1.recv().await.is_none());
        assert!(receiver_2.recv().await.is_some());
        assert!(receiver_2.recv().await.is_none());
        // The tracker is reset so the next, jumped-ahead checkpoint is not
        // mistaken for an out-of-order delivery (which would panic).
        assert_eq!(service.metrics.last_recieved_checkpoint.get(), 0);

        let mut receiver_3 = add_subscriber(&mut service);
        service.handle_checkpoint(checkpoint(100));
        assert_eq!(
            *receiver_3.recv().await.unwrap().summary.sequence_number(),
            100
        );
    }
}
