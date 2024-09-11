// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! RoundProber periodically probes every peer for the latest rounds they received from other peers.
//! This creates a picture of how well each authority's blocks are propagated through the network.
//!
//! Compared to inferring peers' accepted rounds from the DAG represented by each block, the
//! advantage of RoundProber is that it is active even when peers are not proposing. This makes
//! the component necessary for deciding when to disable the various optimizations that lower
//! the network latency but can cost liveness.
//!
//! The source of data used by RoundProber are the `highest_received_rounds` tracked in the
//! ChannelCoreThreadDispatcher. They are updated after verification of the blocks, but before
//! the blocks are checked for dependencies. This should make the values more relevant to how well
//! authorities propagate blocks, and less affected by the ancestors included in the blocks.

use std::{sync::Arc, time::Duration};

use consensus_config::AuthorityIndex;
use futures::stream::{FuturesUnordered, StreamExt as _};
use mysten_common::sync::notify_once::NotifyOnce;
use parking_lot::RwLock;
use tokio::{task::JoinHandle, time::MissedTickBehavior};

use crate::{
    context::Context, core_thread::CoreThreadDispatcher, dag_state::DagState,
    network::NetworkClient, BlockAPI as _, Round,
};

// Handle to control the RoundProber loop and read latest round gaps.
pub(crate) struct RoundProberHandle {
    prober_task: JoinHandle<()>,
    shutdown_notif: Arc<NotifyOnce>,
}

impl RoundProberHandle {
    pub(crate) async fn stop(self) {
        let _ = self.shutdown_notif.notify();
        // Do not abort prober task, which waits for requests to be cancelled.
        if let Err(e) = self.prober_task.await {
            if e.is_panic() {
                std::panic::resume_unwind(e.into_panic());
            }
        }
    }
}

pub(crate) struct RoundProber<C: NetworkClient> {
    context: Arc<Context>,
    core_thread_dispatcher: Arc<dyn CoreThreadDispatcher>,
    dag_state: Arc<RwLock<DagState>>,
    network_client: Arc<C>,
    shutdown_notif: Arc<NotifyOnce>,
}

impl<C: NetworkClient> RoundProber<C> {
    pub(crate) fn new(
        context: Arc<Context>,
        core_thread_dispatcher: Arc<dyn CoreThreadDispatcher>,
        dag_state: Arc<RwLock<DagState>>,
        network_client: Arc<C>,
    ) -> Self {
        Self {
            context,
            core_thread_dispatcher,
            dag_state,
            network_client,
            shutdown_notif: Arc::new(NotifyOnce::new()),
        }
    }

    pub(crate) fn start(self) -> RoundProberHandle {
        let shutdown_notif = self.shutdown_notif.clone();
        let loop_shutdown_notif = shutdown_notif.clone();
        let prober_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        self.probe().await;
                    }
                    _ = loop_shutdown_notif.wait() => {
                        break;
                    }
                }
            }
        });
        RoundProberHandle {
            prober_task,
            shutdown_notif,
        }
    }

    pub(crate) async fn probe(&self) {
        const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

        let own_index = self.context.own_index;
        let last_proposed_round = self
            .dag_state
            .read()
            .get_last_block_for_authority(own_index)
            .round();

        let mut requests = FuturesUnordered::new();
        for (peer, _) in self.context.committee.authorities() {
            if peer == own_index {
                continue;
            }
            let network_client = self.network_client.clone();
            requests.push(async move {
                let result = tokio::time::timeout(
                    PROBE_TIMEOUT,
                    network_client.get_latest_rounds(peer, PROBE_TIMEOUT),
                )
                .await;
                (peer, result)
            });
        }

        let mut highest_received_rounds =
            vec![vec![0; self.context.committee.size()]; self.context.committee.size()];
        highest_received_rounds[own_index] = self.core_thread_dispatcher.highest_received_rounds();
        highest_received_rounds[own_index][own_index] = last_proposed_round;
        loop {
            tokio::select! {
                result = requests.next() => {
                    let Some((peer, result)) = result else {
                        break;
                    };
                    match result {
                        Ok(Ok(rounds)) => {
                            highest_received_rounds[peer] = rounds;
                        }
                        Ok(Err(err)) => {
                            self.context.metrics.node_metrics.round_prober_request_timeouts.inc();
                            tracing::warn!("Failed to get latest rounds from peer {}: {:?}", peer, err);
                        }
                        Err(_) => {
                            self.context.metrics.node_metrics.round_prober_request_timeouts.inc();
                            tracing::warn!("Timeout while getting latest rounds from peer {}", peer);
                        }
                    }
                }
                _ = self.shutdown_notif.wait() => {
                    break;
                }
            }
        }

        let quorum_rounds: Vec<_> = self
            .context
            .committee
            .authorities()
            .map(|(peer, _)| self.compute_quorum_rounds(peer, &highest_received_rounds))
            .collect();
        for ((low, high), (_, authority)) in quorum_rounds
            .iter()
            .zip(self.context.committee.authorities())
        {
            self.context
                .metrics
                .node_metrics
                .round_prober_quorum_gaps
                .with_label_values(&[&authority.hostname])
                .set((high - low) as i64);
        }

        // It is possible more blocks arrive at a quorum of peers before the get_latest_rounds
        // requests arrive.
        // Also, use the lower watermark to increase sensitivity about block propagation issues
        // that can reduce round rate.
        let propagation_delay = last_proposed_round.saturating_sub(quorum_rounds[own_index].0);
        self.context
            .metrics
            .node_metrics
            .round_prober_propagation_delay
            .set(propagation_delay as i64);
        if let Err(e) = self
            .core_thread_dispatcher
            .set_propagation_delay(propagation_delay)
        {
            tracing::warn!("Failed to set propagation round delay: {:?}", e);
        }
    }

    fn compute_quorum_rounds(
        &self,
        target_index: AuthorityIndex,
        highest_received_rounds: &[Vec<Round>],
    ) -> (Round, Round) {
        let mut rounds_with_stake = highest_received_rounds
            .iter()
            .zip(self.context.committee.authorities())
            .map(|(rounds, (_, authority))| (rounds[target_index], authority.stake))
            .collect::<Vec<_>>();
        rounds_with_stake.sort();

        let mut total_stake = 0;
        let mut low = 0;
        let mut high = 0;
        for (round, stake) in rounds_with_stake {
            let reached_validator_before =
                total_stake >= self.context.committee.validity_threshold();
            let reached_quorum_before = total_stake >= self.context.committee.quorum_threshold();
            total_stake += stake;
            if !reached_validator_before
                && total_stake >= self.context.committee.validity_threshold()
            {
                low = round;
            }
            if !reached_quorum_before && total_stake >= self.context.committee.quorum_threshold() {
                high = round;
            }
        }

        (low, high)
    }
}
