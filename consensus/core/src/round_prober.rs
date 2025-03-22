// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! RoundProber periodically checks each peer for the latest rounds they received and accepted
//! from others. This provides insight into how effectively each authority's blocks are propagated
//! and accepted across the network.
//!
//! Unlike inferring accepted rounds from the DAG of each block, RoundProber has the benefit that
//! it remains active even when peers are not proposing. This makes it essential for determining
//! when to disable optimizations that improve DAG quality but may compromise liveness.
//!
//! RoundProber's data sources include the `highest_received_rounds` & `highest_accepted_rounds` tracked
//! by the CoreThreadDispatcher and DagState. The received rounds are updated after blocks are verified
//! but before checking for dependencies. This should make the values more indicative of how well authorities
//! propagate blocks, and less influenced by the quality of ancestors in the proposed blocks. The
//! accepted rounds are updated after checking for dependencies which should indicate the quality
//! of the proposed blocks including its ancestors.

use std::{sync::Arc, time::Duration};

use futures::stream::{FuturesUnordered, StreamExt as _};
use mysten_common::sync::notify_once::NotifyOnce;
use mysten_metrics::monitored_scope;
use parking_lot::RwLock;
use tokio::{task::JoinHandle, time::MissedTickBehavior};

use crate::{
    context::Context, core_thread::CoreThreadDispatcher, dag_state::DagState,
    network::NetworkClient, round_tracker::PeerRoundTracker, BlockAPI as _, Round,
};

// Handle to control the RoundProber loop and read latest round gaps.
pub(crate) struct RoundProberHandle {
    prober_task: JoinHandle<()>,
    shutdown_notify: Arc<NotifyOnce>,
}

impl RoundProberHandle {
    pub(crate) async fn stop(self) {
        let _ = self.shutdown_notify.notify();
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
    round_tracker: Arc<RwLock<PeerRoundTracker>>,
    dag_state: Arc<RwLock<DagState>>,
    network_client: Arc<C>,
    shutdown_notify: Arc<NotifyOnce>,
}

impl<C: NetworkClient> RoundProber<C> {
    pub(crate) fn new(
        context: Arc<Context>,
        core_thread_dispatcher: Arc<dyn CoreThreadDispatcher>,
        round_tracker: Arc<RwLock<PeerRoundTracker>>,
        dag_state: Arc<RwLock<DagState>>,
        network_client: Arc<C>,
    ) -> Self {
        Self {
            context,
            core_thread_dispatcher,
            round_tracker,
            dag_state,
            network_client,
            shutdown_notify: Arc::new(NotifyOnce::new()),
        }
    }

    pub(crate) fn start(self) -> RoundProberHandle {
        let shutdown_notify = self.shutdown_notify.clone();
        let loop_shutdown_notify = shutdown_notify.clone();
        let prober_task = tokio::spawn(async move {
            // With 200 validators, this would result in 200 * 4 * 200 / 2 = 80KB of additional
            // bandwidth usage per sec. We can consider using adaptive intervals, for example
            // 10s by default but reduced to 2s when the propagation delay is higher.
            let mut interval = tokio::time::interval(Duration::from_millis(
                self.context.parameters.round_prober_interval_ms,
            ));
            interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        self.probe().await;
                    }
                    _ = loop_shutdown_notify.wait() => {
                        break;
                    }
                }
            }
        });
        RoundProberHandle {
            prober_task,
            shutdown_notify,
        }
    }

    // Probes each peer for the latest rounds they received from others.
    // Returns the propagation delay of own blocks.
    pub(crate) async fn probe(&self) -> Round {
        let _scope = monitored_scope("RoundProber");

        let node_metrics = &self.context.metrics.node_metrics;
        let request_timeout =
            Duration::from_millis(self.context.parameters.round_prober_request_timeout_ms);
        let own_index = self.context.own_index;
        let mut requests = FuturesUnordered::new();

        for (peer, _) in self.context.committee.authorities() {
            if peer == own_index {
                continue;
            }
            let network_client = self.network_client.clone();
            requests.push(async move {
                let result = tokio::time::timeout(
                    request_timeout,
                    network_client.get_latest_rounds(peer, request_timeout),
                )
                .await;
                (peer, result)
            });
        }

        let mut highest_received_rounds =
            vec![vec![0; self.context.committee.size()]; self.context.committee.size()];
        let mut highest_accepted_rounds =
            vec![vec![0; self.context.committee.size()]; self.context.committee.size()];

        let blocks = self
            .dag_state
            .read()
            .get_last_cached_block_per_authority(Round::MAX);
        let local_highest_accepted_rounds = blocks
            .into_iter()
            .map(|(block, _)| block.round())
            .collect::<Vec<_>>();
        let last_proposed_round = local_highest_accepted_rounds[own_index];

        // For our own index, the highest received & accepted round is our last
        // accepted round or our last proposed round.
        highest_received_rounds[own_index] = self.core_thread_dispatcher.highest_received_rounds();
        highest_accepted_rounds[own_index] = local_highest_accepted_rounds;
        highest_received_rounds[own_index][own_index] = last_proposed_round;
        highest_accepted_rounds[own_index][own_index] = last_proposed_round;

        loop {
            tokio::select! {
                result = requests.next() => {
                    let Some((peer, result)) = result else { break };
                    let peer_name = &self.context.committee.authority(peer).hostname;
                    match result {
                        Ok(Ok((received, accepted))) => {
                            if received.len() == self.context.committee.size()
                            {
                                highest_received_rounds[peer] = received;
                            } else {
                                node_metrics.round_prober_request_errors.with_label_values(&["invalid_received_rounds"]).inc();
                                tracing::warn!("Received invalid number of received rounds from peer {}", peer_name);
                            }

                            if accepted.len() == self.context.committee.size() {
                                highest_accepted_rounds[peer] = accepted;
                            } else {
                                node_metrics.round_prober_request_errors.with_label_values(&["invalid_accepted_rounds"]).inc();
                                tracing::warn!("Received invalid number of accepted rounds from peer {}", peer_name);
                            }
                        },
                        // When a request fails, the highest received rounds from that authority will be 0
                        // for the subsequent computations.
                        // For propagation delay, this behavior is desirable because the computed delay
                        // increases as this authority has more difficulty communicating with peers. Logic
                        // triggered by high delay should usually be triggered with frequent probing failures
                        // as well.
                        // For quorum rounds computed for peer, this means the values should be used for
                        // positive signals (peer A can propagate its blocks well) rather than negative signals
                        // (peer A cannot propagate its blocks well). It can be difficult to distinguish between
                        // own probing failures and actual propagation issues.
                        Ok(Err(err)) => {
                            node_metrics.round_prober_request_errors.with_label_values(&["failed_fetch"]).inc();
                            tracing::debug!("Failed to get latest rounds from peer {}: {:?}", peer_name, err);
                        },
                        Err(_) => {
                            node_metrics.round_prober_request_errors.with_label_values(&["timeout"]).inc();
                            tracing::debug!("Timeout while getting latest rounds from peer {}", peer_name);
                        },
                    }
                }
                _ = self.shutdown_notify.wait() => break,
            }
        }

        self.round_tracker
            .write()
            .update_from_probe(highest_accepted_rounds, highest_received_rounds);
        let propagation_delay = self
            .round_tracker
            .read()
            .calculate_propagation_delay(last_proposed_round);

        let _ = self
            .core_thread_dispatcher
            .set_propagation_delay(propagation_delay);

        propagation_delay
    }
}

#[cfg(test)]
mod test {
    use std::{collections::BTreeSet, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_config::AuthorityIndex;
    use parking_lot::RwLock;

    use crate::{
        block::BlockRef,
        commit::{CertifiedCommits, CommitRange},
        context::Context,
        core_thread::{CoreError, CoreThreadDispatcher},
        dag_state::DagState,
        error::{ConsensusError, ConsensusResult},
        network::{BlockStream, NetworkClient},
        round_prober::RoundProber,
        round_tracker::PeerRoundTracker,
        storage::mem_store::MemStore,
        Round, TestBlock, VerifiedBlock,
    };

    struct FakeThreadDispatcher {
        highest_received_rounds: Vec<Round>,
    }

    impl FakeThreadDispatcher {
        fn new(highest_received_rounds: Vec<Round>) -> Self {
            Self {
                highest_received_rounds,
            }
        }
    }

    #[async_trait]
    impl CoreThreadDispatcher for FakeThreadDispatcher {
        async fn add_blocks(
            &self,
            _blocks: Vec<VerifiedBlock>,
        ) -> Result<BTreeSet<BlockRef>, CoreError> {
            unimplemented!()
        }

        async fn check_block_refs(
            &self,
            _block_refs: Vec<BlockRef>,
        ) -> Result<BTreeSet<BlockRef>, CoreError> {
            unimplemented!()
        }

        async fn add_certified_commits(
            &self,
            _commits: CertifiedCommits,
        ) -> Result<BTreeSet<BlockRef>, CoreError> {
            unimplemented!()
        }

        async fn new_block(&self, _round: Round, _force: bool) -> Result<(), CoreError> {
            unimplemented!()
        }

        async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError> {
            unimplemented!()
        }

        fn set_subscriber_exists(&self, _exists: bool) -> Result<(), CoreError> {
            unimplemented!()
        }

        fn set_propagation_delay(&self, _propagation_delay: Round) -> Result<(), CoreError> {
            Ok(())
        }

        fn set_last_known_proposed_round(&self, _round: Round) -> Result<(), CoreError> {
            unimplemented!()
        }

        fn highest_received_rounds(&self) -> Vec<Round> {
            self.highest_received_rounds.clone()
        }
    }

    struct FakeNetworkClient {
        highest_received_rounds: Vec<Vec<Round>>,
        highest_accepted_rounds: Vec<Vec<Round>>,
    }

    impl FakeNetworkClient {
        fn new(
            highest_received_rounds: Vec<Vec<Round>>,
            highest_accepted_rounds: Vec<Vec<Round>>,
        ) -> Self {
            Self {
                highest_received_rounds,
                highest_accepted_rounds,
            }
        }
    }

    #[async_trait]
    #[async_trait::async_trait]
    impl NetworkClient for FakeNetworkClient {
        const SUPPORT_STREAMING: bool = true;

        async fn send_block(
            &self,
            _peer: AuthorityIndex,
            _serialized_block: &VerifiedBlock,
            _timeout: Duration,
        ) -> ConsensusResult<()> {
            unimplemented!("Unimplemented")
        }

        async fn subscribe_blocks(
            &self,
            _peer: AuthorityIndex,
            _last_received: Round,
            _timeout: Duration,
        ) -> ConsensusResult<BlockStream> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_blocks(
            &self,
            _peer: AuthorityIndex,
            _block_refs: Vec<BlockRef>,
            _highest_accepted_rounds: Vec<Round>,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_commits(
            &self,
            _peer: AuthorityIndex,
            _commit_range: CommitRange,
            _timeout: Duration,
        ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_latest_blocks(
            &self,
            _peer: AuthorityIndex,
            _authorities: Vec<AuthorityIndex>,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }

        async fn get_latest_rounds(
            &self,
            peer: AuthorityIndex,
            _timeout: Duration,
        ) -> ConsensusResult<(Vec<Round>, Vec<Round>)> {
            let received_rounds = self.highest_received_rounds[peer].clone();
            let accepted_rounds = self.highest_accepted_rounds[peer].clone();
            if received_rounds.is_empty() && accepted_rounds.is_empty() {
                Err(ConsensusError::NetworkRequestTimeout("test".to_string()))
            } else {
                Ok((received_rounds, accepted_rounds))
            }
        }
    }

    #[tokio::test]
    async fn test_round_prober() {
        telemetry_subscribers::init_for_testing();
        const NUM_AUTHORITIES: usize = 7;
        let context = Arc::new(Context::new_for_test(NUM_AUTHORITIES).0);
        let core_thread_dispatcher = Arc::new(FakeThreadDispatcher::new(vec![
            110, 120, 130, 140, 150, 160, 170,
        ]));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        // Have some peers return error or incorrect number of rounds.
        let network_client = Arc::new(FakeNetworkClient::new(
            vec![
                vec![],
                vec![109, 121, 131, 0, 151, 161, 171],
                vec![101, 0, 103, 104, 105, 166, 107],
                vec![],
                vec![100, 102, 133, 0, 155, 106, 177],
                vec![105, 115, 103, 0, 125, 126, 127],
                vec![10, 20, 30, 40, 50, 60],
            ], // highest_received_rounds
            vec![
                vec![],
                vec![0, 121, 131, 0, 151, 161, 171],
                vec![1, 0, 103, 104, 105, 166, 107],
                vec![],
                vec![0, 102, 133, 0, 155, 106, 177],
                vec![1, 115, 103, 0, 125, 126, 127],
                vec![1, 20, 30, 40, 50, 60],
            ], // highest_accepted_rounds
        ));

        let round_tracker = Arc::new(RwLock::new(PeerRoundTracker::new(context.clone())));
        let prober = RoundProber::new(
            context.clone(),
            core_thread_dispatcher.clone(),
            round_tracker.clone(),
            dag_state.clone(),
            network_client.clone(),
        );

        // Create test blocks for each authority with incrementing rounds starting at 110
        let blocks = (0..NUM_AUTHORITIES)
            .map(|authority| {
                let round = 110 + (authority as u32 * 10);
                VerifiedBlock::new_for_test(TestBlock::new(round, authority as u32).build())
            })
            .collect::<Vec<_>>();

        dag_state.write().accept_blocks(blocks);

        // Compute quorum rounds and propagation delay based on last proposed round = 110,
        // and highest received rounds:
        // 110, 120, 130, 140, 150, 160, 170,
        // 109, 121, 131, 0,   151, 161, 171,
        // 101, 0,   103, 104, 105, 166, 107,
        // 0,   0,   0,   0,   0,   0,   0,
        // 100, 102, 133, 0,   155, 106, 177,
        // 105, 115, 103, 0,   125, 126, 127,
        // 0,   0,   0,   0,   0,   0,   0,

        let propagation_delay = prober.probe().await;

        assert_eq!(propagation_delay, 10);
    }
}
