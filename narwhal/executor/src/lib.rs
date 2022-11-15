// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
mod errors;
mod state;
mod subscriber;

mod metrics;
mod notifier;

pub use errors::{SubscriberError, SubscriberResult};
pub use state::ExecutionIndices;
use tracing::info;

use crate::metrics::ExecutorMetrics;
use crate::notifier::Notifier;
use async_trait::async_trait;
use config::{Committee, SharedWorkerCache};
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
    metered_channel, CommittedSubDag, ConsensusOutput, ConsensusStore, ReconfigureNotification,
};

/// Convenience type representing a serialized transaction.
pub type SerializedTransaction = Vec<u8>;

/// Convenience type representing a serialized transaction digest.
pub type SerializedTransactionDigest = u64;

#[automock]
#[async_trait]
// Important - if you add method with the default implementation here make sure to update impl ExecutionState for Arc<T>
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
    async fn notify_commit_boundary(&self, _committed_dag: &Arc<CommittedSubDag>) {}

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
        rx_sequence: metered_channel::Receiver<CommittedSubDag>,
        registry: &Registry,
        restored_consensus_output: Vec<CommittedSubDag>,
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
) -> Result<Vec<CommittedSubDag>, SubscriberError> {
    // We always want to recover at least the last committed certificate since we can't know
    // whether the execution has been interrupted and there are still batches/transactions
    // that need to be send for execution.

    let last_committed_leader = execution_state
        .load_execution_indices()
        .await
        .last_committed_round;

    let compressed_sub_dags =
        consensus_store.read_committed_sub_dags_from(&last_committed_leader)?;

    let mut sub_dags = Vec::new();
    for compressed_sub_dag in compressed_sub_dags {
        let (certificate_digests, consensus_indices): (Vec<_>, Vec<_>) =
            compressed_sub_dag.certificates.into_iter().unzip();

        let certificates = certificate_store
            .read_all(certificate_digests)?
            .into_iter()
            .flatten();

        let outputs = certificates
            .into_iter()
            .zip(consensus_indices.into_iter())
            .map(|(certificate, consensus_index)| ConsensusOutput {
                certificate,
                consensus_index,
            })
            .collect();

        let leader = certificate_store.read(compressed_sub_dag.leader)?.unwrap();

        sub_dags.push(CommittedSubDag {
            certificates: outputs,
            leader,
        });
    }

    Ok(sub_dags)
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

    async fn notify_commit_boundary(&self, committed_dag: &Arc<CommittedSubDag>) {
        self.as_ref().notify_commit_boundary(committed_dag).await
    }

    async fn load_execution_indices(&self) -> ExecutionIndices {
        self.as_ref().load_execution_indices().await
    }
}
