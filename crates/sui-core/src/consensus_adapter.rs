// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::CheckpointLocals;
use crate::checkpoints::ConsensusSender;
use bytes::Bytes;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use multiaddr::Multiaddr;
use narwhal_executor::SubscriberResult;
use narwhal_types::TransactionProto;
use narwhal_types::TransactionsClient;
use std::collections::VecDeque;
use std::sync::Arc;
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
};
use sui_types::messages_checkpoint::CheckpointFragment;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::{
    committee::Committee,
    error::{SuiError, SuiResult},
    messages::ConsensusTransaction,
};
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
    task::JoinHandle,
    time::{timeout, Duration},
};
use tracing::debug;

#[cfg(test)]
#[path = "unit_tests/consensus_tests.rs"]
pub mod consensus_tests;

/// A serialized consensus transaction.
type SerializedConsensusTransaction = Vec<u8>;

/// The digest of a consensus transactions.
type ConsensusTransactionDigest = u64;

/// Transaction info response serialized by Sui.
type SerializedTransactionInfoResponse = Vec<u8>;

/// Channel to notify the caller when the Sui certificate has been sequenced.
type TxSequencedNotifier = oneshot::Sender<SuiResult<SerializedTransactionInfoResponse>>;

/// Message to notify the consensus listener that a new transaction has been sent to consensus
/// or that the caller timed out on a specific transaction.
#[derive(Debug)]
pub enum ConsensusListenerMessage {
    New(SerializedConsensusTransaction, TxSequencedNotifier),
    Cleanup(SerializedConsensusTransaction),
}

/// The message returned by the consensus to notify that a Sui certificate has been sequenced
/// and all its shared objects are locked.
type ConsensusOutput = (
    /* result */ SubscriberResult<SerializedTransactionInfoResponse>,
    /* transaction */ SerializedConsensusTransaction,
);

/// Submit Sui certificates to the consensus.
pub struct ConsensusAdapter {
    /// The network client connecting to the consensus node of this authority.
    consensus_client: TransactionsClient<sui_network::tonic::transport::Channel>,
    /// The Sui committee information.
    committee: Committee,
    /// A channel to notify the consensus listener to take action for a transactions.
    tx_consensus_listener: Sender<ConsensusListenerMessage>,
    /// The maximum duration to wait from consensus before aborting the transaction. After
    /// this delay passed, the client will be notified that its transaction was probably not
    /// sequence and it should try to resubmit its transaction.
    max_delay: Duration,
}

impl ConsensusAdapter {
    /// Make a new Consensus adapter instance.
    pub fn new(
        consensus_address: Multiaddr,
        committee: Committee,
        tx_consensus_listener: Sender<ConsensusListenerMessage>,
        max_delay: Duration,
    ) -> Self {
        let consensus_client = TransactionsClient::new(
            mysten_network::client::connect_lazy(&consensus_address)
                .expect("Failed to connect to consensus"),
        );
        Self {
            consensus_client,
            committee,
            tx_consensus_listener,
            max_delay,
        }
    }

    /// Check if this authority should submit the transaction to consensus.
    fn should_submit(_certificate: &ConsensusTransaction) -> bool {
        // TODO [issue #1647]: Right now every authority submits the transaction to consensus.
        true
    }

    /// Submit a transaction to consensus, wait for its processing, and notify the caller.
    pub async fn submit(&self, certificate: &ConsensusTransaction) -> SuiResult {
        // Check the Sui certificate (submitted by the user).
        certificate.verify(&self.committee)?;

        // Serialize the certificate in a way that is understandable to consensus (i.e., using
        // bincode) and it certificate to consensus.
        let serialized = bincode::serialize(certificate).expect("Failed to serialize consensus tx");
        let bytes = Bytes::from(serialized.clone());

        // Notify the consensus listener that we are expecting to process this certificate.
        let (sender, receiver) = oneshot::channel();
        let consensus_input = ConsensusListenerMessage::New(serialized.clone(), sender);
        self.tx_consensus_listener
            .send(consensus_input)
            .await
            .expect("Failed to notify consensus listener");

        // Check if this authority submits the transaction to consensus.
        if Self::should_submit(certificate) {
            self.consensus_client
                .clone()
                .submit_transaction(TransactionProto { transaction: bytes })
                .await
                .map_err(|e| SuiError::ConsensusConnectionBroken(format!("{:?}", e)))?;
        }

        // Wait for the consensus to sequence the certificate and assign locks to shared objects.
        // Since the consensus protocol may drop some messages, it is not guaranteed that our
        // certificate will be sequenced. So the best we can do is to set a timer and notify the
        // client to retry if we timeout without hearing back from consensus (this module does not
        // handle retries). The best timeout value depends on the consensus protocol.
        match timeout(self.max_delay, receiver).await {
            Ok(_) => Ok(()),
            Err(e) => {
                let message = ConsensusListenerMessage::Cleanup(serialized);
                self.tx_consensus_listener
                    .send(message)
                    .await
                    .expect("Cleanup channel with consensus listener dropped");
                Err(SuiError::FailedToHearBackFromConsensus(e.to_string()))
            }
        }
    }
}

/// This module interfaces the consensus with Sui. It receives certificates input to consensus and
/// notify the called when they are sequenced.
pub struct ConsensusListener {
    /// Receive messages input to the consensus.
    rx_consensus_input: Receiver<ConsensusListenerMessage>,
    /// Receive consensus outputs.
    rx_consensus_output: Receiver<ConsensusOutput>,
    /// The maximum number of pending replies. This cap indicates the maximum amount of client
    /// transactions submitted to consensus for which we keep track. If we submit more transactions
    /// than this cap, the transactions will be handled by consensus as usual but this module won't
    /// be keeping track of when they are sequenced. Its only purpose is to ensure the field called
    /// `pending` has a maximum size.
    max_pending_transactions: usize,
    /// Keep a map of all consensus inputs that are currently being sequenced.
    pending: HashMap<ConsensusTransactionDigest, Vec<TxSequencedNotifier>>,
}

impl ConsensusListener {
    /// Spawn a new consensus adapter in a dedicated tokio task.
    pub fn spawn(
        rx_consensus_input: Receiver<ConsensusListenerMessage>,
        rx_consensus_output: Receiver<ConsensusOutput>,
        max_pending_transactions: usize,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                rx_consensus_input,
                rx_consensus_output,
                max_pending_transactions,
                pending: HashMap::with_capacity(2 * max_pending_transactions),
            }
            .run()
            .await
        })
    }

    /// Hash serialized consensus transactions. We do not need specific cryptographic properties except
    /// only collision resistance.
    pub fn hash_serialized_transaction(
        serialized: &SerializedConsensusTransaction,
    ) -> ConsensusTransactionDigest {
        let mut hasher = DefaultHasher::new();
        serialized.hash(&mut hasher);
        hasher.finish()
    }

    /// Main loop receiving messages input to consensus and notifying the caller once the inputs
    /// are sequenced (of if an error happened).
    async fn run(&mut self) {
        loop {
            tokio::select! {
                // A new transaction has been sent to consensus or is no longer needed.
                Some(message) = self.rx_consensus_input.recv() => {
                    match message {
                        // Keep track of this certificates so we can notify the user later.
                        ConsensusListenerMessage::New(transaction, replier) => {
                            let digest = Self::hash_serialized_transaction(&transaction);
                            if self.pending.len() < self.max_pending_transactions {
                                self.pending.entry(digest).or_insert_with(Vec::new).push(replier);
                            } else if replier.send(Err(SuiError::ListenerCapacityExceeded)).is_err() {
                                debug!("No replier to listen to consensus output {digest}");
                            }
                        },

                        // Stop waiting for a consensus transaction.
                        ConsensusListenerMessage::Cleanup(transaction) => {
                            let digest = Self::hash_serialized_transaction(&transaction);
                            let _ = self.pending.get_mut(&digest).and_then(|x| x.pop());
                            if self.pending.get(&digest).map_or_else(|| false, |x| x.is_empty()) {
                                self.pending.remove(&digest);
                            }
                        }
                    }
                },

                // Notify the caller that the transaction has been sequenced (if there is a caller).
                Some((result, serialized)) = self.rx_consensus_output.recv() => {
                    let outcome = result.map_err(SuiError::from);
                    let digest = Self::hash_serialized_transaction(&serialized);
                    if let Some(repliers) = self.pending.remove(&digest) {
                        for replier in repliers {
                            if replier.send(outcome.clone()).is_err() {
                                debug!("No replier to listen to consensus output {digest}");
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Send checkpoint fragments through consensus.
pub struct CheckpointSender {
    tx_checkpoint_consensus_adapter: Sender<CheckpointFragment>,
}

impl CheckpointSender {
    pub fn new(tx_checkpoint_consensus_adapter: Sender<CheckpointFragment>) -> Self {
        Self {
            tx_checkpoint_consensus_adapter,
        }
    }
}

impl ConsensusSender for CheckpointSender {
    fn send_to_consensus(&self, fragment: CheckpointFragment) -> SuiResult {
        self.tx_checkpoint_consensus_adapter
            .try_send(fragment)
            .map_err(|e| SuiError::from(&e.to_string()[..]))
    }
}

/// Reliably submit checkpoints fragments to consensus.
pub struct CheckpointConsensusAdapter {
    /// The network client connecting to the consensus node of this authority.
    consensus_client: TransactionsClient<sui_network::tonic::transport::Channel>,
    /// Channel to request to be notified when a given consensus transaction is sequenced.
    tx_consensus_listener: Sender<ConsensusListenerMessage>,
    /// Receive new checkpoint fragments to sequence.
    rx_checkpoint_consensus_adapter: Receiver<CheckpointFragment>,
    /// A pointer to the checkpoints local store.
    checkpoint_locals: Arc<CheckpointLocals>,
    /// The initial delay to wait before re-attempting a connection with consensus (in ms).
    retry_delay: Duration,
    /// The maximum number of checkpoint fragment pending sequencing.
    max_pending_transactions: usize,
    /// Keep all checkpoint fragment waiting to be sequenced.
    buffer: VecDeque<(SerializedConsensusTransaction, CheckpointSequenceNumber)>,
}

impl CheckpointConsensusAdapter {
    /// Create a new `CheckpointConsensusAdapter`.
    pub fn new(
        consensus_address: Multiaddr,
        tx_consensus_listener: Sender<ConsensusListenerMessage>,
        rx_checkpoint_consensus_adapter: Receiver<CheckpointFragment>,
        checkpoint_locals: Arc<CheckpointLocals>,
        retry_delay: Duration,
        max_pending_transactions: usize,
    ) -> Self {
        // Create a new network client.
        let connection = mysten_network::client::connect_lazy(&consensus_address)
            .expect("Failed to connect to consensus");
        let consensus_client = TransactionsClient::new(connection);

        // Create the new instance.
        Self {
            consensus_client,
            tx_consensus_listener,
            rx_checkpoint_consensus_adapter,
            checkpoint_locals,
            retry_delay,
            max_pending_transactions,
            buffer: VecDeque::with_capacity(max_pending_transactions),
        }
    }

    /// Spawn a `CheckpointConsensusAdapter` in a dedicated tokio task.
    pub fn spawn(mut self) -> JoinHandle<()> {
        tokio::spawn(async move { self.run().await })
    }

    /// Submit a transaction to consensus.
    async fn submit(&self, serialized: SerializedConsensusTransaction) -> SuiResult {
        let transaction = Bytes::from(serialized);
        let proto_transaction = TransactionProto { transaction };
        self.consensus_client
            .clone()
            .submit_transaction(proto_transaction)
            .await
            .map_err(|e| SuiError::ConsensusConnectionBroken(format!("{:?}", e)))
            .map(|_| ())
    }

    /// Wait for a transaction to be sequenced by consensus (or to timeout).
    async fn waiter<T>(
        receiver: oneshot::Receiver<SuiResult<SerializedTransactionInfoResponse>>,
        retry_delay: Duration,
        deliver: T,
    ) -> (SuiResult<SerializedTransactionInfoResponse>, T) {
        let outcome = match timeout(retry_delay, receiver).await {
            Ok(reply) => reply.expect("Failed to read back from consensus listener"),
            Err(e) => Err(SuiError::FailedToHearBackFromConsensus(e.to_string())),
        };
        (outcome, deliver)
    }

    /// Main loop receiving checkpoint fragments to reliably submit to consensus.
    async fn run(&mut self) {
        let mut waiting = FuturesUnordered::new();

        // Continuously listen to checkpoint fragments and re-attempt sequencing if needed.
        loop {
            // Try to submit all pending checkpoint fragments to consensus.
            while let Some((serialized, sequence_number)) = self.buffer.pop_back() {
                match self.submit(serialized.clone()).await {
                    Ok(_) => {
                        // Notify the consensus listener that we wish to be notified once our
                        // consensus transaction is sequenced.
                        let (sender, receiver) = oneshot::channel();
                        let consensus_input =
                            ConsensusListenerMessage::New(serialized.clone(), sender);
                        self.tx_consensus_listener
                            .send(consensus_input)
                            .await
                            .expect("Failed to notify consensus listener");

                        // Add the receiver to the waiter. So we can retransmit if the
                        // connection fails.
                        let deliver = (serialized, sequence_number);
                        let future = Self::waiter(receiver, self.retry_delay, deliver);
                        waiting.push(future);
                    }
                    Err(_) => {
                        self.buffer.push_back((serialized, sequence_number));
                        break;
                    }
                }
            }

            // Process new events.
            tokio::select! {
                // Listen to new checkpoint fragments.
                Some(fragment) = self.rx_checkpoint_consensus_adapter.recv() => {
                    let sequence_number = *fragment.proposer_sequence_number();

                    // Cleanup the buffer.
                    if self.buffer.len() >= self.max_pending_transactions {
                        // Drop the earliest fragments. They are not needed for liveness.
                        if let Some(proposal) = &self.checkpoint_locals.current_proposal {
                            let current_sequence_number = proposal.sequence_number();
                            self.buffer.retain(|(_, s)| s >= current_sequence_number);
                        }
                    }

                    // Add the fragment to the buffer.
                    let transaction = ConsensusTransaction::Checkpoint(Box::new(fragment));
                    let serialized = bincode::serialize(&transaction)
                        .expect("Failed to serialize consensus tx");
                    self.buffer.push_front((serialized, sequence_number));
                },

                // Listen to checkpoint fragments who failed to be sequenced and need reties.
                Some((outcome, identifier)) = waiting.next() => {
                   if let Err(error) = outcome {
                       tracing::debug!("Failed to sequence transaction: {error}");
                       let (serialized_transaction, checkpoint_sequence_number) = identifier;
                       self.buffer.push_back((serialized_transaction, checkpoint_sequence_number));
                   }
                },
            }
        }
    }
}
