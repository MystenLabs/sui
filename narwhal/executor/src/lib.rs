// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
mod batch_loader;
mod core;
mod errors;
mod state;
mod subscriber;

#[cfg(test)]
#[path = "tests/fixtures.rs"]
mod fixtures;

#[cfg(test)]
#[path = "tests/execution_state.rs"]
mod execution_state;

#[cfg(test)]
#[path = "tests/sequencer.rs"]
mod sequencer;

pub use errors::{ExecutionStateError, SubscriberError, SubscriberResult};
pub use state::ExecutionIndices;

use crate::{batch_loader::BatchLoader, core::Core, subscriber::Subscriber};
use async_trait::async_trait;
use config::{Committee, SharedCommittee};
use consensus::{ConsensusOutput, ConsensusSyncRequest};
use crypto::traits::VerifyingKey;
use serde::de::DeserializeOwned;
use std::{fmt::Debug, sync::Arc};
use store::Store;
use tokio::{
    sync::{
        mpsc::{channel, Receiver, Sender},
        watch,
    },
    task::JoinHandle,
};
use tracing::info;
use types::{BatchDigest, ReconfigureNotification, SerializedBatchMessage};

/// Default inter-task channel size.
pub const DEFAULT_CHANNEL_SIZE: usize = 1_000;

/// Convenience type representing a serialized transaction.
pub type SerializedTransaction = Vec<u8>;

/// Convenience type representing a serialized transaction digest.
pub type SerializedTransactionDigest = u64;

#[async_trait]
pub trait ExecutionState {
    /// The type of the transaction to process.
    type Transaction: DeserializeOwned + Send + Debug;

    /// The error type to return in case something went wrong during execution.
    type Error: ExecutionStateError;

    /// The execution outcome to output.
    type Outcome;

    /// Execute the transaction and atomically persist the consensus index. This function
    /// returns an execution outcome that will be output by the executor channel. It may
    /// also return a new committee to reconfigure the system.
    async fn handle_consensus_transaction<PublicKey: VerifyingKey>(
        &self,
        consensus_output: &ConsensusOutput<PublicKey>,
        execution_indices: ExecutionIndices,
        transaction: Self::Transaction,
    ) -> Result<(Self::Outcome, Option<Committee<PublicKey>>), Self::Error>;

    /// Simple guardrail ensuring there is a single instance using the state
    /// to call `handle_consensus_transaction`. Many instances may read the state,
    /// or use it for other purposes.
    fn ask_consensus_write_lock(&self) -> bool;

    /// Tell the state that the caller instance is no longer using calling
    //// `handle_consensus_transaction`.
    fn release_consensus_write_lock(&self);

    /// Load the last consensus index from storage.
    async fn load_execution_indices(&self) -> Result<ExecutionIndices, Self::Error>;
}

/// The output of the executor.
pub type ExecutorOutput<State> = (
    SubscriberResult<<State as ExecutionState>::Outcome>,
    SerializedTransaction,
);

/// A client subscribing to the consensus output and executing every transaction.
pub struct Executor;

impl Executor {
    /// Spawn a new client subscriber.
    pub async fn spawn<State, PublicKey>(
        name: PublicKey,
        committee: SharedCommittee<PublicKey>,
        store: Store<BatchDigest, SerializedBatchMessage>,
        execution_state: Arc<State>,
        tx_reconfigure: &watch::Sender<ReconfigureNotification<PublicKey>>,
        rx_consensus: Receiver<ConsensusOutput<PublicKey>>,
        tx_consensus: Sender<ConsensusSyncRequest>,
        tx_output: Sender<ExecutorOutput<State>>,
    ) -> SubscriberResult<Vec<JoinHandle<()>>>
    where
        State: ExecutionState + Send + Sync + 'static,
        State::Outcome: Send + 'static,
        State::Error: Debug,
        PublicKey: VerifyingKey,
    {
        let (tx_batch_loader, rx_batch_loader) = channel(DEFAULT_CHANNEL_SIZE);
        let (tx_executor, rx_executor) = channel(DEFAULT_CHANNEL_SIZE);

        // Ensure there is a single consensus client modifying the execution state.
        ensure!(
            execution_state.ask_consensus_write_lock(),
            SubscriberError::OnlyOneConsensusClientPermitted
        );

        // Load the subscriber state from storage.
        let execution_indices = execution_state.load_execution_indices().await?;
        let next_consensus_index = execution_indices.next_certificate_index;

        // Spawn the subscriber.
        let subscriber_handle = Subscriber::<PublicKey>::spawn(
            store.clone(),
            tx_reconfigure.subscribe(),
            rx_consensus,
            tx_consensus,
            tx_batch_loader,
            tx_executor,
            next_consensus_index,
        );

        // Spawn the executor's core.
        let executor_handle = Core::<State, PublicKey>::spawn(
            store.clone(),
            execution_state,
            tx_reconfigure.subscribe(),
            /* rx_subscriber */ rx_executor,
            tx_output,
        );

        // Spawn the batch loader.
        let worker_addresses = committee
            .load()
            .authorities
            .iter()
            .find(|(x, _)| *x == &name)
            .map(|(_, authority)| authority)
            .expect("Our public key is not in the committee")
            .workers
            .iter()
            .map(|(id, x)| (*id, x.worker_to_worker.clone()))
            .collect();
        let batch_loader_handle = BatchLoader::spawn(
            store,
            tx_reconfigure.subscribe(),
            rx_batch_loader,
            worker_addresses,
        );

        // Return the handle.
        info!("Consensus subscriber successfully started");
        Ok(vec![
            subscriber_handle,
            executor_handle,
            batch_loader_handle,
        ])
    }
}
