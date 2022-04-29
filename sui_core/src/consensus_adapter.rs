// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::AuthorityState;
use bytes::Bytes;
use futures::SinkExt;
use narwhal_executor::SubscriberResult;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::sync::Arc;
use sui_network::transport;
use sui_network::transport::{RwChannel, TcpDataStream};
use sui_types::committee::Committee;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::ConfirmationTransaction;
use sui_types::messages::ConsensusTransaction;
use sui_types::messages::TransactionInfoResponse;
use sui_types::serialize::deserialize_transaction_info;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};
use tracing::debug;

#[cfg(test)]
#[path = "unit_tests/consensus_tests.rs"]
pub mod consensus_tests;

/// A serialized consensus transaction.
type SerializedConsensusTransaction = Vec<u8>;

/// The digest of a consensus transactions
type ConsensusTransactionDigest = u64;

/// The message returned by the consensus to notify that a Sui certificate has been sequenced
/// and all its shared objects are locked.
type ConsensusOutput = (SubscriberResult<Vec<u8>>, SerializedConsensusTransaction);

/// Channel to notify the called when the Sui certificate has been sequenced.
type Replier = oneshot::Sender<SuiResult<TransactionInfoResponse>>;

/// Message to notify the consensus adapter of a new certificate sent to consensus.
#[derive(Debug)]
pub struct ConsensusInput {
    serialized: SerializedConsensusTransaction,
    replier: Replier,
}

/// Submit Sui certificates to the consensus.
pub struct ConsensusAdapter {
    /// The network address of the consensus node.
    consensus_address: SocketAddr,
    /// The network buffer size.
    buffer_size: usize,
    /// The Sui committee information.
    committee: Committee,
    /// A channel to notify the consensus listener of new transactions.
    tx_consensus_listener: Sender<ConsensusInput>,
    /// The maximum duration to wait from consensus before aborting the transaction.
    max_delay: Duration,
}

impl ConsensusAdapter {
    /// Make a new Consensus submitter instance.
    pub fn new(
        consensus_address: SocketAddr,
        buffer_size: usize,
        committee: Committee,
        tx_consensus_listener: Sender<ConsensusInput>,
        max_delay: Duration,
    ) -> Self {
        Self {
            consensus_address,
            buffer_size,
            committee,
            tx_consensus_listener,
            max_delay,
        }
    }

    /// Attempt to reconnect with a the consensus node.
    async fn reconnect(address: SocketAddr, buffer_size: usize) -> SuiResult<TcpDataStream> {
        transport::connect(address.to_string(), buffer_size)
            .await
            .map_err(|e| SuiError::ConsensusConnectionBroken(e.to_string()))
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
        certificate.check(&self.committee)?;

        // Serialize the certificate in a way that is understandable to consensus (i.e., using
        // bincode) and it certificate to consensus.
        //let serialized = serialize_consensus_transaction(certificate);
        let serialized = bincode::serialize(certificate).expect("Failed to serialize consensus tx");
        let bytes = Bytes::from(serialized.clone());

        // Notify the consensus listener that we are expecting to process this certificate.
        let (sender, receiver) = oneshot::channel();
        let consensus_input = ConsensusInput {
            serialized,
            replier: sender,
        };
        self.tx_consensus_listener
            .send(consensus_input)
            .await
            .expect("Failed to notify consensus listener");

        // Check if this authority submits the transaction to consensus.
        if Self::should_submit(certificate) {
            // TODO [issue #1452]: We are re-creating a connection every time. This is wasteful but
            // does not require to take self as a mutable reference.
            Self::reconnect(self.consensus_address, self.buffer_size)
                .await?
                .sink()
                .send(bytes)
                .await
                .map_err(|e| SuiError::ConsensusConnectionBroken(e.to_string()))?;
        }

        // Wait for the consensus to sequence the certificate and assign locks to shared objects.
        timeout(self.max_delay, receiver)
            .await
            .map_err(|e| SuiError::ConsensusConnectionBroken(e.to_string()))?
            .expect("Channel with consensus listener dropped")
    }
}

/// This module interfaces the consensus with Sui. It receives certificates input to consensus and
/// notify the called when they are sequenced.
pub struct ConsensusListener {
    /// The authority state to execute shared-object transactions.
    state: Arc<AuthorityState>,
    /// Receive messages input to the consensus.
    rx_consensus_input: Receiver<ConsensusInput>,
    /// Receive consensus outputs.
    rx_consensus_output: Receiver<ConsensusOutput>,
    /// Keep a map of all consensus inputs that are currently being sequenced.
    pending: HashMap<ConsensusTransactionDigest, Vec<Replier>>,
}

impl ConsensusListener {
    /// Spawn a new consensus adapter in a dedicated tokio task.
    pub fn spawn(
        state: Arc<AuthorityState>,
        rx_consensus_input: Receiver<ConsensusInput>,
        rx_consensus_output: Receiver<ConsensusOutput>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                state,
                rx_consensus_input,
                rx_consensus_output,
                pending: HashMap::new(),
            }
            .run()
            .await
        })
    }

    /// Execute sequenced transactions and notify the end-user (if any).
    async fn process_consensus_output(&mut self, output: ConsensusOutput) {
        let (result, serialized) = output;

        // Execute the transaction (if the consensus successfully sequenced it).
        /*
        let outcome = match result {
            Ok(()) => {
                let message =
                    bincode::deserialize(&serialized).expect("Failed to deserialize consensus tx");
                let ConsensusTransaction::UserTransaction(certificate) = message;
                let confirmation_transaction = ConfirmationTransaction { certificate };

                // Can the following issue happen? (i) Consensus attributes locks to a shared objects
                // then (ii) We execute the transaction. However, the store didn't finish to persist
                // the locks.
                //tokio::task::yield_now().await;

                let transaction_digest = confirmation_transaction.certificate.transaction.digest();
                if self
                    .state
                    ._database
                    .effects_exists(&transaction_digest)
                    .unwrap()
                // UNWRAP!!
                {
                    let transaction_info =
                        self.state.make_transaction_info(&transaction_digest).await;

                    // Need to increment the sequence number.
                    self.state
                        ._database
                        .update_shared_locks(confirmation_transaction.certificate.clone())
                        .unwrap();

                    println!(
                        "{:?}: {transaction_digest:?} already executed, returning Ok & updating seq",
                        self.state.name
                    );
                    transaction_info
                } else {
                    let x = self
                        .state
                        .handle_confirmation_transaction(confirmation_transaction.clone())
                        .await;
                    println!(
                        "{:?} executed (success={}) {:?}",
                        self.state.name,
                        x.is_ok(),
                        confirmation_transaction.certificate.digest()
                    );
                    if x.is_err() {
                        println!("{x:?}");
                    }
                    x
                }
            }
            Err(e) => Err(SuiError::from(e)),
        };
        */

        // MANY UNWRAP!!!
        let outcome = result
            .map(|x| {
                let message = sui_types::serialize::deserialize_message(&x[..]).unwrap();
                deserialize_transaction_info(message).unwrap()
            })
            .map_err(SuiError::from);

        // Notify the caller that the transaction has been sequenced (if there is a caller).
        let digest = Self::hash(&serialized);
        if let Some(repliers) = self.pending.remove(&digest) {
            for replier in repliers {
                if replier.send(outcome.clone()).is_err() {
                    debug!("No replier to listen to consensus output {digest}");
                }
            }
        }
    }

    /// Main loop receiving messages input to consensus and notifying the caller once the inputs
    /// are sequenced (of if an error happened).
    async fn run(&mut self) {
        loop {
            tokio::select! {
                // Keep track of this certificates so we can notify the user later.
                Some(consensus_input) = self.rx_consensus_input.recv() => {
                    let serialized = consensus_input.serialized;
                    let replier = consensus_input.replier;
                    let digest = Self::hash(&serialized);
                    self.pending.entry(digest).or_insert_with(Vec::new).push(replier);
                },

                // Execute sequenced transactions and notify the end-user (if any).
                Some(output) = self.rx_consensus_output.recv() => self
                    .process_consensus_output(output)
                    .await
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
