// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::metrics::WorkerMetrics;
#[cfg(feature = "trace_transaction")]
use byteorder::{BigEndian, ReadBytesExt};
use fastcrypto::hash::Hash;
use futures::stream::FuturesUnordered;
use store::{rocks::DBMap, Map};

use config::WorkerId;
use tracing::{debug, error};

#[cfg(feature = "benchmark")]
use std::convert::TryInto;

use futures::{Future, StreamExt};

use mysten_metrics::spawn_logged_monitored_task;
use std::sync::Arc;
use tokio::{
    task::JoinHandle,
    time::{sleep, Duration, Instant},
};
use types::{
    error::DagError,
    metered_channel::{Receiver, Sender},
    now, Batch, BatchDigest, ConditionalBroadcastReceiver, PrimaryResponse, Transaction,
    TxResponse, WorkerOurBatchMessage,
};

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
    tx_quorum_waiter: Sender<(Batch, tokio::sync::oneshot::Sender<()>)>,
    /// Metrics handler
    node_metrics: Arc<WorkerMetrics>,
    /// The timestamp of the batch creation.
    /// Average resident time in the batch would be ~ (batch seal time - creation time) / 2
    batch_start_timestamp: Instant,
    /// The batch store to store our own batches.
    store: DBMap<BatchDigest, Batch>,
    // Output channel to send out batches' digests.
    tx_our_batch: Sender<(WorkerOurBatchMessage, PrimaryResponse)>,
}

impl BatchMaker {
    #[must_use]
    pub fn spawn(
        id: WorkerId,
        batch_size_limit: usize,
        max_batch_delay: Duration,
        rx_shutdown: ConditionalBroadcastReceiver,
        rx_batch_maker: Receiver<(Transaction, TxResponse)>,
        tx_quorum_waiter: Sender<(Batch, tokio::sync::oneshot::Sender<()>)>,
        node_metrics: Arc<WorkerMetrics>,
        store: DBMap<BatchDigest, Batch>,
        tx_our_batch: Sender<(WorkerOurBatchMessage, PrimaryResponse)>,
    ) -> JoinHandle<()> {
        spawn_logged_monitored_task!(
            async move {
                Self {
                    id,
                    batch_size_limit,
                    max_batch_delay,
                    rx_shutdown,
                    rx_batch_maker,
                    tx_quorum_waiter,
                    batch_start_timestamp: Instant::now(),
                    node_metrics,
                    store,
                    tx_our_batch,
                }
                .run()
                .await;
            },
            "BatchMakerTask"
        )
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
                    current_batch.transactions.push(transaction);
                    current_responses.push(response_sender);
                    if current_batch_size >= self.batch_size_limit {
                        if let Some(seal) = self.seal(false, current_batch, current_batch_size, current_responses).await{
                            batch_pipeline.push(seal);
                        }
                        self.node_metrics.parallel_worker_batches.set(batch_pipeline.len() as i64);

                        current_batch = Batch::default();
                        current_responses = Vec::new();
                        current_batch_size = 0;

                        timer.as_mut().reset(Instant::now() + self.max_batch_delay);
                        self.batch_start_timestamp = Instant::now();
                    }
                },

                // If the timer triggers, seal the batch even if it contains few transactions.
                () = &mut timer => {
                    if !current_batch.transactions.is_empty() {
                        if let Some(seal) = self.seal(true, current_batch, current_batch_size, current_responses).await {
                            batch_pipeline.push(seal);
                        }
                        self.node_metrics.parallel_worker_batches.set(batch_pipeline.len() as i64);

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
                    self.node_metrics.parallel_worker_batches.set(batch_pipeline.len() as i64);
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
        size: usize,
        responses: Vec<TxResponse>,
    ) -> Option<impl Future<Output = ()>> {
        #[cfg(feature = "benchmark")]
        {
            let digest = batch.digest();

            // Look for sample txs (they all start with 0) and gather their txs id (the next 8 bytes).
            let tx_ids: Vec<_> = batch
                .transactions
                .iter()
                .filter(|tx| tx[0] == 0u8 && tx.len() > 8)
                .filter_map(|tx| tx[1..9].try_into().ok())
                .collect();

            for id in tx_ids {
                // NOTE: This log entry is used to compute performance.
                tracing::info!(
                    "Batch {:?} contains sample tx {}",
                    digest,
                    u64::from_be_bytes(id)
                );
            }

            #[cfg(feature = "trace_transaction")]
            {
                // The first 8 bytes of each transaction message is reserved for an identifier
                // that's useful for debugging and tracking the lifetime of messages between
                // Narwhal and clients.
                let tracking_ids: Vec<_> = batch
                    .transactions
                    .iter()
                    .map(|tx| {
                        let len = tx.len();
                        if len >= 8 {
                            (&tx[0..8]).read_u64::<BigEndian>().unwrap_or_default()
                        } else {
                            0
                        }
                    })
                    .collect();
                tracing::debug!(
                    "Tracking IDs of transactions in the Batch {:?}: {:?}",
                    digest,
                    tracking_ids
                );
            }

            // NOTE: This log entry is used to compute performance.
            tracing::info!("Batch {:?} contains {} B", digest, size);
        }

        let reason = if timeout { "timeout" } else { "size_reached" };

        self.node_metrics
            .created_batch_size
            .with_label_values(&[reason])
            .observe(size as f64);

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
        self.node_metrics
            .created_batch_latency
            .with_label_values(&[reason])
            .observe(batch_creation_duration);

        // Clone things to not capture self
        let store = self.store.clone();
        let worker_id = self.id;
        let tx_our_batch = self.tx_our_batch.clone();

        // The batch has been sealed so we can officially set its creation time
        // for latency calculations.
        batch.metadata.created_at = now();
        let metadata = batch.metadata.clone();

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

            // Finally send to primary
            let (primary_response, batch_done) = tokio::sync::oneshot::channel();
            let message = WorkerOurBatchMessage {
                digest,
                worker_id,
                metadata,
            };
            if tx_our_batch
                .send((message, Some(primary_response)))
                .await
                .is_err()
            {
                debug!("Failed to send created batch to primary. Shutting down.");
                return;
            };

            // Wait for a primary response
            if batch_done.await.is_err() {
                // If there is an error it means the channel closed,
                // and therefore we drop all response handers since we
                // cannot ensure the primary has actually signaled the
                // batch will eventually be sent.
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
