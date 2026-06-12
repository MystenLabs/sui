// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, VecDeque},
    pin::Pin,
    sync::Arc,
    task,
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_types::block::{BlockRef, Round};
use futures::{StreamExt as _, stream};
use parking_lot::RwLock;
use sui_macros::fail_point_async;
use tap::TapFallible;
use tokio::sync::broadcast;

use crate::{
    BlockVerifier, RandomnessSignatureHandler, TransactionVoteTracker,
    authority_service::{BroadcastStream, SubscriptionCounter},
    block::{BlockAPI as _, SignedBlock, VerifiedBlock},
    block_sync_service::BlockSyncService,
    commit::{CommitRange, TrustedCommit},
    commit_vote_monitor::{CommitVoteMonitor, is_commit_lagging},
    context::Context,
    core::AcceptedBlock,
    core_thread::CoreThreadDispatcher,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::{
        NodeId, ObserverBlockStream, ObserverNetworkService, ObserverStreamItem, PeerId,
        observer::AuxiliaryData,
    },
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    synchronizer::SynchronizerHandle,
};

#[allow(dead_code)]
const QUORUM_DELAY_TIMEOUT: Duration = Duration::from_secs(1);

#[allow(dead_code)]
/// A stream adapter that buffers blocks per round and only releases round R blocks once
/// 2f+1 stake worth of blocks have been accepted for that round. This mitigates front-running
/// by ensuring that by the time a subscriber sees round R blocks, the R+1 leader has likely
/// frozen its parent set.
///
/// When round R reaches quorum, all buffered blocks from rounds <= R are released together.
/// If quorum is not reached within [`QUORUM_DELAY_TIMEOUT`], all pending blocks are flushed
/// as a safety valve to prevent the stream from stalling during partitions.
struct QuorumDelayedStream<S> {
    inner: S,
    context: Arc<Context>,
    pending: BTreeMap<Round, RoundPendingState>,
    ready: VecDeque<Vec<AcceptedBlock>>,
    /// Fires when pending blocks have been waiting too long without reaching quorum.
    /// Reset to `None` when the pending map is empty.
    timeout: Option<Pin<Box<tokio::time::Sleep>>>,
}

#[allow(dead_code)]
struct RoundPendingState {
    stake_aggregator: StakeAggregator<QuorumThreshold>,
    blocks: Vec<AcceptedBlock>,
}

#[allow(dead_code)]
impl<S: futures::Stream<Item = Vec<AcceptedBlock>> + Unpin> QuorumDelayedStream<S> {
    fn new(inner: S, context: Arc<Context>) -> Self {
        Self {
            inner,
            context,
            pending: BTreeMap::new(),
            ready: VecDeque::new(),
            timeout: None,
        }
    }

    fn buffer_block(&mut self, accepted_block: AcceptedBlock) {
        let round = accepted_block.block.round();
        let author = accepted_block.block.author();

        // Start the timeout when the first block enters the pending buffer.
        if self.pending.is_empty() {
            self.timeout = Some(Box::pin(tokio::time::sleep(QUORUM_DELAY_TIMEOUT)));
        }

        let state = self
            .pending
            .entry(round)
            .or_insert_with(|| RoundPendingState {
                stake_aggregator: StakeAggregator::new(),
                blocks: Vec::new(),
            });
        state.stake_aggregator.add(author, &self.context.committee);
        state.blocks.push(accepted_block);
    }

    /// Checks all pending rounds for quorum. When any round reaches quorum, releases all
    /// blocks from that round and any earlier rounds.
    fn flush_ready_rounds(&mut self) {
        let committee = &self.context.committee;
        let max_quorum_round = self
            .pending
            .iter()
            .filter(|(_, state)| state.stake_aggregator.reached_threshold(committee))
            .map(|(&round, _)| round)
            .max();

        if let Some(cutoff) = max_quorum_round {
            self.release_rounds_up_to(cutoff);
        }
    }

    /// Drains all pending rounds regardless of quorum status.
    fn flush_all_pending(&mut self) {
        let all = std::mem::take(&mut self.pending);
        for (_round, state) in all {
            self.ready.push_back(state.blocks);
        }
        self.timeout = None;
    }

    /// Releases all pending rounds up to and including `cutoff`.
    fn release_rounds_up_to(&mut self, cutoff: Round) {
        let remaining = self.pending.split_off(&(cutoff + 1));
        let released = std::mem::replace(&mut self.pending, remaining);
        for (_round, state) in released {
            self.ready.push_back(state.blocks);
        }
        if self.pending.is_empty() {
            self.timeout = None;
        }
    }
}

impl<S: futures::Stream<Item = Vec<AcceptedBlock>> + Unpin> futures::Stream
    for QuorumDelayedStream<S>
{
    type Item = Vec<AcceptedBlock>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // Yield any already-ready blocks first.
        if let Some(blocks) = this.ready.pop_front() {
            return task::Poll::Ready(Some(blocks));
        }

        // Poll the inner stream for new blocks.
        loop {
            // Check the timeout before going to sleep: if it has fired, flush everything.
            if let Some(timeout) = this.timeout.as_mut()
                && timeout.as_mut().poll(cx).is_ready()
            {
                this.context
                    .metrics
                    .node_metrics
                    .observer_stream_quorum_delay_timeouts
                    .inc();
                tracing::warn!(
                    pending_rounds = this.pending.len(),
                    "Quorum delay timeout fired, releasing all pending blocks"
                );
                this.flush_all_pending();
                if let Some(blocks) = this.ready.pop_front() {
                    return task::Poll::Ready(Some(blocks));
                }
            }

            match Pin::new(&mut this.inner).poll_next(cx) {
                task::Poll::Ready(Some(blocks)) => {
                    for block in blocks {
                        this.buffer_block(block);
                    }
                    this.flush_ready_rounds();

                    if let Some(blocks) = this.ready.pop_front() {
                        return task::Poll::Ready(Some(blocks));
                    }
                    // No quorum reached yet — keep polling in case more blocks are buffered.
                    continue;
                }
                task::Poll::Ready(None) => return task::Poll::Ready(None),
                task::Poll::Pending => return task::Poll::Pending,
            }
        }
    }
}

/// Serves observer requests from observer or validator peers. It is the server-side
/// counterpart to `ObserverNetworkClient`.
pub(crate) struct ObserverService {
    context: Arc<Context>,
    core_dispatcher: Arc<dyn CoreThreadDispatcher>,
    dag_state: Arc<RwLock<DagState>>,
    rx_accepted_block_broadcast: broadcast::Receiver<AcceptedBlock>,
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
        rx_accepted_block_broadcast: broadcast::Receiver<AcceptedBlock>,
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
        highest_round_per_authority: Vec<u64>,
    ) -> ConsensusResult<ObserverBlockStream> {
        if highest_round_per_authority.len() != self.context.committee.size() {
            return Err(ConsensusError::InvalidSizeOfHighestAcceptedRounds(
                highest_round_per_authority.len(),
                self.context.committee.size(),
            ));
        }

        // Collect all accepted blocks from DagState that the observer hasn't yet seen,
        // sorted by round for consistent ordering.
        let past_blocks = {
            let dag_state = self.dag_state.read();
            let mut past_blocks = Vec::new();

            for (authority, _) in self.context.committee.authorities() {
                let from_round = highest_round_per_authority[authority.value()] as u32 + 1;
                past_blocks.extend(dag_state.get_cached_blocks(authority, from_round));
            }

            past_blocks.sort_unstable_by_key(|b| b.round());
            past_blocks
        };

        // Past blocks from DAG cache have no server-side acceptance timestamp.
        let past_stream =
            stream::iter(
                past_blocks
                    .into_iter()
                    .map(move |block| ObserverStreamItem {
                        blocks: vec![block.serialized().clone()],
                        accepted_timestamps_ms: vec![0],
                        auxiliary_data: Default::default(),
                    }),
            );

        const MAX_BLOCKS_PER_POLL: usize = 20;
        let raw_live_stream = BroadcastStream::<AcceptedBlock>::new(
            PeerId::Observer(Box::new(peer)),
            self.rx_accepted_block_broadcast.resubscribe(),
            MAX_BLOCKS_PER_POLL,
            self.subscription_counter.clone(),
        );
        // TODO: re-enable QuorumDelayedStream once experiments confirm latency tradeoffs.
        // let live_block_stream = QuorumDelayedStream::new(raw_live_stream, self.context.clone())
        let live_block_stream = raw_live_stream.map(|accepted_blocks| {
            let mut blocks = Vec::with_capacity(accepted_blocks.len());
            let mut timestamps = Vec::with_capacity(accepted_blocks.len());
            for ab in accepted_blocks {
                blocks.push(ab.block.serialized().clone());
                timestamps.push(ab.accepted_timestamp_ms);
            }
            ObserverStreamItem {
                blocks,
                accepted_timestamps_ms: timestamps,
                auxiliary_data: Default::default(),
            }
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
                accepted_timestamps_ms: vec![],
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

    fn accepted(block: VerifiedBlock) -> AcceptedBlock {
        AcceptedBlock {
            accepted_timestamp_ms: 0,
            block,
        }
    }

    #[tokio::test]
    async fn test_observer_stream_releases_on_quorum() {
        telemetry_subscribers::init_for_testing();
        let (context, keys) = Context::new_for_test(4);
        let context = Arc::new(context);

        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let (tx_accepted_block, rx_accepted_block) = broadcast::channel::<AcceptedBlock>(100);

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
        let highest_round_per_authority = vec![0u64; context.committee.size()];
        let peer = keys[0].0.public().clone();

        let mut stream = observer_service
            .handle_stream_blocks(peer, highest_round_per_authority)
            .await
            .unwrap();

        // With 4 authorities (f=1), quorum requires 2f+1 = 3 blocks from distinct authorities.
        // Send 3 blocks at round 5 from authorities 0, 1, 2 to reach quorum.
        let block1 = VerifiedBlock::new_for_test(TestBlock::new(5, 0).build());
        let block2 = VerifiedBlock::new_for_test(TestBlock::new(5, 1).build());
        let block3 = VerifiedBlock::new_for_test(TestBlock::new(5, 2).build());

        tx_accepted_block.send(accepted(block1)).unwrap();
        tx_accepted_block.send(accepted(block2)).unwrap();
        tx_accepted_block.send(accepted(block3)).unwrap();

        // All 3 blocks should be released once quorum is reached.
        let mut received_blocks = Vec::new();
        while received_blocks.len() < 3 {
            let item = stream.next().await.unwrap();
            for block_bytes in item.blocks {
                let signed: SignedBlock = bcs::from_bytes(&block_bytes).unwrap();
                received_blocks.push(VerifiedBlock::new_verified(signed, block_bytes));
            }
        }
        assert_eq!(received_blocks.len(), 3);
        for block in &received_blocks {
            assert_eq!(block.round(), 5);
        }
    }

    #[tokio::test]
    async fn test_observer_stream_quorum_releases_earlier_rounds() {
        telemetry_subscribers::init_for_testing();
        let (context, keys) = Context::new_for_test(4);
        let context = Arc::new(context);

        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let (tx_accepted_block, rx_accepted_block) = broadcast::channel::<AcceptedBlock>(100);

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

        let highest_round_per_authority = vec![0u64; context.committee.size()];
        let peer = keys[0].0.public().clone();

        let mut stream = observer_service
            .handle_stream_blocks(peer, highest_round_per_authority)
            .await
            .unwrap();

        // Send 1 block at round 5 (below quorum), then 3 blocks at round 10.
        // When round 10 reaches quorum, the round 5 block should also be released.
        let early_block = VerifiedBlock::new_for_test(TestBlock::new(5, 0).build());
        let block_r10_a = VerifiedBlock::new_for_test(TestBlock::new(10, 0).build());
        let block_r10_b = VerifiedBlock::new_for_test(TestBlock::new(10, 1).build());
        let block_r10_c = VerifiedBlock::new_for_test(TestBlock::new(10, 2).build());

        tx_accepted_block.send(accepted(early_block)).unwrap();
        tx_accepted_block.send(accepted(block_r10_a)).unwrap();
        tx_accepted_block.send(accepted(block_r10_b)).unwrap();
        tx_accepted_block.send(accepted(block_r10_c)).unwrap();

        // Should receive 4 blocks total: 1 from round 5 + 3 from round 10.
        // Round 5 should come first (lower round released before higher round).
        let mut received_blocks = Vec::new();
        while received_blocks.len() < 4 {
            let item = stream.next().await.unwrap();
            for block_bytes in item.blocks {
                let signed: SignedBlock = bcs::from_bytes(&block_bytes).unwrap();
                received_blocks.push(VerifiedBlock::new_verified(signed, block_bytes));
            }
        }
        assert_eq!(received_blocks.len(), 4);
        assert_eq!(received_blocks[0].round(), 5);
        // Remaining blocks are from round 10
        for block in &received_blocks[1..] {
            assert_eq!(block.round(), 10);
        }
    }

    #[tokio::test]
    async fn test_observer_stream_quorum_delay_timeout() {
        telemetry_subscribers::init_for_testing();
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);

        // Use a channel-backed stream to have manual control over block delivery.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Vec<AcceptedBlock>>();
        let inner_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

        let mut stream = QuorumDelayedStream::new(inner_stream, context.clone());

        // Send a single block — not enough for quorum (needs 3 out of 4).
        let block = VerifiedBlock::new_for_test(TestBlock::new(5, 0).build());
        tx.send(vec![accepted(block)]).unwrap();

        // Poll the stream: it should buffer the block but not yield it.
        // Use a short timeout to confirm the stream returns Pending.
        let poll_result =
            tokio::time::timeout(Duration::from_millis(100), StreamExt::next(&mut stream)).await;
        assert!(
            poll_result.is_err(),
            "Stream should not yield before quorum or timeout"
        );

        // Advance time past the quorum delay timeout.
        tokio::time::pause();
        tokio::time::advance(QUORUM_DELAY_TIMEOUT).await;
        tokio::time::resume();

        // Now the stream should yield the buffered block via timeout.
        let item = tokio::time::timeout(Duration::from_millis(100), StreamExt::next(&mut stream))
            .await
            .expect("Stream should yield after timeout")
            .expect("Stream should not be closed");
        assert_eq!(item.len(), 1);
        assert_eq!(item[0].block.round(), 5);

        // Verify the timeout metric was incremented.
        assert_eq!(
            context
                .metrics
                .node_metrics
                .observer_stream_quorum_delay_timeouts
                .get(),
            1
        );
    }

    #[tokio::test]
    async fn test_observer_stream_invalid_input() {
        telemetry_subscribers::init_for_testing();
        let (context, keys) = Context::new_for_test(4);
        let context = Arc::new(context);

        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let (_tx_accepted_block, rx_accepted_block) = broadcast::channel::<AcceptedBlock>(100);

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
        let invalid_highest_rounds = vec![0u64; 10]; // Wrong size, should be 4
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
}
