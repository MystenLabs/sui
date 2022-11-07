// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
mod errors;
mod state;
mod subscriber;

mod metrics;
mod notifier;

pub use errors::{SubscriberError, SubscriberResult};
pub use state::ExecutionIndices;
use tracing::{debug, info};

use crate::metrics::ExecutorMetrics;
use crate::notifier::Notifier;
use async_trait::async_trait;
use config::{Committee, SharedWorkerCache};
use consensus::ConsensusOutput;
use crypto::PublicKey;
use network::P2pNetwork;

use prometheus::Registry;

use std::sync::Arc;
use storage::CertificateStore;

use crate::subscriber::spawn_subscriber;
use mockall::automock;
use tokio::sync::oneshot;
use tokio::{sync::watch, task::JoinHandle};
use types::{
    metered_channel, CertificateDigest, ConsensusStore, ReconfigureNotification, SequenceNumber,
};

/// Convenience type representing a serialized transaction.
pub type SerializedTransaction = Vec<u8>;

/// Convenience type representing a serialized transaction digest.
pub type SerializedTransactionDigest = u64;

#[automock]
#[async_trait]
pub trait ExecutionState {
    /// Execute the transaction and atomically persist the consensus index.
    async fn handle_consensus_transaction(
        &self,
        consensus_output: &Arc<ConsensusOutput>,
        execution_indices: ExecutionIndices,
        transaction: Vec<u8>,
    );

    /// Notifies executor that narwhal commit boundary was reached
    /// Consumers can use this boundary as an approximate signal that it might take some
    /// time before more transactions will arrive
    /// Consumers can use this boundary, for example, to form checkpoints
    ///
    /// Current implementation sends this notification at the end of narwhal certificate
    ///
    /// In the future this will be triggered on the actual commit boundary, once per narwhal commit
    async fn notify_commit_boundary(&self, _consensus_output: &Arc<ConsensusOutput>) {}

    /// Load the last consensus index from storage.
    async fn load_execution_indices(&self) -> ExecutionIndices;
}

/// A client subscribing to the consensus output and executing every transaction.
pub struct Executor;

impl Executor {
    /// Spawn a new client subscriber.
    pub fn spawn<State>(
        name: PublicKey,
        network: oneshot::Receiver<P2pNetwork>,
        worker_cache: SharedWorkerCache,
        committee: Committee,
        execution_state: State,
        tx_reconfigure: &watch::Sender<ReconfigureNotification>,
        rx_sequence: metered_channel::Receiver<ConsensusOutput>,
        registry: &Registry,
        restored_consensus_output: Vec<ConsensusOutput>,
    ) -> SubscriberResult<Vec<JoinHandle<()>>>
    where
        State: ExecutionState + Send + Sync + 'static,
    {
        let metrics = ExecutorMetrics::new(registry);

        let (tx_notifier, rx_notifier) =
            metered_channel::channel(primary::CHANNEL_CAPACITY, &metrics.tx_notifier);

        // We expect this will ultimately be needed in the `Core` as well as the `Subscriber`.
        let arc_metrics = Arc::new(metrics);

        // Spawn the subscriber.
        let subscriber_handle = spawn_subscriber(
            name,
            network,
            worker_cache,
            committee,
            tx_reconfigure.subscribe(),
            rx_sequence,
            tx_notifier,
            arc_metrics.clone(),
            restored_consensus_output,
        );

        let notifier_handler = Notifier::spawn(rx_notifier, execution_state, arc_metrics);

        // Return the handle.
        info!("Consensus subscriber successfully started");

        Ok(vec![subscriber_handle, notifier_handler])
    }
}

pub async fn get_restored_consensus_output<State: ExecutionState>(
    consensus_store: Arc<ConsensusStore>,
    certificate_store: CertificateStore,
    execution_state: &State,
) -> Result<Vec<ConsensusOutput>, SubscriberError> {
    let mut restored_consensus_output = Vec::new();
    let consensus_next_index = consensus_store
        .read_last_consensus_index()
        .map_err(SubscriberError::StoreError)?;

    // Execution state always keeps the index of the latest certificate that has been executed.
    // However, in consensus_store the committed certificates are stored alongside the "next" index
    // that should be assigned to a certificate. Thus, to successfully recover the un-executed
    // certificates from the consensus_store we are incrementing the `next_certificate_index` by 1
    // to align the semantics.
    // TODO: https://github.com/MystenLabs/sui/issues/5819
    let last = execution_state.load_execution_indices().await;
    let last_executed_index = last.next_certificate_index + 1;

    debug!(
        "Recovering with last executor index:{:?}, last consensus index:{}",
        last, consensus_next_index
    );

    // We always want to recover at least the last committed certificate since we can't know
    // whether the execution has been interrupted and there are still batches/transactions
    // that need to be send for execution.
    let missing = consensus_store
        .read_sequenced_certificates_from(&last_executed_index)?
        .into_iter()
        .map(|(seq, digest)| (seq - 1, digest))
        .collect::<Vec<(SequenceNumber, CertificateDigest)>>();

    debug!("Found {} certificates to recover", missing.len());

    for (seq, cert_digest) in missing {
        debug!(
            "Recovered certificate index:{}, digest:{}",
            seq, cert_digest
        );
        if let Some(cert) = certificate_store.read(cert_digest).unwrap() {
            // Save the missing sequence / cert pair as ConsensusOutput to re-send to the executor.
            restored_consensus_output.push(ConsensusOutput {
                certificate: cert,
                consensus_index: seq,
            })
        }
    }
    Ok(restored_consensus_output)
}

#[async_trait]
impl<T: ExecutionState + 'static + Send + Sync> ExecutionState for Arc<T> {
    async fn handle_consensus_transaction(
        &self,
        consensus_output: &Arc<ConsensusOutput>,
        execution_indices: ExecutionIndices,
        transaction: Vec<u8>,
    ) {
        self.as_ref()
            .handle_consensus_transaction(consensus_output, execution_indices, transaction)
            .await
    }

    async fn load_execution_indices(&self) -> ExecutionIndices {
        self.as_ref().load_execution_indices().await
    }
}
