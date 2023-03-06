// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#[cfg(feature = "trace_transaction")]
use byteorder::{BigEndian, ReadBytesExt};
use fastcrypto::hash::Hash;
use futures::stream::FuturesOrdered;
use store::Store;

use config::WorkerId;
use tracing::error;

use futures::{Future, StreamExt};

use tokio::{
    sync::mpsc::{Receiver, Sender},
    task::JoinHandle,
    time::{sleep, Duration, Instant},
};
use tracing::{error, warn};
use types::{
    error::DagError, now, Batch, BatchDigest, ConditionalBroadcastReceiver, PrimaryResponse,
    Transaction, TxResponse, WorkerOurBatchMessage,
};

use crate::metrics::WorkerMetrics;

#[cfg(feature = "trace_transaction")]
use byteorder::{BigEndian, ReadBytesExt};
#[cfg(feature = "benchmark")]
use std::convert::TryInto;

// The number of batches to store / transmit in parallel.
pub const MAX_PARALLEL_BATCH: usize = 100;

#[cfg(test)]
#[path = "tests/batch_maker_tests.rs"]
pub mod batch_maker_tests;

/// Assemble clients transactions into batches.
pub struct BatchMaker {
    // Our worker's id.
    id: WorkerId,
    /// The preferred batch size (in bytes).
    batch_size_limit: usize,
    /// The maximum delay after which to seal the batch.
    max_batch_delay: Duration,
    /// Receiver for shutdown.
    rx_shutdown: ConditionalBroadcastReceiver,
    /// Channel to receive transactions from the network.
    rx_batch_maker: Receiver<(Transaction, TxResponse)>,
    /// Output channel to deliver sealed batches to the `QuorumWaiter`.
    tx_message: Sender<(Batch, Option<tokio::sync::oneshot::Sender<()>>)>,
    /// The timestamp of the batch creation.
    /// Average resident time in the batch would be ~ (batch seal time - creation time) / 2
    batch_start_timestamp: Instant,
    /// The network client to send our batches to the primary.
    client: NetworkClient,
    /// The batch store to store our own batches.
    store: DBMap<BatchDigest, Batch>,
}

impl BatchMaker {
    #[must_use]
    pub fn spawn(
        id: WorkerId,
        batch_size_limit: usize,
        max_batch_delay: Duration,
        rx_shutdown: ConditionalBroadcastReceiver,
        rx_batch_maker: Receiver<(Transaction, TxResponse)>,
        tx_message: Sender<(Batch, Option<tokio::sync::oneshot::Sender<()>>)>,
        store: Store<BatchDigest, Batch>,
        tx_digest: Sender<(WorkerOurBatchMessage, PrimaryResponse)>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                id,
                batch_size,
                max_batch_delay,
                rx_shutdown,
                rx_batch_maker,
                tx_message,
                batch_start_timestamp: Instant::now(),
                store,
                tx_digest,
            }
            .run()
            .await;
        })
    }

    /// Main loop receiving incoming transactions and creating batches.
    async fn run(&mut self) {
        let timer = sleep(self.max_batch_delay);
        tokio::pin!(timer);

        let mut current_batch = Batch::default();
        let mut current_responses = Vec::new();
        let mut current_batch_size = 0;

        let mut batch_pipeline = FuturesUnordered::new();

        loop {
            tokio::select! {
                // Assemble client transactions into batches of preset size.
                // Note that transactions are only consumed when the number of batches
                // 'in-flight' are below a certain number (MAX_PARALLEL_BATCH). This
                // condition will be met eventually if the store and network are functioning.
                Some((transaction, response_sender)) = self.rx_batch_maker.recv(), if batch_pipeline.len() < MAX_PARALLEL_BATCH => {
                    current_batch_size += transaction.len();
                    current_batch.transactions_mut().push(transaction);
                    current_responses.push(response_sender);
                    if current_batch_size >= self.batch_size_limit {
                        if let Some(seal) = self.seal(false, current_batch, current_batch_size, current_responses).await{
                            batch_pipeline.push(seal);
                        }

                        // TODO(metrics): Set `parallel_worker_batches` to `batch_pipeline.len() as i64`

                        current_batch = Batch::default();
                        current_responses = Vec::new();
                        current_batch_size = 0;

                        timer.as_mut().reset(Instant::now() + self.max_batch_delay);
                        self.batch_start_timestamp = Instant::now();
                    }
                },

                // If the timer triggers, seal the batch even if it contains few transactions.
                () = &mut timer => {
                    if !current_batch.transactions().is_empty() {
                        if let Some(seal) = self.seal(true, current_batch, current_batch_size, current_responses).await {
                            batch_pipeline.push(seal);
                        }

                        // TODO(metrics): Set `parallel_worker_batches` to `batch_pipeline.len() as i64`

                        current_batch = Batch::default();
                        current_responses = Vec::new();
                        current_batch_size = 0;
                    }
                    timer.as_mut().reset(Instant::now() + self.max_batch_delay);
                    self.batch_start_timestamp = Instant::now();
                }

                _ = self.rx_shutdown.receiver.recv() => {
                    return
                }

                // Process the pipeline of batches, this consumes items in the `batch_pipeline`
                // list, and ensures the main loop in run will always be able to make progress
                // by lowering it until condition batch_pipeline.len() < MAX_PARALLEL_BATCH is met.
                _ = batch_pipeline.next(), if !batch_pipeline.is_empty() => {
                    // TODO(metrics): Set `parallel_worker_batches` to `batch_pipeline.len() as i64`
                }

            }

            // Give the change to schedule other tasks.
            tokio::task::yield_now().await;
        }
    }

    /// Seal and broadcast the current batch.
    async fn seal(
        &self,
        timeout: bool,
        mut batch: Batch,
        #[allow(unused_variables)] size: usize,
        responses: Vec<TxResponse>,
    ) -> Option<impl Future<Output = ()>> {
        let reason = if timeout { "timeout" } else { "size_reached" };

        // TODO(metrics): Observe `size as f64` as `created_batch_size`

        // Send the batch through the deliver channel for further processing.
        let (notify_done, done_sending) = tokio::sync::oneshot::channel();
        if self
            .tx_quorum_waiter
            .send((batch.clone(), notify_done))
            .await
            .is_err()
        {
            tracing::debug!("{}", DagError::ShuttingDown);
            return None;
        }

        let batch_creation_duration = self.batch_start_timestamp.elapsed().as_secs_f64();

        tracing::debug!(
            "Batch {:?} took {} seconds to create due to {}",
            batch.digest(),
            batch_creation_duration,
            reason
        );

        // we are deliberately measuring this after the sending to the downstream
        // channel tx_quorum_waiter as the operation is blocking and affects any further
        // batch creation.
        // TODO(metrics): Observe `batch_creation_duration` as `created_batch_latency`

        // Clone things to not capture self
        let client = self.client.clone();
        let store = self.store.clone();
        let worker_id = self.id;

        // The batch has been sealed so we can officially set its creation time
        // for latency calculations.
        batch.metadata_mut().created_at = now();
        let metadata = batch.metadata().clone();

        Some(async move {
            // Now save it to disk
            let digest = batch.digest();

            if let Err(e) = store.insert(&digest, &batch) {
                error!("Store failed with error: {:?}", e);
                return;
            }

            // Also wait for sending to be done here
            //
            // TODO: Here if we get back Err it means that potentially this was not send
            //       to a quorum. However, if that happens we can still proceed on the basis
            //       that an other authority will request the batch from us, and we will deliver
            //       it since it is now stored. So ignore the error for the moment.
            let _ = done_sending.await;

            // Send the batch to the primary.
            let message = WorkerOurBatchMessage {
                digest,
                worker_id,
                metadata,
            };
            if let Err(e) = client.report_our_batch(message).await {
                warn!("Failed to report our batch: {}", e);
                // Drop all response handers to signal error, since we
                // cannot ensure the primary has actually signaled the
                // batch will eventually be sent.
                // The transaction submitter will see the error and retry.
                return;
            }

            // We now signal back to the transaction sender that the transaction is in a
            // batch and also the digest of the batch.
            for response in responses {
                let _ = response.send(digest);
            }
        })
    }
}
