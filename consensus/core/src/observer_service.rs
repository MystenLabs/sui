// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use consensus_types::block::BlockRef;
use futures::{StreamExt as _, stream};
use parking_lot::RwLock;
use tokio::sync::broadcast;

use crate::{
    authority_service::{BroadcastStream, SubscriptionCounter},
    block::{BlockAPI as _, VerifiedBlock},
    commit::{CommitIndex, CommitRange, TrustedCommit},
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::{
        NodeId, ObserverBlockStream, ObserverBlockStreamItem, ObserverNetworkService, PeerId,
    },
};

/// Serves observer requests from observer or validator peers. It is the server-side
/// counterpart to `ObserverNetworkClient`.
pub(crate) struct ObserverService {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    rx_accepted_block_broadcast: broadcast::Receiver<(VerifiedBlock, CommitIndex)>,
    subscription_counter: Arc<SubscriptionCounter>,
}

impl ObserverService {
    pub(crate) fn new(
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        rx_accepted_block_broadcast: broadcast::Receiver<(VerifiedBlock, CommitIndex)>,
    ) -> Self {
        let subscription_counter = Arc::new(SubscriptionCounter::new(context.clone()));
        Self {
            context,
            dag_state,
            rx_accepted_block_broadcast,
            subscription_counter,
        }
    }
}

#[async_trait]
impl ObserverNetworkService for ObserverService {
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
        context::Context,
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

        let observer_service = ObserverService::new(context.clone(), dag_state, rx_accepted_block);

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

        let observer_service = ObserverService::new(context.clone(), dag_state, rx_accepted_block);

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
