// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::Committee;
use consensus_types::block::{BlockRef, Round};
use futures::{StreamExt as _, stream};
use parking_lot::RwLock;
use sui_macros::fail_point_async;
use tap::TapFallible;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    BlockVerifier, RandomnessSignatureHandler, TransactionVoteTracker,
    authority_service::{BroadcastStream, SubscriptionCounter},
    block::{BlockAPI as _, SignedBlock, VerifiedBlock},
    block_sync_service::BlockSyncService,
    commit::{CommitRange, TrustedCommit},
    commit_vote_monitor::{CommitVoteMonitor, is_commit_lagging},
    context::Context,
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
        let live_block_stream = quorum_gated_accepted_block_stream(
            self.context.clone(),
            BroadcastStream::<VerifiedBlock>::new(
                PeerId::Observer(Box::new(peer)),
                broadcast_rx,
                MAX_BLOCKS_PER_POLL,
                self.subscription_counter.clone(),
            ),
            MAX_BLOCKS_PER_POLL,
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
}

fn quorum_gated_accepted_block_stream(
    context: Arc<Context>,
    mut source: BroadcastStream<VerifiedBlock>,
    max_blocks_per_item: usize,
) -> ReceiverStream<Vec<VerifiedBlock>> {
    let release_timeout = context.parameters.leader_timeout;
    let (tx, rx) = mpsc::channel(max_blocks_per_item);

    mysten_metrics::spawn_monitored_task!(async move {
        let mut state = AcceptedBlockReleaseState::new(context, release_timeout);

        loop {
            let released = tokio::select! {
                blocks = source.next() => {
                    let Some(blocks) = blocks else {
                        break;
                    };
                    state.accept_blocks(blocks)
                }
                _ = tokio::time::sleep_until(state.next_timeout()) => {
                    state.release_timed_out_rounds()
                }
            };

            if !send_released_blocks(&tx, released, max_blocks_per_item).await {
                return;
            }
        }

        let released = state.release_all();
        let _ = send_released_blocks(&tx, released, max_blocks_per_item).await;
    });

    ReceiverStream::new(rx)
}

async fn send_released_blocks(
    tx: &mpsc::Sender<Vec<VerifiedBlock>>,
    released: Vec<VerifiedBlock>,
    max_blocks_per_item: usize,
) -> bool {
    for blocks in released.chunks(max_blocks_per_item) {
        if tx.send(blocks.to_vec()).await.is_err() {
            return false;
        }
    }
    true
}

struct BufferedRound {
    first_buffered_at: Instant,
    blocks: Vec<VerifiedBlock>,
    // Aggregates the stake of block authors buffered on this stream only. Blocks dropped by
    // broadcast lag or accepted before subscription are not counted, so a round may miss
    // quorum here even when the validator has it — those rounds fall back to the release
    // timeout.
    quorum: StakeAggregator<QuorumThreshold>,
}

impl BufferedRound {
    /// Buffers the block and counts its author's stake towards the round quorum.
    /// Returns true when the quorum has been reached.
    fn add_block(&mut self, block: VerifiedBlock, committee: &Committee) -> bool {
        let reached_quorum = self.quorum.add(block.author(), committee);
        self.blocks.push(block);
        reached_quorum
    }
}

impl Default for BufferedRound {
    fn default() -> Self {
        Self {
            first_buffered_at: Instant::now(),
            blocks: Vec::new(),
            quorum: StakeAggregator::new(),
        }
    }
}

struct AcceptedBlockReleaseState {
    context: Arc<Context>,
    release_timeout: Duration,
    buffered_rounds: BTreeMap<Round, BufferedRound>,
    // Highest round released so far. Releasing a round (on quorum or timeout) also releases
    // all buffered rounds below it, so released rounds form a contiguous prefix and blocks
    // at or below this watermark pass through without buffering.
    last_released_round: Round,
}

impl AcceptedBlockReleaseState {
    fn new(context: Arc<Context>, release_timeout: Duration) -> Self {
        Self {
            context,
            release_timeout,
            buffered_rounds: BTreeMap::new(),
            last_released_round: 0,
        }
    }

    fn accept_blocks(&mut self, mut blocks: Vec<VerifiedBlock>) -> Vec<VerifiedBlock> {
        // Process blocks in ascending order, so released blocks come out in increasing round
        // order without re-sorting the (potentially much larger) output.
        sort_blocks(&mut blocks);

        let mut immediately_release = Vec::new();
        for block in blocks {
            let round = block.round();
            if round <= self.last_released_round {
                immediately_release.push(block);
                continue;
            }

            let reached_quorum = self
                .buffered_rounds
                .entry(round)
                .or_default()
                .add_block(block, &self.context.committee);

            if reached_quorum {
                immediately_release.extend(self.release_rounds_up_to(round));
            }
        }
        immediately_release
    }

    fn release_timed_out_rounds(&mut self) -> Vec<VerifiedBlock> {
        let now = Instant::now();
        let max_timed_out_round = self
            .buffered_rounds
            .iter()
            .filter_map(|(round, buffered)| {
                (now.duration_since(buffered.first_buffered_at) >= self.release_timeout)
                    .then_some(*round)
            })
            .max();

        match max_timed_out_round {
            Some(round) => self.release_rounds_up_to(round),
            None => Vec::new(),
        }
    }

    fn release_all(&mut self) -> Vec<VerifiedBlock> {
        match self.buffered_rounds.keys().next_back().copied() {
            Some(max_round) => self.release_rounds_up_to(max_round),
            None => Vec::new(),
        }
    }

    fn next_timeout(&self) -> tokio::time::Instant {
        // Effectively no timeout when nothing is buffered - the select loop re-evaluates on
        // every incoming batch anyway.
        const NO_BUFFERED_ROUNDS_TIMEOUT: Duration = Duration::from_secs(3600);

        let next_timeout = self
            .buffered_rounds
            .values()
            .map(|buffered| buffered.first_buffered_at + self.release_timeout)
            .min()
            .unwrap_or_else(|| Instant::now() + NO_BUFFERED_ROUNDS_TIMEOUT);
        tokio::time::Instant::from_std(next_timeout)
    }

    /// Releases all buffered rounds at or below `round`. A quorum (or timeout) at a round
    /// implies the rounds below it are settled, so there is no reason to keep them buffered.
    fn release_rounds_up_to(&mut self, round: Round) -> Vec<VerifiedBlock> {
        self.last_released_round = self.last_released_round.max(round);
        let remaining_rounds = self.buffered_rounds.split_off(&round.saturating_add(1));
        let released_rounds = std::mem::replace(&mut self.buffered_rounds, remaining_rounds);
        released_rounds
            .into_values()
            .flat_map(|buffered| buffered.blocks)
            .collect()
    }
}

fn sort_blocks(blocks: &mut [VerifiedBlock]) {
    blocks.sort_unstable_by_key(|block| block.reference());
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

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
        let (mut context, keys) = Context::new_for_test(4);
        context.parameters.leader_timeout = Duration::from_millis(10);
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
    async fn test_accepted_block_release_waits_for_round_quorum() {
        telemetry_subscribers::init_for_testing();
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);

        let mut state = new_release_state(context.clone());

        let block1 = VerifiedBlock::new_for_test(TestBlock::new(1, 0).build());
        let block2 = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
        let block3 = VerifiedBlock::new_for_test(TestBlock::new(1, 2).build());
        let block4 = VerifiedBlock::new_for_test(TestBlock::new(1, 3).build());

        // Adding two blocks will not form a quorum, so they are not released.
        assert!(
            state
                .accept_blocks(vec![block1.clone(), block2.clone()])
                .is_empty()
        );
        assert_eq!(state.buffered_rounds.len(), 1);
        assert_eq!(state.buffered_rounds[&1].blocks.len(), 2);

        // Adding a third block forms a quorum, so all the buffered blocks are released and
        // the round's buffer is cleaned up.
        let released = state.accept_blocks(vec![block3.clone()]);
        assert_eq!(
            released
                .iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>(),
            vec![block1.reference(), block2.reference(), block3.reference()]
        );
        assert!(state.buffered_rounds.is_empty());
        assert_eq!(state.last_released_round, 1);

        // Adding a forth block gets immediately released, as the round already reached quorum and released earlier.
        let released = state.accept_blocks(vec![block4.clone()]);
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].reference(), block4.reference());
        assert!(state.buffered_rounds.is_empty());
    }

    #[tokio::test]
    async fn test_accepted_block_release_timeout() {
        telemetry_subscribers::init_for_testing();
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);

        let mut state = AcceptedBlockReleaseState::new(context, Duration::from_millis(1));

        let block = VerifiedBlock::new_for_test(TestBlock::new(1, 0).build());
        assert!(state.accept_blocks(vec![block.clone()]).is_empty());
        assert_eq!(state.buffered_rounds.len(), 1);

        tokio::time::sleep(Duration::from_millis(2)).await;
        let released = state.release_timed_out_rounds();
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].reference(), block.reference());
        assert!(state.buffered_rounds.is_empty());
    }

    fn new_release_state(context: Arc<Context>) -> AcceptedBlockReleaseState {
        AcceptedBlockReleaseState::new(context, Duration::from_secs(60))
    }

    #[tokio::test]
    async fn test_accepted_block_release_quorum_releases_lower_rounds() {
        telemetry_subscribers::init_for_testing();
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);

        let mut state = new_release_state(context);

        // Rounds 1 and 3 stay below quorum with blocks from 2 authorities each.
        let round1_blocks = (0..2)
            .map(|author| VerifiedBlock::new_for_test(TestBlock::new(1, author).build()))
            .collect::<Vec<_>>();
        let round3_blocks = (0..2)
            .map(|author| VerifiedBlock::new_for_test(TestBlock::new(3, author).build()))
            .collect::<Vec<_>>();
        assert!(state.accept_blocks(round1_blocks.clone()).is_empty());
        assert!(state.accept_blocks(round3_blocks.clone()).is_empty());
        assert_eq!(
            state.buffered_rounds.keys().copied().collect::<Vec<_>>(),
            vec![1, 3]
        );

        // Quorum on round 2 releases rounds 1 and 2, but not round 3.
        let round2_blocks = (0..3)
            .map(|author| VerifiedBlock::new_for_test(TestBlock::new(2, author).build()))
            .collect::<Vec<_>>();
        let released = state.accept_blocks(round2_blocks.clone());
        let expected = round1_blocks
            .iter()
            .chain(round2_blocks.iter())
            .map(|block| block.reference())
            .collect::<Vec<_>>();
        assert_eq!(
            released
                .iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>(),
            expected
        );

        // The released rounds are cleaned up, only round 3 remains buffered.
        assert_eq!(
            state.buffered_rounds.keys().copied().collect::<Vec<_>>(),
            vec![3]
        );
        assert_eq!(state.last_released_round, 2);

        // A straggler below the released watermark passes through without buffering.
        let straggler = VerifiedBlock::new_for_test(TestBlock::new(1, 2).build());
        let released = state.accept_blocks(vec![straggler.clone()]);
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].reference(), straggler.reference());
        assert_eq!(
            state.buffered_rounds.keys().copied().collect::<Vec<_>>(),
            vec![3]
        );

        // Only the round 3 blocks are still buffered.
        let released = state.release_all();
        assert_eq!(
            released
                .iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>(),
            round3_blocks
                .iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>()
        );
        assert!(state.buffered_rounds.is_empty());
    }

    #[tokio::test]
    async fn test_accepted_block_release_timeout_releases_lower_rounds() {
        telemetry_subscribers::init_for_testing();
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);

        let mut state = new_release_state(context);

        let block1 = VerifiedBlock::new_for_test(TestBlock::new(1, 0).build());
        let block2 = VerifiedBlock::new_for_test(TestBlock::new(2, 0).build());
        assert!(
            state
                .accept_blocks(vec![block1.clone(), block2.clone()])
                .is_empty()
        );

        // Backdate only round 2 past the release timeout - the fresh round 1 below it is
        // released as well.
        state.buffered_rounds.get_mut(&2).unwrap().first_buffered_at =
            Instant::now() - Duration::from_secs(61);

        let released = state.release_timed_out_rounds();
        assert_eq!(
            released
                .iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>(),
            vec![block1.reference(), block2.reference()]
        );
        assert!(state.buffered_rounds.is_empty());
        assert_eq!(state.last_released_round, 2);
    }

    #[tokio::test]
    async fn test_accepted_block_release_only_timed_out_rounds() {
        telemetry_subscribers::init_for_testing();
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);

        let mut state = new_release_state(context);

        let block1 = VerifiedBlock::new_for_test(TestBlock::new(1, 0).build());
        let block2 = VerifiedBlock::new_for_test(TestBlock::new(2, 0).build());
        assert!(
            state
                .accept_blocks(vec![block1.clone(), block2.clone()])
                .is_empty()
        );

        // Backdate round 1 past the release timeout; round 2 stays fresh.
        state.buffered_rounds.get_mut(&1).unwrap().first_buffered_at =
            Instant::now() - Duration::from_secs(61);

        let released = state.release_timed_out_rounds();
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].reference(), block1.reference());
        assert_eq!(
            state.buffered_rounds.keys().copied().collect::<Vec<_>>(),
            vec![2]
        );

        // A straggler for the timed-out round passes through without buffering.
        let straggler = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
        let released = state.accept_blocks(vec![straggler.clone()]);
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].reference(), straggler.reference());
        assert_eq!(
            state.buffered_rounds.keys().copied().collect::<Vec<_>>(),
            vec![2]
        );

        // Round 2 is still buffered.
        let released = state.release_all();
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].reference(), block2.reference());
        assert!(state.buffered_rounds.is_empty());
    }

    #[tokio::test]
    async fn test_accepted_block_release_next_timeout() {
        telemetry_subscribers::init_for_testing();
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);

        let mut state = new_release_state(context);

        // With nothing buffered the next timeout is far in the future.
        assert!(
            state.next_timeout()
                >= tokio::time::Instant::from_std(Instant::now() + Duration::from_secs(1800))
        );

        // Buffering a block moves the next timeout to within the release timeout.
        let block = VerifiedBlock::new_for_test(TestBlock::new(1, 0).build());
        assert!(state.accept_blocks(vec![block]).is_empty());
        assert!(
            state.next_timeout()
                <= tokio::time::Instant::from_std(Instant::now() + Duration::from_secs(60))
        );
    }

    #[tokio::test]
    async fn test_observer_stream_releases_blocks_on_quorum() {
        telemetry_subscribers::init_for_testing();
        let (mut context, keys) = Context::new_for_test(4);
        // Make the release timeout long enough that only a round quorum can release
        // blocks within the test duration.
        context.parameters.leader_timeout = Duration::from_secs(300);
        let context = Arc::new(context);

        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let (tx_accepted_block, rx_accepted_block) = broadcast::channel::<VerifiedBlock>(100);

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

        let highest_round_per_authority = vec![0 as Round; context.committee.size()];
        let peer = keys[0].0.public().clone();
        let mut stream = observer_service
            .handle_stream_blocks(peer, highest_round_per_authority)
            .await
            .unwrap();

        // Broadcast round 1 blocks from 3 of 4 authorities to reach quorum.
        let blocks = (0..3)
            .map(|author| VerifiedBlock::new_for_test(TestBlock::new(1, author).build()))
            .collect::<Vec<_>>();
        for block in &blocks {
            tx_accepted_block.send(block.clone()).unwrap();
        }

        let mut received_blocks = Vec::new();
        while received_blocks.len() < 3 {
            let item = stream.next().await.unwrap();
            for block_bytes in item.blocks {
                let signed: SignedBlock = bcs::from_bytes(&block_bytes).unwrap();
                received_blocks.push(VerifiedBlock::new_verified(signed, block_bytes));
            }
        }
        assert_eq!(
            received_blocks
                .iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>(),
            blocks
                .iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>()
        );
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
}
