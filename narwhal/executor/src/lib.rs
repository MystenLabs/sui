// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
mod errors;
mod state;
mod subscriber;

mod metrics;

pub use errors::{SubscriberError, SubscriberResult};
pub use state::ExecutionIndices;
use sui_protocol_config::ProtocolConfig;

use crate::metrics::ExecutorMetrics;
use crate::subscriber::spawn_subscriber;

use async_trait::async_trait;
use config::{AuthorityIdentifier, Committee, WorkerCache};
use mockall::automock;
use mysten_metrics::metered_channel;
use network::client::NetworkClient;
use prometheus::Registry;
use std::sync::Arc;
use storage::{CertificateStore, ConsensusStore};
use tokio::task::JoinHandle;
use tracing::info;
use types::{CertificateDigest, CommittedSubDag, ConditionalBroadcastReceiver, ConsensusOutput};

/// Convenience type representing a serialized transaction.
pub type SerializedTransaction = Vec<u8>;

/// Convenience type representing a serialized transaction digest.
pub type SerializedTransactionDigest = u64;

#[automock]
#[async_trait]
pub trait ExecutionState {
    /// Execute the transaction and atomically persist the consensus index.
    async fn handle_consensus_output(&mut self, consensus_output: ConsensusOutput);

    /// The last executed sub-dag / commit leader round.
    fn last_executed_sub_dag_round(&self) -> u64;

    /// The last executed sub-dag / commit index.
    fn last_executed_sub_dag_index(&self) -> u64;
}

/// A client subscribing to the consensus output and executing every transaction.
pub struct Executor;

impl Executor {
    /// Spawn a new client subscriber.
    pub fn spawn<State>(
        authority_id: AuthorityIdentifier,
        worker_cache: WorkerCache,
        committee: Committee,
        protocol_config: &ProtocolConfig,
        client: NetworkClient,
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

        // This will be needed in the `Subscriber`.
        let arc_metrics = Arc::new(metrics);

        // Spawn the subscriber.
        let subscriber_handles = spawn_subscriber(
            authority_id,
            worker_cache,
            committee,
            protocol_config.clone(),
            client,
            shutdown_receivers,
            rx_sequence,
            arc_metrics,
            restored_consensus_output,
            execution_state,
        );

        // Return the handle.
        info!("Consensus subscriber successfully started");

        Ok(subscriber_handles)
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

    let last_executed_sub_dag_index = execution_state.last_executed_sub_dag_index();

    let compressed_sub_dags =
        consensus_store.read_committed_sub_dags_from(&last_executed_sub_dag_index)?;

    let mut sub_dags = Vec::new();
    for compressed_sub_dag in compressed_sub_dags {
        let certificate_digests: Vec<CertificateDigest> = compressed_sub_dag.certificates();

        let certificates = certificate_store
            .read_all(certificate_digests)?
            .into_iter()
            .flatten()
            .collect();

        let leader = certificate_store
            .read(compressed_sub_dag.leader())?
            .unwrap();

        sub_dags.push(CommittedSubDag::from_commit(
            compressed_sub_dag,
            certificates,
            leader,
        ));
    }

    Ok(sub_dags)
}
