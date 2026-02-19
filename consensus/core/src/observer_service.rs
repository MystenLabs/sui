// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use consensus_types::block::BlockRef;
use futures::{StreamExt as _, stream};
use parking_lot::RwLock;
use tokio::sync::broadcast;
use tracing::warn;

use crate::{
    block::{BlockAPI as _, VerifiedBlock},
    commit::{CommitIndex, CommitRange, TrustedCommit},
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::{NodeId, ObserverBlockStream, ObserverBlockStreamItem, ObserverNetworkService},
};

/// Serves observer requests from observer or validator peers. It is the server-side
/// counterpart to `ObserverNetworkClient`.
pub(crate) struct ObserverService {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    rx_accepted_block_broadcast: broadcast::Receiver<(VerifiedBlock, CommitIndex)>,
}

impl ObserverService {
    pub(crate) fn new(
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        rx_accepted_block_broadcast: broadcast::Receiver<(VerifiedBlock, CommitIndex)>,
    ) -> Self {
        Self {
            context,
            dag_state,
            rx_accepted_block_broadcast,
        }
    }
}

#[async_trait]
impl ObserverNetworkService for ObserverService {
    async fn handle_stream_blocks(
        &self,
        _peer: NodeId,
        highest_round_per_authority: Vec<u64>,
    ) -> ConsensusResult<ObserverBlockStream> {
        if highest_round_per_authority.len() != self.context.committee.size() {
            return Err(ConsensusError::InvalidSizeOfHighestAcceptedRounds(
                highest_round_per_authority.len(),
                self.context.committee.size(),
            ));
        }

        // Subscribe to the live channel BEFORE reading the DagState snapshot. This eliminates
        // a race where a block accepted between the snapshot read and resubscribe() would fall
        // in neither stream. With this ordering, such a block is guaranteed to be delivered via
        // the live channel. Blocks accepted in the narrow window between resubscribe() and the
        // snapshot read may appear in both streams; they are deduplicated below.
        let rx = self.rx_accepted_block_broadcast.resubscribe();

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

        // Subscribe to newly accepted blocks streamed in real time via the broadcast channel.
        // The commit index is bundled with each block in the broadcast payload, eliminating any
        // need to acquire the dag_state lock per block inside this hot path.
        let live_stream = stream::unfold(rx, |mut rx| async move {
            loop {
                match rx.recv().await {
                    Ok((block, commit_index)) => {
                        return Some((
                            ObserverBlockStreamItem {
                                block: block.serialized().clone(),
                                highest_commit_index: commit_index as u64,
                            },
                            rx,
                        ));
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(
                            "Observer block stream lagged by {n} messages, some blocks may have been missed"
                        );
                        // Continue to the next available message.
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
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
