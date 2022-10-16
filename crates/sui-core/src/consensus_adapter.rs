// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::CheckpointStore;
use crate::checkpoints::ConsensusSender;
use bytes::Bytes;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use multiaddr::Multiaddr;
use narwhal_types::TransactionProto;
use narwhal_types::TransactionsClient;
use parking_lot::Mutex;
use prometheus::register_int_gauge_with_registry;
use prometheus::IntCounter;
use prometheus::IntGauge;
use prometheus::Registry;
use prometheus::{register_histogram_with_registry, register_int_counter_with_registry, Histogram};
use std::collections::VecDeque;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
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

use tap::prelude::*;
use tokio::time::Instant;

use sui_types::base_types::AuthorityName;
use sui_types::messages::CertifiedTransaction;
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
    task::JoinHandle,
    time::{timeout, Duration},
};
use tracing::debug;
use tracing::error;

#[cfg(test)]
#[path = "unit_tests/consensus_tests.rs"]
pub mod consensus_tests;

/// A serialized consensus transaction.
type SerializedConsensusTransaction = Vec<u8>;

/// The digest of a consensus transactions.
type ConsensusTransactionDigest = u64;

/// Channel to notify the caller when the Sui certificate has been sequenced.
type TxSequencedNotifier = oneshot::Sender<SuiResult<()>>;
type TxSequencedNotifierClose = oneshot::Sender<()>;

const SEQUENCING_CERTIFICATE_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.1, 0.25, 0.5, 1., 2.5, 5., 7.5, 10., 12.5, 15., 20., 25., 30., 60., 90., 120., 180., 300.,
    600.,
];

pub struct ConsensusAdapterMetrics {
    // Certificate sequencing metrics
    pub sequencing_certificate_attempt: IntCounter,
    pub sequencing_certificate_success: IntCounter,
    pub sequencing_certificate_timeouts: IntCounter,
    pub sequencing_certificate_failures: IntCounter,
    pub sequencing_certificate_inflight: IntGauge,
    pub sequencing_certificate_latency: Histogram,

    // Fragment sequencing metrics
    pub sequencing_fragment_attempt: IntCounter,
    pub sequencing_fragment_success: IntCounter,
    pub sequencing_fragment_timeouts: IntCounter,
    pub sequencing_fragment_control_delay: IntGauge,
}

pub type OptArcConsensusAdapterMetrics = Option<Arc<ConsensusAdapterMetrics>>;

impl ConsensusAdapterMetrics {
    pub fn new(registry: &Registry) -> OptArcConsensusAdapterMetrics {
        Some(Arc::new(ConsensusAdapterMetrics {
            sequencing_certificate_attempt: register_int_counter_with_registry!(
                "sequencing_certificate_attempt",
                "Counts the number of certificates the validator attempts to sequence.",
                registry,
            )
            .unwrap(),
            sequencing_certificate_success: register_int_counter_with_registry!(
                "sequencing_certificate_success",
                "Counts the number of successfully sequenced certificates.",
                registry,
            )
            .unwrap(),
            sequencing_certificate_timeouts: register_int_counter_with_registry!(
                "sequencing_certificate_timeouts",
                "Counts the number of sequenced certificates that timed out.",
                registry,
            )
            .unwrap(),
            sequencing_certificate_failures: register_int_counter_with_registry!(
                "sequencing_certificate_failures",
                "Counts the number of sequenced certificates that failed other than by timeout.",
                registry,
            )
            .unwrap(),
            sequencing_certificate_inflight: register_int_gauge_with_registry!(
                "sequencing_certificate_inflight",
                "The inflight requests to sequence certificates.",
                registry,
            )
            .unwrap(),
            sequencing_certificate_latency: register_histogram_with_registry!(
                "sequencing_certificate_latency",
                "The latency for certificate sequencing.",
                SEQUENCING_CERTIFICATE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            sequencing_fragment_attempt: register_int_counter_with_registry!(
                "sequencing_fragment_attempt",
                "Counts the number of sequenced fragments submitted.",
                registry,
            )
            .unwrap(),
            sequencing_fragment_success: register_int_counter_with_registry!(
                "sequencing_fragment_success",
                "Counts the number of successfully sequenced fragments.",
                registry,
            )
            .unwrap(),
            sequencing_fragment_timeouts: register_int_counter_with_registry!(
                "sequencing_fragment_timeouts",
                "Counts the number of sequenced fragments that timed out.",
                registry,
            )
            .unwrap(),
            sequencing_fragment_control_delay: register_int_gauge_with_registry!(
                "sequencing_fragment_control_delay",
                "The estimated latency of sequencing fragments.",
                registry,
            )
            .unwrap(),
        }))
    }

    pub fn new_test() -> OptArcConsensusAdapterMetrics {
        None
    }
}

/// Message to notify the consensus listener that a new transaction has been sent to consensus
/// or that the caller timed out on a specific transaction.
#[derive(Debug)]
pub enum ConsensusListenerMessage {
    New(
        SerializedConsensusTransaction,
        (TxSequencedNotifier, TxSequencedNotifierClose),
    ),
    Processed(Vec<u8>),
}

pub struct ConsensusWaiter {
    // This channel is used to signal the result if the transaction gets
    // sequenced and observed at the output of consensus.
    signal_back: oneshot::Receiver<SuiResult<()>>,
    // We use this channel as a signalling mechanism, to detect if the ConsensusWaiter
    // struct is dropped, and to clean up the ConsensusListener structures to prevent
    // memory leaks.
    signal_close: oneshot::Receiver<()>,
}

impl ConsensusWaiter {
    pub fn new() -> (
        ConsensusWaiter,
        (TxSequencedNotifier, TxSequencedNotifierClose),
    ) {
        let (notif, signal_back) = oneshot::channel();
        let (close, signal_close) = oneshot::channel();
        (
            ConsensusWaiter {
                signal_back,
                signal_close,
            },
            (notif, close),
        )
    }

    pub fn close(&mut self) {
        self.signal_close.close();
    }

    pub async fn wait_for_result(self) -> SuiResult<()> {
        self.signal_back
            .await
            .map_err(|e| SuiError::FailedToHearBackFromConsensus(e.to_string()))?
    }
}

/// Submit Sui certificates to the consensus.
pub struct ConsensusAdapter {
    /// The network client connecting to the consensus node of this authority.
    consensus_client: TransactionsClient<sui_network::tonic::transport::Channel>,
    /// The Sui committee information.
    committee: Committee,
    /// A channel to notify the consensus listener to take action for a transaction.
    tx_consensus_listener: Sender<ConsensusListenerMessage>,
    /// Retries sending a transaction to consensus after this timeout.
    timeout: Duration,
    /// Number of submitted transactions still inflight at this node.
    num_inflight_transactions: AtomicU64,
    /// A structure to register metrics
    opt_metrics: OptArcConsensusAdapterMetrics,
}

impl ConsensusAdapter {
    /// Make a new Consensus adapter instance.
    pub fn new(
        consensus_address: Multiaddr,
        committee: Committee,
        tx_consensus_listener: Sender<ConsensusListenerMessage>,
        timeout: Duration,
        opt_metrics: OptArcConsensusAdapterMetrics,
    ) -> Self {
        let consensus_client = TransactionsClient::new(
            mysten_network::client::connect_lazy(&consensus_address)
                .expect("Failed to connect to consensus"),
        );
        let num_inflight_transactions = Default::default();
        Self {
            consensus_client,
            committee,
            tx_consensus_listener,
            timeout,
            num_inflight_transactions,
            opt_metrics,
        }
    }

    pub fn num_inflight_transactions(&self) -> u64 {
        self.num_inflight_transactions.load(Ordering::Relaxed)
    }

    /// Check if this authority should submit the transaction to consensus.
    fn should_submit(_certificate: &CertifiedTransaction) -> bool {
        // TODO [issue #1647]: Right now every authority submits the transaction to consensus.
        true
    }

    /// Submit a transaction to consensus, wait for its processing, and notify the caller.
    // Use .inspect when its stable.
    #[allow(clippy::option_map_unit_fn)]
    pub async fn submit(
        &self,
        authority: &AuthorityName,
        certificate: &CertifiedTransaction,
    ) -> SuiResult {
        // Check the Sui certificate (submitted by the user).
        certificate.verify(&self.committee)?;

        // Serialize the certificate in a way that is understandable to consensus (i.e., using
        // bincode) and it certificate to consensus.
        let transaction =
            ConsensusTransaction::new_certificate_message(authority, certificate.clone());
        let tracking_id = transaction.get_tracking_id();
        let tx_digest = certificate.digest();
        debug!(
            ?tracking_id,
            ?tx_digest,
            "Certified transaction consensus message created"
        );
        let serialized = bincode::serialize(&transaction)
            .expect("Serializing consensus transaction cannot fail");
        let bytes = Bytes::from(serialized.clone());

        // Notify the consensus listener that we are expecting to process this certificate.
        let (waiter, signals) = ConsensusWaiter::new();

        let consensus_input = ConsensusListenerMessage::New(serialized, signals);
        self.tx_consensus_listener
            .send(consensus_input)
            .await
            .expect("Failed to notify consensus listener");

        // Check if this authority submits the transaction to consensus.
        let now = Instant::now();
        let should_submit = Self::should_submit(certificate);
        if should_submit {
            self.consensus_client
                .clone()
                .submit_transaction(TransactionProto { transaction: bytes })
                .await
                .map_err(|e| SuiError::ConsensusConnectionBroken(format!("{:?}", e)))
                .tap_err(|r| {
                    error!("Submit transaction failed with: {:?}", r);
                })?;
            let inflight = self
                .num_inflight_transactions
                .fetch_add(1, Ordering::SeqCst);
            self.opt_metrics.as_ref().map(|metrics| {
                metrics.sequencing_certificate_attempt.inc();
                metrics.sequencing_certificate_inflight.set(inflight as i64);
            });
        }

        // TODO: make consensus guarantee delivery after submit_transaction() returns, and avoid the timeout below.
        let result = match timeout(self.timeout, waiter.wait_for_result()).await {
            Ok(Ok(_)) => {
                // Increment the attempted certificate sequencing success
                self.opt_metrics.as_ref().map(|metrics| {
                    metrics.sequencing_certificate_success.inc();
                });

                Ok(())
            }
            Ok(Err(e)) => {
                // Increment the attempted certificate sequencing failure
                self.opt_metrics.as_ref().map(|metrics| {
                    metrics.sequencing_certificate_failures.inc();
                });

                Err(e)
            }
            Err(e) => {
                // Increment the attempted certificate sequencing timeout
                self.opt_metrics.as_ref().map(|metrics| {
                    metrics.sequencing_certificate_timeouts.inc();
                });

                // We drop the waiter which will signal to the conensus listener task to clean up
                // the channels.
                Err(SuiError::FailedToHearBackFromConsensus(e.to_string()))
            }
        };

        if should_submit {
            let inflight = self
                .num_inflight_transactions
                .fetch_sub(1, Ordering::SeqCst);
            let elapsed_secs = now.elapsed().as_secs_f64();
            // Store the latest latency
            self.opt_metrics.as_ref().map(|metrics| {
                metrics.sequencing_certificate_inflight.set(inflight as i64);
                metrics.sequencing_certificate_latency.observe(elapsed_secs);
            });
        }

        result
    }
}

/// This module interfaces the consensus with Sui. It receives certificates input to consensus and
/// notify the called when they are sequenced.
pub struct ConsensusListener {
    /// Receive messages input to the consensus.
    rx_consensus_input: Receiver<ConsensusListenerMessage>,
    /// Keep a map of all consensus inputs that are currently being sequenced.
    /// Maximum size of the pending notifiers is bounded by the maximum pending transactions of the node.
    pending: HashMap<ConsensusTransactionDigest, Vec<(u64, TxSequencedNotifier)>>,
}

impl ConsensusListener {
    /// Spawn a new consensus adapter in a dedicated tokio task.
    pub fn spawn(rx_consensus_input: Receiver<ConsensusListenerMessage>) -> JoinHandle<()> {
        tokio::spawn(
            Self {
                rx_consensus_input,
                pending: HashMap::new(),
            }
            .run(),
        )
    }

    /// Hash serialized consensus transactions. We do not need specific cryptographic properties except
    /// only collision resistance.
    pub fn hash_serialized_transaction(
        serialized: &SerializedConsensusTransaction,
    ) -> ConsensusTransactionDigest {
        let mut hasher = DefaultHasher::new();
        let len = serialized.len();
        if len > 8 {
            // The first 8 bytes are the tracking id, and we don't want to hash that so that
            // certificates submitted by different validators are considered the same message.
            (serialized[8..]).hash(&mut hasher);
        } else {
            // If somehow the length is <= 8 (which is invalid), we just don't care and hash
            // the whole thing.
            serialized.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Main loop receiving messages input to consensus and notifying the caller once the inputs
    /// are sequenced (or if an error happened).
    async fn run(mut self) {
        let mut closed_notifications = FuturesUnordered::new();
        let mut id_counter: u64 = 0;

        loop {
            tokio::select! {
                // A new transaction has been sent to consensus or is no longer needed.
                Some(message) = self.rx_consensus_input.recv() => {
                    match message {
                        // Keep track of this certificates so we can notify the user later.
                        ConsensusListenerMessage::New(transaction, (replier, mut _closer)) => {
                            let digest = Self::hash_serialized_transaction(&transaction);
                            let id = id_counter;
                            id_counter += 1;

                            let list = self.pending.entry(digest).or_insert_with(Vec::new);
                            list.push((id, replier));

                            // Register with the close notification.
                            closed_notifications.push(async move {
                                // Wait for the channel to close
                                _closer.closed().await;
                                // Return he digest concerned
                                (digest, id)
                            });
                        },
                        ConsensusListenerMessage::Processed(serialized) => {
                            let digest = Self::hash_serialized_transaction(&serialized);
                            if let Some(repliers) = self.pending.remove(&digest) {
                                for (_, replier) in repliers {
                                    if replier.send(Ok(())).is_err() {
                                        debug!("No replier to listen to consensus output {digest}");
                                    }
                                }
                            }
                        }
                    }
                },

                Some((digest, id)) = closed_notifications.next() => {
                    let should_delete = if let Some(list) = self.pending.get_mut(&digest) {
                        // First clean up the list
                        list.retain(|(item_id, _)| *item_id != id);
                        // if the resuting list is empty we should delete the entry.
                        list.is_empty()
                    } else { false };

                    // Secondly we determine if we need to delete the entry
                    if should_delete {
                        self.pending.remove(&digest);
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

fn weighted_average_half(old_average: u64, new_value: u64) -> u64 {
    (500 * old_average + 500 * new_value) / 1000
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
    checkpoint_db: Arc<Mutex<CheckpointStore>>,
    /// The initial delay to wait before re-attempting a connection with consensus (in ms).
    retry_delay: Duration,
    /// The maximum number of checkpoint fragment pending sequencing.
    max_pending_transactions: usize,
    /// Keep all checkpoint fragment waiting to be sequenced.
    buffer: VecDeque<(SerializedConsensusTransaction, CheckpointSequenceNumber)>,

    /// A structure to register metrics
    opt_metrics: OptArcConsensusAdapterMetrics,
}

impl CheckpointConsensusAdapter {
    /// Create a new `CheckpointConsensusAdapter`.
    pub fn new(
        consensus_address: Multiaddr,
        tx_consensus_listener: Sender<ConsensusListenerMessage>,
        rx_checkpoint_consensus_adapter: Receiver<CheckpointFragment>,
        checkpoint_db: Arc<Mutex<CheckpointStore>>,
        retry_delay: Duration,
        max_pending_transactions: usize,
        opt_metrics: OptArcConsensusAdapterMetrics,
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
            checkpoint_db,
            retry_delay,
            max_pending_transactions,
            buffer: VecDeque::with_capacity(max_pending_transactions),
            opt_metrics,
        }
    }

    /// Spawn a `CheckpointConsensusAdapter` in a dedicated tokio task.
    pub fn spawn(mut self) -> JoinHandle<()> {
        tokio::spawn(async move { self.run().await })
    }

    /// Submit a transaction to consensus.
    // Use .inspect when its stable.
    #[allow(clippy::option_map_unit_fn)]
    async fn submit(&self, serialized: SerializedConsensusTransaction) -> SuiResult {
        let transaction = Bytes::from(serialized);
        let proto_transaction = TransactionProto { transaction };

        // Increment the attempted fragment sequencing failure
        self.opt_metrics.as_ref().map(|metrics| {
            metrics.sequencing_fragment_attempt.inc();
        });

        self.consensus_client
            .clone()
            .submit_transaction(proto_transaction)
            .await
            .map_err(|e| SuiError::ConsensusConnectionBroken(format!("{:?}", e)))
            .map(|_| ())
    }

    /// Wait for a transaction to be sequenced by consensus (or to timeout).
    async fn waiter<T>(
        receiver: ConsensusWaiter,
        retry_delay: Duration,
        deliver: T,
    ) -> (SuiResult<()>, u64, T) {
        let now = Instant::now();
        let outcome = match timeout(retry_delay, receiver.wait_for_result()).await {
            Ok(reply) => reply,
            Err(e) => Err(SuiError::FailedToHearBackFromConsensus(e.to_string())),
        };
        let conensus_latency = now.elapsed().as_millis() as u64;
        (outcome, conensus_latency, deliver)
    }

    /// Main loop receiving checkpoint fragments to reliably submit to consensus.
    // Use .inspect when its stable.
    #[allow(clippy::option_map_unit_fn)]
    async fn run(&mut self) {
        let mut waiting = FuturesUnordered::new();

        // Fragment sequencing latency estimation
        let mut latency_estimate = self.retry_delay.as_millis() as u64;
        let max_latency = latency_estimate * 100;

        // Continuously listen to checkpoint fragments and re-attempt sequencing if needed.
        loop {
            // Try to submit all pending checkpoint fragments to consensus.
            while let Some((serialized, sequence_number)) = self.buffer.pop_back() {
                match self.submit(serialized.clone()).await {
                    Ok(_) => {
                        // Notify the consensus listener that we wish to be notified once our
                        // consensus transaction is sequenced.
                        let (waiter, signals) = ConsensusWaiter::new();

                        let consensus_input =
                            ConsensusListenerMessage::New(serialized.clone(), signals);

                        // Add the receiver to the waiter. So we can retransmit if the
                        // connection fails.
                        let deliver = (serialized, sequence_number);
                        let timeout_delay =
                            Duration::from_millis(latency_estimate) + self.retry_delay;
                        let future = Self::waiter(waiter, timeout_delay, deliver);
                        waiting.push(future);

                        // Finally sent to consensus, after registering to avoid a race condition
                        self.tx_consensus_listener
                            .send(consensus_input)
                            .await
                            .expect("Failed to notify consensus listener");
                    }
                    Err(e) => {
                        error!("Checkpoint fragment submit failed: {:?}", e);
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
                        if let Some(proposal) = &self.checkpoint_db.lock().get_locals().current_proposal {
                            let current_sequence_number = proposal.sequence_number();
                            self.buffer.retain(|(_, s)| s >= current_sequence_number);
                        }
                    }

                    // Add the fragment to the buffer.
                    let cp_seq = *fragment.proposer_sequence_number();
                    let proposer = fragment.proposer.auth_signature.authority;
                    let other = fragment.other.auth_signature.authority;
                    let transaction = ConsensusTransaction::new_checkpoint_message(fragment);
                    let tracking_id = transaction.get_tracking_id();
                    let serialized = bincode::serialize(&transaction).expect("Serialize consensus transaction cannot fail");
                    debug!(
                        ?tracking_id,
                        ?cp_seq,
                        size=?serialized.len(),
                        "Checkpoint fragment consensus message created. Proposer: {}, Other: {}",
                        proposer,
                        other,
                    );
                    self.buffer.push_front((serialized, sequence_number));
                },

                // Listen to checkpoint fragments who failed to be sequenced and need retries.
                Some((outcome, latency_ms, identifier)) = waiting.next() => {

                    // Update the latency estimate using a weigted average
                    // But also cap it upwards by max_latency
                    latency_estimate = max_latency.min(weighted_average_half(latency_estimate, latency_ms));

                    // Record the latest consensus latency estimate for fragments
                    self.opt_metrics.as_ref().map(|metrics| {
                        metrics.sequencing_fragment_control_delay.set(latency_estimate as i64);
                    });

                   if let Err(error) = outcome {
                       tracing::warn!("Failed to sequence checkpoint fragment, and re-submitting fragment: {error}");
                       let (serialized_transaction, checkpoint_sequence_number) = identifier;

                            // Increment the attempted fragment sequencing failure
                            self.opt_metrics.as_ref().map(|metrics| {
                                metrics.sequencing_fragment_timeouts.inc();
                            });

                       self.buffer.push_back((serialized_transaction, checkpoint_sequence_number));
                   } else {
                            // Increment the attempted fragment sequencing success
                            self.opt_metrics.as_ref().map(|metrics| {
                                metrics.sequencing_fragment_success.inc();
                            });
                   }
                },
            }
        }
    }
}
