// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    bail,
    errors::{SubscriberError, SubscriberResult},
    state::ExecutionIndices,
    BatchExecutionState, ExecutionState, ExecutorOutput, SerializedTransaction,
    SingleExecutionState,
};
use async_trait::async_trait;
use consensus::ConsensusOutput;
use futures::lock::Mutex;
use std::{fmt::Debug, sync::Arc};
use store::Store;
use tokio::{
    sync::{mpsc::Sender, watch},
    task::JoinHandle,
};
use tracing::debug;
use types::{metered_channel, Batch, BatchDigest, ReconfigureNotification, SequenceNumber};

#[cfg(test)]
#[path = "tests/executor_tests.rs"]
pub mod executor_tests;

/// Use the execution state to execute transactions. This module expects to receive a sequence of
/// consensus messages in the right and complete order. All transactions data referenced by the
/// certificate should already be downloaded in the temporary storage. This module ensures it does
/// not processes twice the same transaction (despite crash-recovery).
pub struct Core<State: ExecutionState> {
    /// The temporary storage holding all transactions' data (that may be too big to hold in memory).
    store: Store<BatchDigest, Batch>,
    /// The (global) state to perform execution.
    execution_state: Arc<State>,
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    /// Receive ordered consensus output to execute.
    rx_subscriber: metered_channel::Receiver<ConsensusOutput>,
}

impl<State: ExecutionState> Drop for Core<State> {
    fn drop(&mut self) {
        self.execution_state.release_consensus_write_lock();
    }
}

impl<State> Core<State>
where
    State: BatchExecutionState + Send + Sync + 'static,
    State::Error: Debug,
{
    /// Spawn a new executor in a dedicated tokio task.
    #[must_use]
    pub fn spawn(
        store: Store<BatchDigest, Batch>,
        execution_state: Arc<State>,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        rx_subscriber: metered_channel::Receiver<ConsensusOutput>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                store,
                execution_state,
                rx_reconfigure,
                rx_subscriber,
            }
            .run()
            .await
            .expect("Failed to run core")
        })
    }

    /// Main loop listening to new certificates and execute them.
    async fn run(&mut self) -> SubscriberResult<()> {
        let _next_certificate_index = self
            .execution_state
            .load_next_certificate_index()
            .await
            .expect("Failed to load execution indices from store");

        // TODO: Replay certificates from the store.

        loop {
            tokio::select! {
                // Execute all transactions associated with the consensus output message.
                Some(message) = self.rx_subscriber.recv() => {
                    // This function persists the necessary data to enable crash-recovery.
                    self.execute_certificate(&message).await?;

                    // Cleanup the temporary persistent storage.
                    // TODO [issue #191]: Security cleanup the store.
                },

                // Check whether the committee changed.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    if let ReconfigureNotification::Shutdown = message {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Execute a single certificate.
    async fn execute_certificate(&mut self, message: &ConsensusOutput) -> SubscriberResult<()> {
        // Collect all transactions in all the batches.
        let mut batches = Vec::new();

        for batch_digest in message.certificate.header.payload.keys() {
            batches.push(self.collect_batch(batch_digest).await?);
        }

        let result = self
            .execution_state
            .handle_consensus(message, batches)
            .await
            .map_err(SubscriberError::from);

        match result {
            Ok(()) => Ok(()),
            Err(error @ SubscriberError::ClientExecutionError(_)) => {
                // We may want to log the errors that are the user's fault (i.e., that are neither
                // our fault or the fault of consensus) for debug purposes. It is safe to continue
                // by ignoring those transactions since all honest subscribers will do the same.
                debug!("{error}");
                Ok(())
            }
            Err(error) => {
                bail!(error)
            }
        }
    }

    /// Collect all transactions in a batch
    async fn collect_batch(
        &mut self,
        batch_digest: &BatchDigest,
    ) -> SubscriberResult<Vec<SerializedTransaction>> {
        // The store should now hold all transaction data referenced by the input certificate.
        let transactions = match self.store.read(*batch_digest).await? {
            Some(x) => x.0,
            None => {
                // If two certificates contain the exact same batch (eg. by the actions of a Byzantine
                // consensus node), some correct client may already have deleted the batch from their
                // temporary storage while others may not. This is not a problem, we can simply ignore
                // the second batch since there is no point in executing twice the same transactions
                // (as the second execution attempt will always fail).
                debug!("Duplicate batch {batch_digest}");
                return Ok(Vec::new());
            }
        };

        Ok(transactions)
    }
}

/// Executor that feeds transactions one by one to the execution state.
pub struct SingleExecutor<State>
where
    State: SingleExecutionState,
{
    /// The (global) state to perform execution.
    execution_state: Arc<State>,
    /// The indices ensuring we do not execute twice the same transaction.
    execution_indices: Mutex<ExecutionIndices>,
    /// Outputs executed transactions.
    tx_output: Sender<ExecutorOutput<State>>,
}

#[async_trait]
impl<State> ExecutionState for SingleExecutor<State>
where
    State: SingleExecutionState + Sync + Send + 'static,
    State::Outcome: Sync + Send + 'static,
    State::Error: Sync + Send + 'static,
{
    type Error = State::Error;

    fn ask_consensus_write_lock(&self) -> bool {
        self.execution_state.ask_consensus_write_lock()
    }

    fn release_consensus_write_lock(&self) {
        self.execution_state.release_consensus_write_lock()
    }
}

#[async_trait]
impl<State> BatchExecutionState for SingleExecutor<State>
where
    State: SingleExecutionState + Sync + Send + 'static,
    State::Outcome: Sync + Send + 'static,
    State::Error: Clone + Sync + Send + 'static,
{
    async fn load_next_certificate_index(&self) -> Result<SequenceNumber, Self::Error> {
        let indices = self.execution_state.load_execution_indices().await?;
        let mut execution_indices = self.execution_indices.lock().await;
        *execution_indices = indices;
        Ok(execution_indices.next_certificate_index)
    }

    async fn handle_consensus(
        &self,
        consensus_output: &ConsensusOutput,
        transaction_batches: Vec<Vec<SerializedTransaction>>,
    ) -> Result<(), Self::Error> {
        let mut execution_indices = self.execution_indices.lock().await;

        if transaction_batches.is_empty() {
            execution_indices.skip_certificate();
        } else {
            // Execute every batch in the certificate.
            let total_batches = transaction_batches.len();
            for (index, batch) in transaction_batches.into_iter().enumerate() {
                // Skip batches that we already executed (after crash-recovery).
                if execution_indices.check_next_batch_index(index as SequenceNumber) {
                    self.execute_batch(
                        &mut execution_indices,
                        consensus_output,
                        batch,
                        total_batches,
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }
}

impl<State> SingleExecutor<State>
where
    State: SingleExecutionState,
    State::Error: Clone,
    State::Outcome: Sync + Send + 'static,
{
    pub fn new(execution_state: Arc<State>, tx_output: Sender<ExecutorOutput<State>>) -> Arc<Self> {
        Arc::new(Self {
            execution_state,
            execution_indices: Mutex::new(ExecutionIndices::default()),
            tx_output,
        })
    }

    /// Execute a single batch of transactions.
    async fn execute_batch(
        &self,
        execution_indices: &mut ExecutionIndices,
        consensus_output: &ConsensusOutput,
        transactions: Vec<SerializedTransaction>,
        total_batches: usize,
    ) -> Result<(), State::Error> {
        if transactions.is_empty() {
            execution_indices.skip_batch(total_batches);
            return Ok(());
        }

        // Execute every transaction in the batch.
        let total_transactions = transactions.len();
        for (index, transaction) in transactions.into_iter().enumerate() {
            // Skip transactions that we already executed (after crash-recovery).
            if execution_indices.check_next_transaction_index(index as SequenceNumber) {
                // Execute the transaction
                self.execute_transaction(
                    execution_indices,
                    consensus_output,
                    transaction,
                    total_batches,
                    total_transactions,
                )
                .await?;
            }
        }
        Ok(())
    }

    /// Execute a single transaction.
    async fn execute_transaction(
        &self,
        execution_indices: &mut ExecutionIndices,
        consensus_output: &ConsensusOutput,
        serialized: SerializedTransaction,
        total_batches: usize,
        total_transactions: usize,
    ) -> Result<(), State::Error> {
        // Compute the next expected indices. Those will be persisted upon transaction execution
        // and are only used for crash-recovery.
        execution_indices.next(total_batches, total_transactions);

        // The consensus simply orders bytes, so we first need to deserialize the transaction.
        // If the deserialization fail it is safe to ignore the transaction since all correct
        // clients will do the same. Remember that a bad authority or client may input random
        // bytes to the consensus.
        let (result, outcome) = match bincode::deserialize::<State::Transaction>(&serialized) {
            Err(e) => {
                let error = SubscriberError::ClientExecutionError(format!(
                    "Failed to deserialize transaction: {e}"
                ));
                // There is always a chance that the fault lies with our deserialization.
                debug!("{error}");
                (Ok(()), Err(error))
            }
            Ok(transaction) => {
                // Execute the transaction. Note that the executor will need to choose whether to discard
                // transactions from previous epochs by itself.
                let result = self
                    .execution_state
                    .handle_consensus_transaction(
                        consensus_output,
                        execution_indices.clone(),
                        transaction,
                    )
                    .await;

                match result {
                    Ok(outcome) => (Ok(()), Ok(outcome)),
                    Err(error) => match SubscriberError::from(error.clone()) {
                        // We may want to log the errors that are the user's fault (i.e., that are neither
                        // our fault or the fault of consensus) for debug purposes. It is safe to continue
                        // by ignoring those transactions since all honest subscribers will do the same.
                        non_fatal @ SubscriberError::ClientExecutionError(_) => {
                            debug!("{non_fatal}");
                            (Ok(()), Err(non_fatal))
                        }
                        // We must take special care to errors that are our fault, such as storage errors.
                        // We may be the only authority experiencing it, and thus cannot continue to process
                        // transactions until the problem is fixed.
                        fatal => (Err(error), Err(fatal)),
                    },
                }
            }
        };

        // Output the result (eg. to notify the end-user);
        let output = (outcome, serialized);
        if self.tx_output.send(output).await.is_err() {
            debug!("No users listening for transaction execution");
        }

        result
    }
}
