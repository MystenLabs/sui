// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::batch_maker::MAX_PARALLEL_BATCH;
use config::{Authority, Committee, Stake, WorkerCache, WorkerId};
use fastcrypto::hash::Hash;
use futures::stream::{futures_unordered::FuturesUnordered, FuturesOrdered, StreamExt as _};
use network::{CancelOnDropHandler, ReliableNetwork};
use std::time::Duration;
use tokio::{sync::mpsc, task::JoinHandle, time::timeout};
use tracing::{error, trace};
use types::{Batch, ConditionalBroadcastReceiver, WorkerBatchMessage};

#[cfg(test)]
#[path = "tests/quorum_waiter_tests.rs"]
pub mod quorum_waiter_tests;

/// The QuorumWaiter waits for 2f authorities to acknowledge reception of a batch.
pub struct QuorumWaiter {
    /// This authority.
    authority: Authority,
    /// The id of this worker.
    id: WorkerId,
    /// The committee information.
    committee: Committee,
    /// The worker information cache.
    worker_cache: WorkerCache,
    /// Receiver for shutdown.
    rx_shutdown: ConditionalBroadcastReceiver,
    /// Input Channel to receive commands.
    rx_message: mpsc::Receiver<(Batch, Option<tokio::sync::oneshot::Sender<()>>)>,
    /// A network sender to broadcast the batches to the other workers.
    network: anemo::Network,
}

impl QuorumWaiter {
    /// Spawn a new QuorumWaiter.
    #[must_use]
    pub fn spawn(
        authority: Authority,
        id: WorkerId,
        committee: Committee,
        worker_cache: WorkerCache,
        rx_shutdown: ConditionalBroadcastReceiver,
        rx_message: mpsc::Receiver<(Batch, Option<tokio::sync::oneshot::Sender<()>>)>,
        network: anemo::Network,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                id,
                committee,
                worker_cache,
                rx_shutdown,
                rx_message,
                network,
            }
            .run()
            .await;
        })
    }

    /// Helper function. It waits for a future to complete and then delivers a value.
    async fn waiter(
        wait_for: CancelOnDropHandler<anemo::Result<anemo::Response<()>>>,
        deliver: Stake,
    ) -> Stake {
        let _ = wait_for.await;
        deliver
    }

    /// Main loop.
    async fn run(&mut self) {
        let mut pipeline = FuturesUnordered::new();
        let mut best_effort_with_timeout = FuturesUnordered::new();

        loop {
            tokio::select! {

                // When a new batch is available, and the pipeline is not full, add a new
                // task to the pipeline to send this batch to workers.
                //
                // TODO: make the constant a config parameter.
                Some((batch, channel)) = self.rx_quorum_waiter.recv(), if pipeline.len() < MAX_PARALLEL_BATCH => {
                    // Broadcast the batch to the other workers.
                    let workers: Vec<_> = self
                        .worker_cache
                        .others_workers_by_id(self.authority.protocol_key(), &self.id)
                        .into_iter()
                        .map(|(name, info)| (name, info.name))
                        .collect();
                    let (primary_names, worker_names): (Vec<_>, _) = workers.into_iter().unzip();
                    let message  = WorkerBatchMessage{batch: batch.clone()};
                    let handlers = self.network.broadcast(worker_names, &message);

                    // Collect all the handlers to receive acknowledgements.
                    let mut wait_for_quorum: FuturesUnordered<_> = primary_names
                        .into_iter()
                        .zip(handlers.into_iter())
                        .map(|(name, handler)| {
                            let stake = self.committee.stake(&name);
                            Self::waiter(handler, stake)
                        })
                        .collect();

                    // Wait for the first 2f nodes to send back an Ack. Then we consider the batch
                    // delivered and we send its digest to the primary (that will include it into
                    // the dag). This should reduce the amount of syncing.
                    let threshold = self.committee.quorum_threshold();
                    let mut total_stake = self.authority.stake();

                    pipeline.push(async move {
                        // A future that sends to 2/3 stake then returns. Also prints a warning
                        // if we terminate before we have managed to get to the full 2/3 stake.
                        let mut opt_channel = Some(channel);
                        loop{
                            if let Some(stake) = wait_for_quorum.next().await {
                                total_stake += stake;
                                if total_stake >= threshold {
                                    // Notify anyone waiting for this.
                                    let channel = opt_channel.take().unwrap();
                                    if let Err(e) = channel.send(()) {
                                        warn!("Channel waiting for quorum response dropped: {:?}", e);
                                    }
                                    break
                                }
                            } else {
                                // This should not happen unless shutting down, because
                                // `broadcast()` is supposed to keep retrying.
                                warn!("Batch dissemination ended without a quorum. Shutting down.");
                                break;
                            }
                        }
                        (batch, opt_channel, wait_for_quorum)
                    });
                },

                // Process futures in the pipeline. They complete when we have sent to >2/3
                // of other worker by stake, but after that we still try to send to the remaining
                // on a best effort basis.
                Some((batch, opt_channel, mut remaining)) = pipeline.next() => {
                    // opt_channel is not consumed only when the worker is shutting down and
                    // broadcast fails. TODO: switch to returning a status from pipeline.
                    if opt_channel.is_some() {
                        return;
                    }
                    // Attempt to send messages to the remaining workers
                    if !remaining.is_empty() {
                        trace!("Best effort dissemination for batch {} for remaining {}", batch.digest(), remaining.len());
                        best_effort_with_timeout.push(async move {
                           // Bound the attempt to a few seconds to tolerate nodes that are
                           // offline and will never succeed.
                           //
                           // TODO: make the constant a config parameter.
                           timeout(Duration::from_secs(5), async move{
                               while remaining.next().await.is_some() { }
                           }).await
                       });
                    }
                },

                // Drive the best effort send efforts which may update remaining workers
                // or timeout.
                Some(_) = best_effort_with_timeout.next() => {}

                _ = self.rx_shutdown.receiver.recv() => {
                    return
                }
            }
        }
    }
}
