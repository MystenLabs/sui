// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tokio::sync::broadcast;
use tokio::sync::broadcast::error::SendError;

/// PreSubscribedBroadcastSender is a wrapped Broadcast channel that limits
/// subscription to initialization time. This is designed to be used for cancellation
/// signal to all the components, and the limitation is intended to prevent a component missing
/// the shutdown signal due to a subscription that happens after the shutdown signal was sent.
/// The receivers have a special peek method which can be used to conditionally check for
/// shutdown signal on the channel.
pub struct PreSubscribedBroadcastSender {
    sender: broadcast::Sender<()>,
    receivers: Vec<ConditionalBroadcastReceiver>,
}

#[derive(Debug)]
pub struct ConditionalBroadcastReceiver {
    pub receiver: broadcast::Receiver<()>,
}

/// ConditionalBroadcastReceiver has an additional method for convenience to be able to use
/// to conditionally check for shutdown in all branches of a select statement. Using this method
/// will allow for the shutdown signal to propagate faster, sice we will no longer be waiting
/// until the branch that checks the receiver is randomly selected by the select macro.
impl ConditionalBroadcastReceiver {
    pub async fn received_signal(&mut self) -> bool {
        futures::future::poll_immediate(&mut Box::pin(self.receiver.recv()))
            .await
            .is_some()
    }
}

impl PreSubscribedBroadcastSender {
    pub fn new(num_subscribers: u64) -> Self {
        let (tx_init, _) = broadcast::channel(1);
        let mut receivers = Vec::new();
        for _i in 0..num_subscribers {
            receivers.push(ConditionalBroadcastReceiver {
                receiver: tx_init.subscribe(),
            });
        }

        PreSubscribedBroadcastSender {
            sender: tx_init,
            receivers,
        }
    }

    pub fn try_subscribe(&mut self) -> Option<ConditionalBroadcastReceiver> {
        self.receivers.pop()
    }

    pub fn subscribe(&mut self) -> ConditionalBroadcastReceiver {
        self.receivers.pop().expect("No remaining subscribers ")
    }

    pub fn subscribe_n(&mut self, n: u64) -> Vec<ConditionalBroadcastReceiver> {
        let mut output = Vec::new();
        for _ in 0..n {
            output.push(self.subscribe());
        }
        output
    }

    pub fn send(&self) -> Result<usize, SendError<()>> {
        self.sender.send(())
    }
}

#[tokio::test]
async fn test_pre_subscribed_broadcast() {
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(2);
    let mut rx_shutdown_a = tx_shutdown.try_subscribe().unwrap();

    let a = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = rx_shutdown_a.receiver.recv() => {
                    return 1
                }

                _ = async{}, if true => {
                    if rx_shutdown_a.received_signal().await {
                        return 1
                    }
                }
            }
        }
    });

    let mut rx_shutdown_b = tx_shutdown.try_subscribe().unwrap();
    let rx_shutdown_c = tx_shutdown.try_subscribe();

    assert!(rx_shutdown_c.is_none());

    // send the shutdown signal before we start component b and started listening for shutdown there
    assert!(tx_shutdown.send().is_ok());

    let b = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = rx_shutdown_b.receiver.recv() => {
                    return 2
                }

                _ = async{}, if true => {
                    if rx_shutdown_b.received_signal().await {
                        return 2
                    }
                }
            }
        }
    });

    // assert that both component a and b loops have exited, effectively shutting down
    assert_eq!(a.await.unwrap() + b.await.unwrap(), 3);
}

#[tokio::test]
async fn test_conditional_broadcast_receiver() {
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(2);
    let mut rx_shutdown = tx_shutdown.try_subscribe().unwrap();

    let a = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = async{}, if true => {
                    if rx_shutdown.received_signal().await {
                        return 1
                    }
                }
            }
        }
    });

    assert!(tx_shutdown.send().is_ok());

    assert_eq!(a.await.unwrap(), 1);
}
