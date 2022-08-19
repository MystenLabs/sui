// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    errors::SubscriberResult, metrics::ExecutorMetrics, try_fut_and_permit, SubscriberError,
    SubscriberError::PayloadRetrieveError,
};
use backoff::{Error, ExponentialBackoff};
use consensus::ConsensusOutput;
use fastcrypto::Hash;
use primary::BlockCommand;
use std::{sync::Arc, time::Duration};
use store::Store;
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
};
use tracing::error;
use types::{
    bounded_future_queue::BoundedFuturesOrdered, metered_channel, Batch, BatchDigest,
    ReconfigureNotification,
};

#[cfg(test)]
#[path = "tests/subscriber_tests.rs"]
pub mod subscriber_tests;

/// The `Subscriber` receives certificates sequenced by the consensus and waits until the
/// downloaded all the transactions references by the certificates; it then
/// forward the certificates to the Executor Core.
pub struct Subscriber {
    /// The temporary storage holding all transactions' data (that may be too big to hold in memory).
    store: Store<BatchDigest, Batch>,
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    /// A channel to receive consensus messages.
    rx_consensus: metered_channel::Receiver<ConsensusOutput>,
    /// A channel to send the complete and ordered list of consensus outputs to the executor. This
    /// channel is used once all transactions data are downloaded.
    tx_executor: metered_channel::Sender<ConsensusOutput>,
    // A channel to send commands to the block waiter to receive
    // a certificate's batches (block).
    tx_get_block_commands: metered_channel::Sender<BlockCommand>,
    // When asking for a certificate's payload we want to retry until we succeed, unless
    // some irrecoverable error occurs. For that reason a backoff policy is defined
    get_block_retry_policy: ExponentialBackoff,
    /// The metrics handler
    metrics: Arc<ExecutorMetrics>,
}

impl Subscriber {
    /// Returns the max amount of pending consensus messages we should expect.
    const MAX_PENDING_CONSENSUS_MESSAGES: usize = 2000;

    /// Spawn a new subscriber in a new tokio task.
    #[must_use]
    pub fn spawn(
        store: Store<BatchDigest, Batch>,
        tx_get_block_commands: metered_channel::Sender<BlockCommand>,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        rx_consensus: metered_channel::Receiver<ConsensusOutput>,
        tx_executor: metered_channel::Sender<ConsensusOutput>,
        metrics: Arc<ExecutorMetrics>,
    ) -> JoinHandle<()> {
        let get_block_retry_policy = ExponentialBackoff {
            initial_interval: Duration::from_millis(500),
            randomization_factor: backoff::default::RANDOMIZATION_FACTOR,
            multiplier: backoff::default::MULTIPLIER,
            max_interval: Duration::from_secs(10), // Maximum backoff is 10 seconds
            max_elapsed_time: None, // Never end retrying unless a non recoverable error occurs.
            ..Default::default()
        };

        tokio::spawn(async move {
            Self {
                store,
                rx_reconfigure,
                rx_consensus,
                tx_executor,
                tx_get_block_commands,
                get_block_retry_policy,
                metrics,
            }
            .run()
            .await
            .expect("Failed to run subscriber")
        })
    }

    /// Main loop connecting to the consensus to listen to sequence messages.
    async fn run(&mut self) -> SubscriberResult<()> {
        // It's important to have the futures in ordered fashion as we want
        // to guarantee that will deliver to the executor the certificates
        // in the same order we received from rx_consensus. So it doesn't
        // mater if we somehow managed to fetch the batches from a later
        // certificate. Unless the earlier certificate's payload has been
        // fetched, no later certificate will be delivered.
        let mut waiting =
            BoundedFuturesOrdered::with_capacity(Self::MAX_PENDING_CONSENSUS_MESSAGES);

        // Listen to sequenced consensus message and process them.
        loop {
            tokio::select! {
                // Receive the ordered sequence of consensus messages from a consensus node.
                Some(message) = self.rx_consensus.recv(), if waiting.available_permits() > 0 => {
                    // Fetch the certificate's payload from the workers. This is done via the
                    // block_waiter component. If the batches are not available in the workers then
                    // block_waiter will do its best to sync from the other peers. Once all batches
                    // are available, we forward the certificate o the Executor Core.
                    let future = Self::wait_on_payload(
                        self.get_block_retry_policy.clone(),
                        self.store.clone(),
                        self.tx_get_block_commands.clone(),
                        message);
                    waiting.push(future).await;
                },

                // Receive here consensus messages for which we have downloaded all transactions data.
                (Some(message), permit) = try_fut_and_permit!(waiting.try_next(), self.tx_executor) => {
                    permit.send(message)
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

            self.metrics
                .waiting_elements_subscriber
                .set(waiting.len() as i64);
        }
    }

    /// The wait_on_payload will try to retrieve the certificate's payload
    /// from the workers via the block_waiter component and relase the
    /// `deliver` once successfully done. Since we want the output to be
    /// sequenced we will not quit this method until we have successfully
    /// fetched the payload.
    async fn wait_on_payload(
        back_off_policy: ExponentialBackoff,
        store: Store<BatchDigest, Batch>,
        tx_get_block_commands: metered_channel::Sender<BlockCommand>,
        deliver: ConsensusOutput,
    ) -> SubscriberResult<ConsensusOutput> {
        let get_block = move || {
            let message = deliver.clone();
            let id = message.certificate.digest();
            let tx_get_block = tx_get_block_commands.clone();
            let batch_store = store.clone();

            async move {
                let (sender, receiver) = oneshot::channel();

                tx_get_block
                    .send(BlockCommand::GetBlock { id, sender })
                    .await
                    .map_err(|err| Error::permanent(PayloadRetrieveError(id, err.to_string())))?;

                match receiver
                    .await
                    .map_err(|err| Error::permanent(PayloadRetrieveError(id, err.to_string())))?
                {
                    Ok(block) => {
                        // we successfully received the payload. Now let's add to store
                        batch_store
                            .write_all(block.batches.into_iter().map(|b| (b.id, b.transactions)))
                            .await
                            .map_err(|err| Error::permanent(SubscriberError::from(err)))?;

                        Ok(message)
                    }
                    Err(err) => {
                        // whatever the error might be at this point we don't
                        // have many options apart from retrying.
                        error!("Error while retrieving block via block waiter: {}", err);
                        Err(Error::transient(PayloadRetrieveError(id, err.to_string())))
                    }
                }
            }
        };

        backoff::future::retry(back_off_policy, get_block).await
    }
}
