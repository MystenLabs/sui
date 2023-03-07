// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
mod errors;
mod state;
mod subscriber;

mod metrics;

pub use errors::{SubscriberError, SubscriberResult};
pub use state::ExecutionIndices;
use tracing::info;

use crate::metrics::ExecutorMetrics;
use async_trait::async_trait;
use config::{Committee, WorkerCache};
use crypto::PublicKey;

use prometheus::Registry;

use std::sync::Arc;
use storage::CertificateStore;

use crate::subscriber::spawn_subscriber;
use mockall::automock;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use types::{
    metered_channel, CertificateDigest, CommittedSubDag, ConditionalBroadcastReceiver,
    ConsensusOutput, ConsensusStore,
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
    async fn handle_consensus_output(&self, consensus_output: ConsensusOutput);

    /// Load the last executed sub-dag index from storage
    async fn last_executed_sub_dag_index(&self) -> u64;
}

/// A client subscribing to the consensus output and executing every transaction.
pub struct Executor;

impl Executor {
    /// Spawn a new client subscriber.
    pub fn spawn<State>(
        name: PublicKey,
        network: oneshot::Receiver<anemo::Network>,
        worker_cache: WorkerCache,
        committee: Committee,
        execution_state: State,
        shutdown_receivers: Vec<ConditionalBroadcastReceiver>,
        rx_sequence: metered_channel::Receiver<CommittedSubDag>,
        registry: &Registry,
        restored_consensus_output: Vec<CommittedSubDag>,
    ) -> SubscriberResult<Vec<JoinHandle<()>>>
    where
        State: ExecutionState + Send + Sync + 'static,
    {
        let metrics = ExecutorMetrics::new(registry);

        // We expect this will ultimately be needed in the `Core` as well as the `Subscriber`.
        let arc_metrics = Arc::new(metrics);

        // Spawn the subscriber.
        let subscriber_handle = spawn_subscriber(
            name,
            network,
            worker_cache,
            committee,
            shutdown_receivers,
            rx_sequence,
            arc_metrics,
            restored_consensus_output,
            execution_state,
        );

        // Return the handle.
        info!("Consensus subscriber successfully started");

        Ok(subscriber_handle)
    }
}

pub async fn get_restored_consensus_output<State: ExecutionState>(
    consensus_store: Arc<ConsensusStore>,
    certificate_store: CertificateStore,
    execution_state: &State,
) -> Result<Vec<CommittedSubDag>, SubscriberError> {
    // We always want to recover at least the last committed sub-dag since we can't know
    // whether the execution has been interrupted and there are still batches/transactions
    // that need to be sent for execution.

    let last_executed_sub_dag_index = execution_state.last_executed_sub_dag_index().await;

    let compressed_sub_dags =
        consensus_store.read_committed_sub_dags_from(&last_executed_sub_dag_index)?;

    let mut sub_dags = Vec::new();
    for compressed_sub_dag in compressed_sub_dags {
        let sub_dag_index = compressed_sub_dag.sub_dag_index;
        let certificate_digests: Vec<CertificateDigest> = compressed_sub_dag.certificates;

        let certificates = certificate_store
            .read_all(certificate_digests)?
            .into_iter()
            .flatten()
            .collect();

        let leader = certificate_store.read(compressed_sub_dag.leader)?.unwrap();

        sub_dags.push(CommittedSubDag {
            certificates,
            leader,
            sub_dag_index,
            reputation_score: compressed_sub_dag.reputation_score,
        });
    }

    Ok(sub_dags)
}

#[async_trait]
impl<T: ExecutionState + 'static + Send + Sync> ExecutionState for Arc<T> {
    async fn handle_consensus_output(&self, consensus_output: ConsensusOutput) {
        self.as_ref()
            .handle_consensus_output(consensus_output)
            .await
    }

    async fn last_executed_sub_dag_index(&self) -> u64 {
        self.as_ref().last_executed_sub_dag_index().await
    }
}
