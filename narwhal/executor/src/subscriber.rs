// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    bail,
    errors::{SubscriberError, SubscriberResult},
};
use consensus::{ConsensusOutput, ConsensusSyncRequest};
use crypto::traits::VerifyingKey;
use futures::{
    future::try_join_all,
    stream::{FuturesOrdered, StreamExt},
};
use std::cmp::Ordering;
use store::Store;
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        watch,
    },
    task::JoinHandle,
};
use tracing::debug;
use types::{BatchDigest, ReconfigureNotification, SequenceNumber, SerializedBatchMessage};

#[cfg(test)]
#[path = "tests/subscriber_tests.rs"]
pub mod subscriber_tests;

/// The `Subscriber` receives certificates sequenced by the consensus and execute every
/// transaction it references. We assume that the messages we receives from consensus has
/// already been authenticated (ie. they really come from a trusted consensus node) and
/// integrity-validated (ie. no corrupted messages).
pub struct Subscriber<PublicKey: VerifyingKey> {
    /// The temporary storage holding all transactions' data (that may be too big to hold in memory).
    store: Store<BatchDigest, SerializedBatchMessage>,
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
    /// A channel to receive consensus messages.
    rx_consensus: Receiver<ConsensusOutput<PublicKey>>,
    /// A channel to send sync request to consensus for missed messages.
    tx_consensus: Sender<ConsensusSyncRequest>,
    /// A channel to the batch loader to download transaction's data.
    tx_batch_loader: Sender<ConsensusOutput<PublicKey>>,
    /// A channel to send the complete and ordered list of consensus outputs to the executor. This
    /// channel is used once all transactions data are downloaded.
    tx_executor: Sender<ConsensusOutput<PublicKey>>,
    /// The index of the next expected consensus output.
    next_consensus_index: SequenceNumber,
}

impl<PublicKey: VerifyingKey> Subscriber<PublicKey> {
    /// Spawn a new subscriber in a new tokio task.
    pub fn spawn(
        store: Store<BatchDigest, SerializedBatchMessage>,
        rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
        rx_consensus: Receiver<ConsensusOutput<PublicKey>>,
        tx_consensus: Sender<ConsensusSyncRequest>,
        tx_batch_loader: Sender<ConsensusOutput<PublicKey>>,
        tx_executor: Sender<ConsensusOutput<PublicKey>>,
        next_consensus_index: SequenceNumber,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                store,
                rx_reconfigure,
                rx_consensus,
                tx_consensus,
                tx_batch_loader,
                tx_executor,
                next_consensus_index,
            }
            .run()
            .await
            .expect("Failed to run subscriber")
        })
    }

    /// Synchronize with the consensus in case we missed part of its output sequence.
    /// It is safety-critical that we process the consensus' outputs in the complete
    /// and right order. This function reads the consensus outputs out of a stream and
    /// return them in the right order.
    async fn synchronize(
        &mut self,
        last_known_client_index: SequenceNumber,
        last_known_server_index: SequenceNumber,
    ) -> SubscriberResult<Vec<ConsensusOutput<PublicKey>>> {
        // Send a sync request.
        let request = ConsensusSyncRequest {
            missing: (last_known_client_index + 1..=last_known_server_index),
        };
        self.tx_consensus
            .send(request)
            .await
            .map_err(|_| SubscriberError::ConsensusConnectionDropped)?;

        // Read the replies.
        let mut next_ordinary_sequence = last_known_server_index + 1;
        let mut next_catchup_sequence = last_known_client_index + 1;
        let mut buffer = Vec::new();
        let mut sequence = Vec::new();
        loop {
            let output = match self.rx_consensus.recv().await {
                Some(x) => x,
                None => bail!(SubscriberError::ConsensusConnectionDropped),
            };
            let consensus_index = output.consensus_index;

            if consensus_index == next_ordinary_sequence {
                buffer.push(output);
                next_ordinary_sequence += 1;
            } else if consensus_index == next_catchup_sequence {
                sequence.push(output);
                next_catchup_sequence += 1;
            } else {
                bail!(SubscriberError::UnexpectedConsensusIndex(consensus_index));
            }

            if consensus_index == last_known_server_index {
                break;
            }
        }

        sequence.extend(buffer);
        Ok(sequence)
    }

    /// Process a single consensus output message. If we realize we are missing part of the sequence,
    /// we first sync every missing output and return them on the right order.
    async fn handle_consensus_message(
        &mut self,
        message: &ConsensusOutput<PublicKey>,
    ) -> SubscriberResult<Vec<ConsensusOutput<PublicKey>>> {
        let consensus_index = message.consensus_index;

        // Check that the latest consensus index is as expected; otherwise synchronize.
        let need_to_sync = match self.next_consensus_index.cmp(&consensus_index) {
            Ordering::Greater => {
                // That is fine, it may happen when the consensus node crashes and recovers.
                debug!("Consensus index of authority bigger than expected");
                return Ok(Vec::default());
            }
            Ordering::Less => {
                debug!("Subscriber is synchronizing missed consensus output messages");
                true
            }
            Ordering::Equal => false,
        };

        // Send the certificate to the batch loader to download all transactions' data.
        self.tx_batch_loader
            .send(message.clone())
            .await
            .expect("Failed to send message ot batch loader");

        // Synchronize missing consensus outputs if we need to.
        if need_to_sync {
            let last_known_client_index = self.next_consensus_index;
            let last_known_server_index = message.consensus_index;
            self.synchronize(last_known_client_index, last_known_server_index)
                .await
        } else {
            Ok(vec![message.clone()])
        }
    }

    /// Wait for particular data to become available in the storage and then returns.
    async fn waiter<T>(
        missing: Vec<BatchDigest>,
        store: Store<BatchDigest, SerializedBatchMessage>,
        deliver: T,
    ) -> SubscriberResult<T> {
        let waiting: Vec<_> = missing.into_iter().map(|x| store.notify_read(x)).collect();
        try_join_all(waiting)
            .await
            .map(|_| deliver)
            .map_err(SubscriberError::from)
    }

    /// Main loop connecting to the consensus to listen to sequence messages.
    async fn run(&mut self) -> SubscriberResult<()> {
        let mut waiting = FuturesOrdered::new();

        // Listen to sequenced consensus message and process them.
        loop {
            tokio::select! {
                // Receive the ordered sequence of consensus messages from a consensus node.
                Some(message) = self.rx_consensus.recv() => {
                    // Process the consensus message (synchronize missing messages, download transaction data).
                    let sequence = self.handle_consensus_message(&message).await?;

                    // Update the latest consensus index. The state will atomically persist the change when
                    // executing the transaction. It is important to increment the consensus index before
                    // deserializing the transaction data because the consensus core will increment its own
                    // index regardless of deserialization or other application-specific failures.
                    self.next_consensus_index += sequence.len() as SequenceNumber;

                    // Wait for the transaction data to be available in the store. We will then execute the transactions.
                    for message in sequence {
                        let digests = message.certificate.header.payload.keys().cloned().collect();
                        let future = Self::waiter(digests, self.store.clone(), message);
                        waiting.push(future);
                    }
                },

                // Receive here consensus messages for which we have downloaded all transactions data.
                Some(message) = waiting.next() => self
                    .tx_executor
                    .send(message?)
                    .await
                    .map_err(|_| SubscriberError::ExecutorConnectionDropped)?,

                // Check whether the committee changed.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewCommittee(_) => (),
                        ReconfigureNotification::Shutdown => return Ok(())
                    }
                }
            }
        }
    }
}
