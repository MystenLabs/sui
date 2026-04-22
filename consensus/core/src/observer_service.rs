// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_types::block::BlockRef;
use futures::{StreamExt as _, stream};
use parking_lot::RwLock;
use sui_macros::fail_point_async;
use tap::TapFallible;
use tokio::sync::broadcast;

use crate::{
    BlockVerifier, TransactionVoteTracker,
    authority_service::{BroadcastStream, SubscriptionCounter},
    block::{BlockAPI as _, SignedBlock, VerifiedBlock},
    commit::{CommitIndex, CommitRange, TrustedCommit},
    commit_vote_monitor::CommitVoteMonitor,
    context::Context,
    core_thread::CoreThreadDispatcher,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::{
        NodeId, ObserverBlockStream, ObserverBlockStreamItem, ObserverNetworkService, PeerId,
    },
};

// Is used to calculate the threshold for blocking blocks when the commit index is lagging too far from the quorum commit index.
// This is a multiplier of the commit_sync_batch_size.
pub(crate) const COMMIT_LAG_MULTIPLIER: u32 = 5;

/// Serves observer requests from observer or validator peers. It is the server-side
/// counterpart to `ObserverNetworkClient`.
pub(crate) struct ObserverService {
    context: Arc<Context>,
    core_dispatcher: Arc<dyn CoreThreadDispatcher>,
    dag_state: Arc<RwLock<DagState>>,
    rx_accepted_block_broadcast: broadcast::Receiver<(VerifiedBlock, CommitIndex)>,
    subscription_counter: Arc<SubscriptionCounter>,
    block_verifier: Arc<dyn BlockVerifier>,
    commit_vote_monitor: Arc<CommitVoteMonitor>,
    transaction_vote_tracker: TransactionVoteTracker,
}

impl ObserverService {
    pub(crate) fn new(
        context: Arc<Context>,
        core_dispatcher: Arc<dyn CoreThreadDispatcher>,
        dag_state: Arc<RwLock<DagState>>,
        rx_accepted_block_broadcast: broadcast::Receiver<(VerifiedBlock, CommitIndex)>,
        block_verifier: Arc<dyn BlockVerifier>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        transaction_vote_tracker: TransactionVoteTracker,
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
        }
    }
}

#[async_trait]
impl ObserverNetworkService for ObserverService {
    async fn handle_block(
        &self,
        peer: PeerId,
        item: ObserverBlockStreamItem,
    ) -> ConsensusResult<()> {
        fail_point_async!("consensus-rpc-response");

        // TODO: dedup block verifications, here and with fetched blocks.
        let signed_block: SignedBlock =
            bcs::from_bytes(&item.block).map_err(ConsensusError::MalformedBlock)?;

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
            .verify_and_vote(signed_block, item.block)
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
        // The threshold to ignore block should be larger than commit_sync_batch_size,
        // to avoid excessive block rejections and synchronizations.
        if last_commit_index
            + self.context.parameters.commit_sync_batch_size * COMMIT_LAG_MULTIPLIER
            < quorum_commit_index
        {
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

        // TODO: Schedule fetching missing ancestors from this peer in the background.
        // This requires the refactored synchronizer that supports PeerId (from the
        // consensus-synchronizer-peers-pool branch). For now, just record metrics.
        if !missing_ancestors.is_empty() {
            self.context
                .metrics
                .node_metrics
                .handler_received_block_missing_ancestors
                .with_label_values(&[block_author_hostname])
                .inc_by(missing_ancestors.len() as u64);

            tracing::debug!(
                "Block has {} missing ancestors that need to be fetched",
                missing_ancestors.len()
            );
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
        let (past_blocks, current_commit_index) = {
            let dag_state = self.dag_state.read();
            let current_commit_index = dag_state.last_commit_index();
            let mut past_blocks = Vec::new();

            for (authority, _) in self.context.committee.authorities() {
                let from_round = highest_round_per_authority[authority.value()] as u32 + 1;
                past_blocks.extend(dag_state.get_cached_blocks(authority, from_round));
            }

            past_blocks.sort_unstable_by_key(|b| b.round());
            (past_blocks, current_commit_index)
        };

        let past_stream =
            stream::iter(
                past_blocks
                    .into_iter()
                    .map(move |block| ObserverBlockStreamItem {
                        block: block.serialized().clone(),
                        highest_commit_index: current_commit_index as u64,
                    }),
            );

        let live_stream = BroadcastStream::<(VerifiedBlock, CommitIndex)>::new(
            PeerId::Observer(peer),
            self.rx_accepted_block_broadcast.resubscribe(),
            self.subscription_counter.clone(),
        )
        .map(|(block, commit_index)| ObserverBlockStreamItem {
            block: block.serialized().clone(),
            highest_commit_index: commit_index as u64,
        });

        Ok(Box::pin(past_stream.chain(live_stream)))
    }

    async fn handle_fetch_blocks(
        &self,
        _peer: NodeId,
        _block_refs: Vec<BlockRef>,
    ) -> ConsensusResult<Vec<Bytes>> {
        // TODO: implement observer fetch blocks, similar to validator fetch_blocks but
        // without highest_accepted_rounds.
        Err(ConsensusError::NetworkRequest(
            "Observer fetch blocks not yet implemented".to_string(),
        ))
    }

    async fn handle_fetch_commits(
        &self,
        _peer: NodeId,
        _commit_range: CommitRange,
    ) -> ConsensusResult<(Vec<TrustedCommit>, Vec<VerifiedBlock>)> {
        // TODO: implement observer fetch commits, similar to validator fetch_commits.
        Err(ConsensusError::NetworkRequest(
            "Observer fetch commits not yet implemented".to_string(),
        ))
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

    #[tokio::test]
    async fn test_observer_stream_receives_broadcast_blocks() {
        telemetry_subscribers::init_for_testing();
        let (context, keys) = Context::new_for_test(4);
        let context = Arc::new(context);

        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        let (tx_accepted_block, rx_accepted_block) =
            broadcast::channel::<(VerifiedBlock, CommitIndex)>(100);

        // Create mock dependencies
        let core_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let block_verifier = Arc::new(NoopBlockVerifier);
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());

        let observer_service = ObserverService::new(
            context.clone(),
            core_dispatcher,
            dag_state,
            rx_accepted_block,
            block_verifier,
            commit_vote_monitor,
            transaction_vote_tracker,
        );

        // Observer starts with no blocks seen
        let highest_round_per_authority = vec![0u64; context.committee.size()];
        let peer = keys[0].0.public().clone();

        let mut stream = observer_service
            .handle_stream_blocks(peer, highest_round_per_authority)
            .await
            .unwrap();

        // Broadcast three blocks
        let block1 = VerifiedBlock::new_for_test(TestBlock::new(5, 0).build());
        let block2 = VerifiedBlock::new_for_test(TestBlock::new(10, 1).build());
        let block3 = VerifiedBlock::new_for_test(TestBlock::new(15, 2).build());

        tx_accepted_block.send((block1.clone(), 1)).unwrap();
        tx_accepted_block.send((block2.clone(), 2)).unwrap();
        tx_accepted_block.send((block3.clone(), 3)).unwrap();

        // Verify observer receives all three blocks in order
        let item1 = stream.next().await.unwrap();
        let signed1 = bcs::from_bytes(&item1.block).unwrap();
        let received1 = VerifiedBlock::new_verified(signed1, item1.block.clone());
        assert_eq!(received1.round(), 5);
        assert_eq!(received1.author().value(), 0);
        assert_eq!(item1.highest_commit_index, 1);

        let item2 = stream.next().await.unwrap();
        let signed2 = bcs::from_bytes(&item2.block).unwrap();
        let received2 = VerifiedBlock::new_verified(signed2, item2.block.clone());
        assert_eq!(received2.round(), 10);
        assert_eq!(received2.author().value(), 1);
        assert_eq!(item2.highest_commit_index, 2);

        let item3 = stream.next().await.unwrap();
        let signed3 = bcs::from_bytes(&item3.block).unwrap();
        let received3 = VerifiedBlock::new_verified(signed3, item3.block.clone());
        assert_eq!(received3.round(), 15);
        assert_eq!(received3.author().value(), 2);
        assert_eq!(item3.highest_commit_index, 3);
    }

    #[tokio::test]
    async fn test_observer_stream_invalid_input() {
        telemetry_subscribers::init_for_testing();
        let (context, keys) = Context::new_for_test(4);
        let context = Arc::new(context);

        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        let (_tx_accepted_block, rx_accepted_block) =
            broadcast::channel::<(VerifiedBlock, CommitIndex)>(100);

        // Create mock dependencies
        let core_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let block_verifier = Arc::new(NoopBlockVerifier);
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());

        let observer_service = ObserverService::new(
            context.clone(),
            core_dispatcher,
            dag_state,
            rx_accepted_block,
            block_verifier,
            commit_vote_monitor,
            transaction_vote_tracker,
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
