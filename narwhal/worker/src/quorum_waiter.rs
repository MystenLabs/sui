// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::batch_maker::MAX_PARALLEL_BATCH;
use config::{Committee, SharedWorkerCache, Stake, WorkerId};
use crypto::PublicKey;
use fastcrypto::hash::Hash;
use futures::stream::{futures_unordered::FuturesUnordered, FuturesOrdered, StreamExt as _};
use network::{CancelOnDropHandler, P2pNetwork, ReliableNetwork};
use std::time::Duration;
use sui_metrics::{monitored_future, spawn_monitored_task};
use tokio::{sync::watch, task::JoinHandle, time::timeout};
use tracing::{error, trace};
use types::{metered_channel::Receiver, Batch, ReconfigureNotification, WorkerBatchMessage};

#[cfg(test)]
#[path = "tests/quorum_waiter_tests.rs"]
pub mod quorum_waiter_tests;

/// The QuorumWaiter waits for 2f authorities to acknowledge reception of a batch.
pub struct QuorumWaiter {
    /// The public key of this authority.
    name: PublicKey,
    /// The id of this worker.
    id: WorkerId,
    /// The committee information.
    committee: Committee,
    /// The worker information cache.
    worker_cache: SharedWorkerCache,
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    /// Input Channel to receive commands.
    rx_message: Receiver<(Batch, Option<tokio::sync::oneshot::Sender<()>>)>,
    /// A network sender to broadcast the batches to the other workers.
    network: P2pNetwork,
}

impl QuorumWaiter {
    /// Spawn a new QuorumWaiter.
    #[must_use]
    pub fn spawn(
        name: PublicKey,
        id: WorkerId,
        committee: Committee,
        worker_cache: SharedWorkerCache,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        rx_message: Receiver<(Batch, Option<tokio::sync::oneshot::Sender<()>>)>,
        network: P2pNetwork,
    ) -> JoinHandle<()> {
        spawn_monitored_task!(async move {
            Self {
                name,
                id,
                committee,
                worker_cache,
                rx_reconfigure,
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
        //
        let mut pipeline = FuturesOrdered::new();
        let mut best_effort_with_timeout = FuturesUnordered::new();

        loop {
            tokio::select! {

                // When a new batch is available, and the pipeline is not full, add a new
                // task to the pipeline to send this batch to workers.
                //
                // TODO: make the constant a config parameter.
                Some((batch, opt_channel)) = self.rx_message.recv(), if pipeline.len() < MAX_PARALLEL_BATCH => {
                    // Broadcast the batch to the other workers.
                    let workers: Vec<_> = self
                        .worker_cache
                        .load()
                        .others_workers_by_id(&self.name, &self.id)
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
                            monitored_future!(Self::waiter(handler, stake))
                        })
                        .collect();

                    // Wait for the first 2f nodes to send back an Ack. Then we consider the batch
                    // delivered and we send its digest to the primary (that will include it into
                    // the dag). This should reduce the amount of synching.
                    let threshold = self.committee.quorum_threshold();
                    let mut total_stake = self.committee.stake(&self.name);

                    pipeline.push_back(async move {
                        // A future that sends to 2/3 stake then returns. Also prints an error
                        // if we terminate before we have managed to get to the full 2/3 stake.

                        loop{
                            if let Some(stake) = wait_for_quorum.next().await {
                                total_stake += stake;
                                if total_stake >= threshold {

                                    // Notify anyone waiting for this.
                                    if let Some(channel) = opt_channel {
                                        if let Err(e) = channel.send(()) {
                                            error!("Channel waiting for quorum response dropped: {:?}", e);
                                        }
                                    }
                                    break
                                }
                            } else {
                                // TODO: maybe we should be stopping the system if that happens,
                                //       since it seems serious. However, it may happen at the
                                //       start of the epoch when not everyone is ready?
                                tracing::error!("Batch dissemination ended without a quorum.");
                                break;
                            }
                        }
                        (batch, wait_for_quorum)
                    });
                },

                // Process futures in the pipeline. They complete when we have sent to >2/3
                // of other worker by stake, but after that we still try to send to the remaining
                // on a best effort basis.
                Some((batch, mut remaining)) = pipeline.next() => {

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

                // Trigger reconfigure.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewEpoch(new_committee) => {
                            self.committee = new_committee;
                        },
                        ReconfigureNotification::UpdateCommittee(new_committee) => {
                            self.committee = new_committee;

                            // Upon reconfiguration we drop all current batches.
                            // TODO: maybe re-send to new committee?
                            tracing::error!("Batch dissemination dropped {} batches before quorum.", pipeline.len() );

                            pipeline = FuturesOrdered::new();
                            best_effort_with_timeout = FuturesUnordered::new()

                        },
                        ReconfigureNotification::Shutdown => return
                    }
                    tracing::debug!("Committee updated to {}", self.committee);
                }
            }
        }
    }
}
