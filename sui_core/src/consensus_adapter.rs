// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use futures::SinkExt;
use narwhal_executor::SubscriberResult;
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    net::SocketAddr,
};
use sui_types::{
    committee::Committee,
    error::{SuiError, SuiResult},
    messages::{ConsensusTransaction, TransactionInfoResponse},
};
use tokio::{
    net::TcpStream,
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
    task::JoinHandle,
    time::{timeout, Duration},
};
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec};
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
    /// The network address of the consensus node.
    consensus_address: SocketAddr,
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
    /// Make a new Consensus submitter instance.
    pub fn new(
        consensus_address: SocketAddr,
        committee: Committee,
        tx_consensus_listener: Sender<ConsensusListenerMessage>,
        max_delay: Duration,
    ) -> Self {
        Self {
            consensus_address,
            committee,
            tx_consensus_listener,
            max_delay,
        }
    }

    /// Attempt to reconnect with a the consensus node.
    async fn reconnect(
        address: SocketAddr,
    ) -> SuiResult<FramedWrite<TcpStream, LengthDelimitedCodec>> {
        let stream = TcpStream::connect(address)
            .await
            .map_err(|e| SuiError::ConsensusConnectionBroken(e.to_string()))?;

        let stream = FramedWrite::new(stream, LengthDelimitedCodec::builder().new_codec());
        Ok(stream)
    }

    /// Check if this authority should submit the transaction to consensus.
    fn should_submit(_certificate: &ConsensusTransaction) -> bool {
        // TODO [issue #1647]: Right now every authority submits the transaction to consensus.
        true
    }

    /// Submit a transaction to consensus, wait for its processing, and notify the caller.
    pub async fn submit(
        &self,
        certificate: &ConsensusTransaction,
    ) -> SuiResult<TransactionInfoResponse> {
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
            // TODO [issue #1452]: We are re-creating a connection every time. This is wasteful but
            // does not require to take self as a mutable reference.
            Self::reconnect(self.consensus_address)
                .await?
                .send(bytes)
                .await
                .map_err(|e| SuiError::ConsensusConnectionBroken(e.to_string()))?;
        }

        // Wait for the consensus to sequence the certificate and assign locks to shared objects.
        // Since the consensus protocol may drop some messages, it is not guaranteed that our
        // certificate will be sequenced. So the best we can do is to set a timer and notify the
        // client to retry if we timeout without hearing back from consensus (this module does not
        // handle retries). The best timeout value depends on the consensus protocol.
        let resp = match timeout(self.max_delay, receiver).await {
            Ok(reply) => reply.expect("Failed to read back from consensus listener"),
            Err(e) => {
                let message = ConsensusListenerMessage::Cleanup(serialized);
                self.tx_consensus_listener
                    .send(message)
                    .await
                    .expect("Cleanup channel with consensus listener dropped");
                Err(SuiError::ConsensusConnectionBroken(e.to_string()))
            }
        };

        resp.and_then(|r| {
            bincode::deserialize(&r).map_err(|e| SuiError::ConsensusConnectionBroken(e.to_string()))
        })
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
                            let digest = Self::hash(&transaction);
                            if self.pending.len() < self.max_pending_transactions {
                                self.pending.entry(digest).or_insert_with(Vec::new).push(replier);
                            } else if replier.send(Err(SuiError::ListenerCapacityExceeded)).is_err() {
                                debug!("No replier to listen to consensus output {digest}");
                            }
                        },

                        // Stop waiting for a consensus transaction.
                        ConsensusListenerMessage::Cleanup(transaction) => {
                            let digest = Self::hash(&transaction);
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
                    let digest = Self::hash(&serialized);
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

    /// Hash serialized consensus transactions. We do not need specific cryptographic properties except
    /// only collision resistance.
    pub fn hash(serialized: &SerializedConsensusTransaction) -> ConsensusTransactionDigest {
        let mut hasher = DefaultHasher::new();
        serialized.hash(&mut hasher);
        hasher.finish()
    }
}
