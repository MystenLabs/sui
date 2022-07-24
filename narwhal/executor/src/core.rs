// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    bail,
    errors::{SubscriberError, SubscriberResult},
    state::ExecutionIndices,
    ExecutionState, ExecutorOutput, SerializedTransaction,
};
use config::Committee;
use consensus::ConsensusOutput;
use crypto::traits::VerifyingKey;
use std::{fmt::Debug, sync::Arc};
use store::Store;
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        watch,
    },
    task::JoinHandle,
};
use tracing::debug;
use types::{Batch, BatchDigest, ReconfigureNotification, SequenceNumber, SerializedBatchMessage};
use worker::WorkerMessage;

#[cfg(test)]
#[path = "tests/executor_tests.rs"]
pub mod executor_tests;

/// Use the execution state to execute transactions. This module expects to receive a sequence of
/// consensus messages in the right and complete order. All transactions data referenced by the
/// certificate should already be downloaded in the temporary storage. This module ensures it does
/// not processes twice the same transaction (despite crash-recovery).
pub struct Core<State: ExecutionState, PublicKey: VerifyingKey> {
    /// The temporary storage holding all transactions' data (that may be too big to hold in memory).
    store: Store<BatchDigest, SerializedBatchMessage>,
    /// The (global) state to perform execution.
    execution_state: Arc<State>,
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
    /// Receive ordered consensus output to execute.
    rx_subscriber: Receiver<ConsensusOutput<PublicKey>>,
    /// Outputs executed transactions.
    tx_output: Sender<ExecutorOutput<State>>,
    /// The indices ensuring we do not execute twice the same transaction.
    execution_indices: ExecutionIndices,
}

impl<State: ExecutionState, PublicKey: VerifyingKey> Drop for Core<State, PublicKey> {
    fn drop(&mut self) {
        self.execution_state.release_consensus_write_lock();
    }
}

impl<State, PublicKey> Core<State, PublicKey>
where
    State: ExecutionState + Send + Sync + 'static,
    State::Outcome: Send + 'static,
    State::Error: Debug,
    PublicKey: VerifyingKey,
{
    /// Spawn a new executor in a dedicated tokio task.
    pub fn spawn(
        store: Store<BatchDigest, SerializedBatchMessage>,
        execution_state: Arc<State>,
        rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
        rx_subscriber: Receiver<ConsensusOutput<PublicKey>>,
        tx_output: Sender<ExecutorOutput<State>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let execution_indices = execution_state
                .load_execution_indices()
                .await
                .expect("Failed to load execution indices from store");
            Self {
                store,
                execution_state,
                rx_reconfigure,
                rx_subscriber,
                tx_output,
                execution_indices,
            }
            .run()
            .await
            .expect("Failed to run core")
        })
    }

    /// Main loop listening to new certificates and execute them.
    async fn run(&mut self) -> SubscriberResult<()> {
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
                    match message {
                        ReconfigureNotification::NewCommittee(_) => (),
                        ReconfigureNotification::Shutdown => return Ok(())
                    }
                }
            }
        }
    }

    /// Execute a single certificate.
    async fn execute_certificate(
        &mut self,
        message: &ConsensusOutput<PublicKey>,
    ) -> SubscriberResult<()> {
        // Skip the certificate if it contains no transactions.
        if message.certificate.header.payload.is_empty() {
            self.execution_indices.skip_certificate();
            return Ok(());
        }

        // Execute every batch in the certificate.
        let total_batches = message.certificate.header.payload.len();
        for (index, digest) in message.certificate.header.payload.keys().enumerate() {
            // Skip batches that we already executed (after crash-recovery).
            if self
                .execution_indices
                .check_next_batch_index(index as SequenceNumber)
            {
                self.execute_batch(message, *digest, total_batches).await?;
            }
        }
        Ok(())
    }

    /// Execute a single batch of transactions.
    async fn execute_batch(
        &mut self,
        consensus_output: &ConsensusOutput<PublicKey>,
        batch_digest: BatchDigest,
        total_batches: usize,
    ) -> SubscriberResult<()> {
        // The store should now hold all transaction data referenced by the input certificate.
        let batch = match self.store.read(batch_digest).await? {
            Some(x) => x,
            None => {
                // If two certificates contain the exact same batch (eg. by the actions of a Byzantine
                // consensus node), some correct client may already have deleted the batch from their
                // temporary storage while others may not. This is not a problem, we can simply ignore
                // the second batch since there is no point in executing twice the same transactions
                // (as the second execution attempt will always fail).
                debug!("Duplicate batch {batch_digest}");
                self.execution_indices.skip_batch(total_batches);
                return Ok(());
            }
        };

        // Deserialize the consensus workers' batch message to retrieve a list of transactions.
        let transactions = match bincode::deserialize(&batch)? {
            WorkerMessage::<PublicKey>::Batch(Batch(x)) => x,
            _ => bail!(SubscriberError::UnexpectedProtocolMessage),
        };

        // Execute every transaction in the batch.
        let total_transactions = transactions.len();
        for (index, transaction) in transactions.into_iter().enumerate() {
            // Skip transactions that we already executed (after crash-recovery).
            if self
                .execution_indices
                .check_next_transaction_index(index as SequenceNumber)
            {
                // Execute the transaction
                let result = self
                    .execute_transaction(
                        consensus_output,
                        transaction.clone(),
                        total_transactions,
                        total_batches,
                    )
                    .await;

                let (bail, result) = match result {
                    Ok((outcome, committee)) => {
                        if let Some(_committee) = committee {
                            // TODO: Do we really need to receive back the committee here? We will know
                            // once we try to integrate Narwhal with Sui. #580
                            // todo!();
                        }

                        (None, Ok(outcome))
                    }

                    // We may want to log the errors that are the user's fault (i.e., that are neither
                    // our fault or the fault of consensus) for debug purposes. It is safe to continue
                    // by ignoring those transactions since all honest subscribers will do the same.
                    Err(error @ SubscriberError::ClientExecutionError(_)) => {
                        debug!("{error}");
                        (None, Err(error))
                    }

                    // We must take special care to errors that are our fault, such as storage errors.
                    // We may be the only authority experiencing it, and thus cannot continue to process
                    // transactions until the problem is fixed.
                    Err(error) => (Some(error.clone()), Err(error)),
                };

                // Output the result (eg. to notify the end-user);
                let output = (result, transaction);
                if self.tx_output.send(output).await.is_err() {
                    debug!("No users listening for transaction execution");
                }

                // Bail if a fatal error occurred.
                if let Some(e) = bail {
                    bail!(e);
                }
            }
        }
        Ok(())
    }

    /// Execute a single transaction.
    async fn execute_transaction(
        &mut self,
        consensus_output: &ConsensusOutput<PublicKey>,
        serialized: SerializedTransaction,
        total_transactions: usize,
        total_batches: usize,
    ) -> SubscriberResult<(
        <State as ExecutionState>::Outcome,
        Option<Committee<PublicKey>>,
    )> {
        // Compute the next expected indices. Those will be persisted upon transaction execution
        // and are only used for crash-recovery.
        self.execution_indices
            .next(total_batches, total_transactions);

        // The consensus simply orders bytes, so we first need to deserialize the transaction.
        // If the deserialization fail it is safe to ignore the transaction since all correct
        // clients will do the same. Remember that a bad authority or client may input random
        // bytes to the consensus.
        let transaction: State::Transaction = match bincode::deserialize(&serialized) {
            Ok(x) => x,
            Err(e) => bail!(SubscriberError::ClientExecutionError(format!(
                "Failed to deserialize transaction: {e}"
            ))),
        };

        // Execute the transaction. Note that the executor will need to choose whether to discard
        // transactions from previous epochs by itself.
        self.execution_state
            .handle_consensus_transaction(
                consensus_output,
                self.execution_indices.clone(),
                transaction,
            )
            .await
            .map_err(SubscriberError::from)
    }
}
