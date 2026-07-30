// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::VecDeque,
    sync::{Arc, Weak},
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_types::block::{BlockRef, Round};
use futures::{StreamExt as _, stream};
use mysten_common::ZipDebugEqIteratorExt;
use parking_lot::RwLock;
use prometheus::{IntCounter, IntGauge};
use sui_macros::fail_point_async;
use tap::TapFallible;
use tokio::sync::broadcast;

use crate::{
    BlockVerifier, CommitIndex, RandomnessSignatureHandler, TransactionVoteTracker,
    authority_service::{BroadcastStream, SubscriptionCounter},
    block::{BlockAPI as _, SignedBlock, VerifiedBlock},
    block_sync_service::{BlockSyncService, CommitWindow},
    commit::{CommitAPI as _, CommitRange, TrustedCommit},
    commit_vote_monitor::{CommitVoteMonitor, is_commit_lagging},
    context::Context,
    core_thread::CoreThreadDispatcher,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::{
        CommitStreamItem, NodeId, ObserverBlockStream, ObserverCommitStream,
        ObserverNetworkService, ObserverStreamItem, PeerId, observer::AuxiliaryData,
        tonic_network::MAX_FETCH_RESPONSE_BYTES,
    },
    synchronizer::SynchronizerHandle,
};

/// Serves observer requests from observer or validator peers. It is the server-side
/// counterpart to `ObserverNetworkClient`.
pub(crate) struct ObserverService {
    context: Arc<Context>,
    core_dispatcher: Arc<dyn CoreThreadDispatcher>,
    dag_state: Arc<RwLock<DagState>>,
    rx_accepted_block_broadcast: broadcast::Receiver<VerifiedBlock>,
    subscription_counter: Arc<SubscriptionCounter>,
    block_verifier: Arc<dyn BlockVerifier>,
    commit_vote_monitor: Arc<CommitVoteMonitor>,
    transaction_vote_tracker: TransactionVoteTracker,
    synchronizer: Arc<SynchronizerHandle>,
    block_sync_service: Arc<BlockSyncService>,
    randomness_signature_handler: Option<Arc<dyn RandomnessSignatureHandler>>,
}

impl ObserverService {
    pub(crate) fn new(
        context: Arc<Context>,
        core_dispatcher: Arc<dyn CoreThreadDispatcher>,
        dag_state: Arc<RwLock<DagState>>,
        rx_accepted_block_broadcast: broadcast::Receiver<VerifiedBlock>,
        block_verifier: Arc<dyn BlockVerifier>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        transaction_vote_tracker: TransactionVoteTracker,
        synchronizer: Arc<SynchronizerHandle>,
        block_sync_service: Arc<BlockSyncService>,
        randomness_signature_handler: Option<Arc<dyn RandomnessSignatureHandler>>,
    ) -> Self {
        let subscription_counter = Arc::new(SubscriptionCounter::new(context.clone()));
        Self {
            context,
            core_dispatcher,
            dag_state,
            rx_accepted_block_broadcast,
            subscription_counter,
            block_verifier,
            commit_vote_monitor,
            transaction_vote_tracker,
            synchronizer,
            block_sync_service,
            randomness_signature_handler,
        }
    }
}

#[async_trait]
impl ObserverNetworkService for ObserverService {
    async fn handle_block(&self, peer: PeerId, block: Bytes) -> ConsensusResult<()> {
        fail_point_async!("consensus-rpc-response");

        // TODO: dedup block verifications, here and with fetched blocks.
        let signed_block: SignedBlock =
            bcs::from_bytes(&block).map_err(ConsensusError::MalformedBlock)?;

        // Create owned strings for observer peer names to avoid borrowing issues
        let observer_name;
        let peer_name = match &peer {
            PeerId::Validator(authority) => self
                .context
                .committee
                .authority(*authority)
                .hostname
                .as_str(),
            PeerId::Observer(node_id) => {
                observer_name = format!("{:?}", node_id);
                observer_name.as_str()
            }
        };

        // Reject blocks failing parsing and validations.
        // Of Observer nodes we don't care about the transaction votes.
        let (verified_block, _reject_txn_votes) = self
            .block_verifier
            .verify_and_vote(signed_block, block)
            .tap_err(|e| {
                self.context
                    .metrics
                    .node_metrics
                    .invalid_blocks
                    .with_label_values(&[peer_name, "handle_send_block", e.name()])
                    .inc();
                tracing::info!("Invalid block from {}: {}", peer.clone(), e);
            })?;

        let block_author_hostname = &self
            .context
            .committee
            .authority(verified_block.author())
            .hostname;
        let block_ref = verified_block.reference();
        tracing::debug!("Received block {} via send block.", block_ref);

        self.context
            .metrics
            .node_metrics
            .verified_blocks
            .with_label_values(&[block_author_hostname])
            .inc();

        let now = self.context.clock.timestamp_utc_ms();
        let forward_time_drift =
            Duration::from_millis(verified_block.timestamp_ms().saturating_sub(now));

        self.context
            .metrics
            .node_metrics
            .block_timestamp_drift_ms
            .with_label_values(&[block_author_hostname.as_str(), "handle_send_block"])
            .inc_by(forward_time_drift.as_millis() as u64);

        // Observe the block for the commit votes. When local commit is lagging too much,
        // commit sync loop will trigger fetching.
        self.commit_vote_monitor.observe_block(&verified_block);

        // Reject blocks when local commit index is lagging too far from quorum commit index,
        // to avoid the memory overhead from suspended blocks.
        //
        // IMPORTANT: this must be done after observing votes from the block, otherwise
        // observed quorum commit will no longer progress.
        //
        // Since the main issue with too many suspended blocks is memory usage not CPU,
        // it is ok to reject after block verifications instead of before.
        let last_commit_index = self.dag_state.read().last_commit_index();
        let quorum_commit_index = self.commit_vote_monitor.quorum_commit_index();
        if is_commit_lagging(
            self.context.as_ref(),
            last_commit_index,
            quorum_commit_index,
        ) {
            self.context
                .metrics
                .node_metrics
                .rejected_blocks
                .with_label_values(&["commit_lagging"])
                .inc();
            tracing::debug!(
                "Block {:?} is rejected because last commit index is lagging quorum commit index too much ({} < {})",
                block_ref,
                last_commit_index,
                quorum_commit_index,
            );
            return Err(ConsensusError::BlockRejected {
                block_ref,
                reason: format!(
                    "Last commit index is lagging quorum commit index too much ({} < {})",
                    last_commit_index, quorum_commit_index,
                ),
            });
        }

        // Add the block to the transaction vote tracker. No "own" votes are recorded for observer nodes.
        if self.context.protocol_config.transaction_voting_enabled() {
            self.transaction_vote_tracker
                .add_voted_blocks(vec![(verified_block.clone(), vec![])]);
        }

        // Send the block to Core to try accepting it into the DAG.
        let missing_ancestors = self
            .core_dispatcher
            .add_blocks(vec![verified_block.clone()])
            .await
            .map_err(|_| ConsensusError::Shutdown)?;

        // Schedule fetching missing ancestors from this peer in the background.
        if !missing_ancestors.is_empty() {
            self.context
                .metrics
                .node_metrics
                .handler_received_block_missing_ancestors
                .with_label_values(&[block_author_hostname])
                .inc_by(missing_ancestors.len() as u64);

            let synchronizer = self.synchronizer.clone();
            mysten_metrics::spawn_monitored_task!(async move {
                // This does not wait for the fetch request to complete.
                // It only waits for synchronizer to queue the request to a peer.
                // When this fails, it usually means the queue is full.
                // The fetch will retry from other peers via live and periodic syncs.
                if let Err(err) = synchronizer.fetch_blocks(missing_ancestors, peer).await {
                    tracing::debug!("Failed to fetch missing ancestors via synchronizer: {err}");
                }
            });
        }

        Ok(())
    }

    async fn handle_stream_blocks(
        &self,
        peer: NodeId,
        highest_round_per_authority: Vec<Round>,
    ) -> ConsensusResult<ObserverBlockStream> {
        if highest_round_per_authority.len() != self.context.committee.size() {
            return Err(ConsensusError::InvalidSizeOfHighestAcceptedRounds(
                highest_round_per_authority.len(),
                self.context.committee.size(),
            ));
        }

        // Subscribe before snapshotting past blocks below. This can duplicate
        // a block in both the subscription stream and snapshot, which is fine.
        // Otherwise, it is possible to miss a block if it is broadcasted after snapshotting
        // but before subscribing.
        let broadcast_rx = self.rx_accepted_block_broadcast.resubscribe();

        // Collect all accepted blocks from DagState that the observer hasn't yet seen,
        // sorted by round for consistent ordering.
        let past_blocks = {
            let dag_state = self.dag_state.read();
            let mut past_blocks = Vec::new();

            for (authority, _) in self.context.committee.authorities() {
                // Saturate so an out-of-range round from the peer cannot wrap to 0 and
                // replay the entire block cache.
                let from_round = highest_round_per_authority[authority.value()].saturating_add(1);
                past_blocks.extend(dag_state.get_cached_blocks(authority, from_round));
            }

            past_blocks.sort_unstable_by_key(|b| b.round());
            past_blocks
        };

        let past_stream =
            stream::iter(
                past_blocks
                    .into_iter()
                    .map(move |block| ObserverStreamItem {
                        blocks: vec![block.serialized().clone()],
                        auxiliary_data: Default::default(),
                    }),
            );

        const MAX_BLOCKS_PER_POLL: usize = 20;
        let live_block_stream = BroadcastStream::<VerifiedBlock>::new(
            PeerId::Observer(Box::new(peer)),
            broadcast_rx,
            MAX_BLOCKS_PER_POLL,
            self.subscription_counter.clone(),
        )
        .map(|blocks| ObserverStreamItem {
            blocks: blocks
                .into_iter()
                .map(|block| block.serialized().clone())
                .collect(),
            auxiliary_data: Default::default(),
        });

        let block_stream = past_stream.chain(live_block_stream);

        // Merge randomness signature broadcast into the block stream when a handler is available.
        if let Some(handler) = &self.randomness_signature_handler {
            const MAX_SIGNATURES_PER_POLL: usize = 20;
            let sig_stream = BroadcastStream::new_untracked(
                handler.subscribe_randomness_signatures(),
                MAX_SIGNATURES_PER_POLL,
            )
            .map(|sigs| ObserverStreamItem {
                blocks: vec![],
                auxiliary_data: AuxiliaryData {
                    randomness_signatures: sigs,
                },
            });
            Ok(Box::pin(futures::stream::select(block_stream, sig_stream)))
        } else {
            Ok(Box::pin(block_stream))
        }
    }

    async fn handle_fetch_blocks(
        &self,
        _peer: NodeId,
        block_refs: Vec<BlockRef>,
        fetch_after_rounds: Vec<Round>,
        fetch_missing_ancestors: bool,
    ) -> ConsensusResult<Vec<Bytes>> {
        fail_point_async!("consensus-rpc-response");

        // Delegate to BlockSyncService
        self.block_sync_service
            .fetch_blocks(block_refs, fetch_after_rounds, fetch_missing_ancestors)
            .await
    }

    async fn handle_fetch_commits(
        &self,
        _peer: NodeId,
        commit_range: CommitRange,
    ) -> ConsensusResult<(Vec<TrustedCommit>, Vec<VerifiedBlock>)> {
        fail_point_async!("consensus-rpc-response");

        // Delegate to BlockSyncService
        self.block_sync_service.fetch_commits(commit_range).await
    }

    async fn handle_stream_commits(
        &self,
        peer: NodeId,
        start: CommitIndex,
    ) -> ConsensusResult<ObserverCommitStream> {
        // The stream can outlive the observer service on the network's schedule during
        // shutdown, so it must not hold the block sync service (and transitively DagState
        // via ObserverService) strongly.
        let block_sync_service = Arc::downgrade(&self.block_sync_service);
        let node_metrics = &self.context.metrics.node_metrics;
        node_metrics.commit_stream_active_streams.inc();

        struct StreamState {
            block_sync_service: Weak<BlockSyncService>,
            peer: NodeId,
            next_start: CommitIndex,
            pending: VecDeque<CommitStreamItem>,
            served_commits: IntCounter,
            served_bytes: IntCounter,
            // Decrements commit_stream_active_streams when the stream is dropped.
            _active_guard: ActiveStreamGuard,
        }

        let state = StreamState {
            block_sync_service,
            peer,
            next_start: start,
            pending: VecDeque::new(),
            served_commits: node_metrics.commit_stream_served_commits.clone(),
            served_bytes: node_metrics.commit_stream_served_bytes.clone(),
            _active_guard: ActiveStreamGuard {
                gauge: node_metrics.commit_stream_active_streams.clone(),
            },
        };

        let stream = stream::unfold(state, |mut state| async move {
            loop {
                if let Some(item) = state.pending.pop_front() {
                    return Some((item, state));
                }
                let block_sync_service = state.block_sync_service.upgrade()?;
                let window = match block_sync_service
                    .serve_commit_window(state.next_start)
                    .await
                {
                    Ok(Some(window)) => window,
                    // The peer has caught up with the local certified commit tip.
                    Ok(None) => return None,
                    Err(e) => {
                        tracing::debug!(
                            "Ending commit stream to peer {:?} at index {}: {}",
                            state.peer,
                            state.next_start,
                            e
                        );
                        return None;
                    }
                };
                // Commits are only served with certifier blocks proving quorum on the
                // window's last commit; without them the client cannot verify the window.
                if window.certifier_blocks.is_empty() {
                    return None;
                }
                state.next_start = window
                    .commits
                    .last()
                    .expect("Commit window is never empty")
                    .index()
                    + 1;
                state.served_commits.inc_by(window.commits.len() as u64);
                state.pending = chunk_commit_window(window, MAX_FETCH_RESPONSE_BYTES);
                let bytes: usize = state
                    .pending
                    .iter()
                    .map(|item| {
                        item.commits
                            .iter()
                            .chain(item.blocks.iter())
                            .chain(item.certifier_blocks.iter())
                            .map(|b| b.len())
                            .sum::<usize>()
                    })
                    .sum();
                state.served_bytes.inc_by(bytes as u64);
            }
        });

        Ok(Box::pin(stream))
    }
}

struct ActiveStreamGuard {
    gauge: IntGauge,
}

impl Drop for ActiveStreamGuard {
    fn drop(&mut self) {
        self.gauge.dec();
    }
}

/// Splits a commit window into stream items under a soft per-item byte budget, keeping
/// each commit and its blocks in the same item. The certifier blocks are attached only
/// to the last item, marking the end of the verification window for the client.
fn chunk_commit_window(window: CommitWindow, max_bytes: usize) -> VecDeque<CommitStreamItem> {
    let mut items = VecDeque::new();
    let mut current = CommitStreamItem {
        commits: vec![],
        block_counts: vec![],
        blocks: vec![],
        certifier_blocks: vec![],
    };
    let mut current_bytes = 0usize;
    for (commit, blocks) in window.commits.into_iter().zip_debug_eq(window.blocks) {
        let entry_bytes =
            commit.serialized().len() + blocks.iter().map(|b| b.serialized().len()).sum::<usize>();
        if !current.commits.is_empty() && current_bytes + entry_bytes > max_bytes {
            items.push_back(current);
            current = CommitStreamItem {
                commits: vec![],
                block_counts: vec![],
                blocks: vec![],
                certifier_blocks: vec![],
            };
            current_bytes = 0;
        }
        current.commits.push(commit.serialized().clone());
        current.block_counts.push(blocks.len() as u32);
        current
            .blocks
            .extend(blocks.iter().map(|b| b.serialized().clone()));
        current_bytes += entry_bytes;
    }
    current.certifier_blocks = window
        .certifier_blocks
        .iter()
        .map(|b| b.serialized().clone())
        .collect();
    items.push_back(current);
    items
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::StreamExt;
    use parking_lot::RwLock;
    use tokio::sync::broadcast;

    use super::*;
    use crate::{
        block::{TestBlock, VerifiedBlock},
        block_verifier::NoopBlockVerifier,
        commit_vote_monitor::CommitVoteMonitor,
        context::Context,
        core_thread::MockCoreThreadDispatcher,
        storage::mem_store::MemStore,
    };

    // Helper function to create a mock synchronizer for tests
    fn create_mock_synchronizer() -> Arc<SynchronizerHandle> {
        SynchronizerHandle::new_for_test()
    }

    #[tokio::test]
    async fn test_observer_stream_receives_broadcast_blocks() {
        telemetry_subscribers::init_for_testing();
        let (context, keys) = Context::new_for_test(4);
        let context = Arc::new(context);

        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let (tx_accepted_block, rx_accepted_block) = broadcast::channel::<VerifiedBlock>(100);

        // Create mock dependencies
        let core_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let block_verifier = Arc::new(NoopBlockVerifier);
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());

        let block_sync_service = Arc::new(BlockSyncService::new(
            context.clone(),
            dag_state.clone(),
            store.clone(),
        ));
        let observer_service = ObserverService::new(
            context.clone(),
            core_dispatcher,
            dag_state,
            rx_accepted_block,
            block_verifier,
            commit_vote_monitor,
            transaction_vote_tracker,
            create_mock_synchronizer(),
            block_sync_service,
            None,
        );

        // Observer starts with no blocks seen
        let highest_round_per_authority = vec![0 as Round; context.committee.size()];
        let peer = keys[0].0.public().clone();

        let mut stream = observer_service
            .handle_stream_blocks(peer, highest_round_per_authority)
            .await
            .unwrap();

        // Broadcast three blocks
        let block1 = VerifiedBlock::new_for_test(TestBlock::new(5, 0).build());
        let block2 = VerifiedBlock::new_for_test(TestBlock::new(10, 1).build());
        let block3 = VerifiedBlock::new_for_test(TestBlock::new(15, 2).build());

        tx_accepted_block.send(block1.clone()).unwrap();
        tx_accepted_block.send(block2.clone()).unwrap();
        tx_accepted_block.send(block3.clone()).unwrap();

        // Verify observer receives all three blocks in order.
        // Collect all blocks from the batched stream.
        let mut received_blocks = Vec::new();
        while received_blocks.len() < 3 {
            let item = stream.next().await.unwrap();
            for block_bytes in item.blocks {
                let signed: SignedBlock = bcs::from_bytes(&block_bytes).unwrap();
                received_blocks.push(VerifiedBlock::new_verified(signed, block_bytes));
            }
        }
        assert_eq!(received_blocks[0].round(), 5);
        assert_eq!(received_blocks[0].author().value(), 0);
        assert_eq!(received_blocks[1].round(), 10);
        assert_eq!(received_blocks[1].author().value(), 1);
        assert_eq!(received_blocks[2].round(), 15);
        assert_eq!(received_blocks[2].author().value(), 2);
    }

    #[tokio::test]
    async fn test_observer_stream_invalid_input() {
        telemetry_subscribers::init_for_testing();
        let (context, keys) = Context::new_for_test(4);
        let context = Arc::new(context);

        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let (_tx_accepted_block, rx_accepted_block) = broadcast::channel::<VerifiedBlock>(100);

        // Create mock dependencies
        let core_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let block_verifier = Arc::new(NoopBlockVerifier);
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());

        let block_sync_service = Arc::new(BlockSyncService::new(
            context.clone(),
            dag_state.clone(),
            store.clone(),
        ));
        let observer_service = ObserverService::new(
            context.clone(),
            core_dispatcher,
            dag_state,
            rx_accepted_block,
            block_verifier,
            commit_vote_monitor,
            transaction_vote_tracker,
            create_mock_synchronizer(),
            block_sync_service,
            None,
        );

        let peer = keys[0].0.public().clone();

        // Test with wrong size of highest_round_per_authority
        let invalid_highest_rounds = vec![0 as Round; 10]; // Wrong size, should be 4
        let result = observer_service
            .handle_stream_blocks(peer, invalid_highest_rounds)
            .await;

        match result {
            Err(ConsensusError::InvalidSizeOfHighestAcceptedRounds(provided, expected)) => {
                assert_eq!(provided, 10);
                assert_eq!(expected, context.committee.size());
            }
            Err(e) => panic!(
                "Expected InvalidSizeOfHighestAcceptedRounds error, got: {:?}",
                e
            ),
            Ok(_) => panic!("Expected error, got Ok"),
        }
    }

    #[tokio::test]
    async fn test_chunk_commit_window() {
        use crate::{
            block::TestBlock,
            commit::{CommitDigest, TrustedCommit},
        };

        // Build a window of 4 commits, each with 2 blocks.
        let mut commits = vec![];
        let mut blocks = vec![];
        let mut previous_digest = CommitDigest::MIN;
        for index in 1u32..=4 {
            let commit_blocks = (0..2)
                .map(|author| VerifiedBlock::new_for_test(TestBlock::new(index, author).build()))
                .collect::<Vec<_>>();
            let commit = TrustedCommit::new_for_test(
                index,
                previous_digest,
                0,
                commit_blocks[0].reference(),
                commit_blocks.iter().map(|b| b.reference()).collect(),
            );
            previous_digest = commit.digest();
            commits.push(commit);
            blocks.push(commit_blocks);
        }
        let certifier_blocks = vec![VerifiedBlock::new_for_test(TestBlock::new(5, 0).build())];
        let entry_bytes = commits[0].serialized().len()
            + blocks[0]
                .iter()
                .map(|b| b.serialized().len())
                .sum::<usize>();
        let window = CommitWindow {
            commits,
            blocks,
            certifier_blocks,
        };

        // Budget fits two commits per item: expect 2 items of 2 commits each.
        let items = chunk_commit_window(window, entry_bytes * 2);
        assert_eq!(items.len(), 2);
        for item in &items {
            assert_eq!(item.commits.len(), 2);
            assert_eq!(item.block_counts, vec![2, 2]);
            assert_eq!(item.blocks.len(), 4);
        }
        // Certifier blocks only on the last item.
        assert!(items[0].certifier_blocks.is_empty());
        assert_eq!(items[1].certifier_blocks.len(), 1);
    }
}
