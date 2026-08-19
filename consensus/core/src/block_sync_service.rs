// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    sync::Arc,
};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round};
use mysten_common::ZipDebugEqIteratorExt;
use parking_lot::RwLock;
use rand::seq::SliceRandom;
use tracing::debug;

use crate::{
    CommitIndex, VerifiedBlock,
    block::{BlockAPI, GENESIS_ROUND},
    commit::{CommitAPI, CommitRange, TrustedCommit},
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    storage::Store,
};

/// Provides shared block synchronization functionality for both AuthorityService and ObserverService.
/// This service handles fetch requests from synchronizer and commit_syncer components,
/// eliminating code duplication between the two network services.
pub(crate) struct BlockSyncService {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    store: Arc<dyn Store>,
}

impl BlockSyncService {
    pub fn new(
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        store: Arc<dyn Store>,
    ) -> Self {
        Self {
            context,
            dag_state,
            store,
        }
    }

    // Handles 3 types of requests:
    // 1. Live sync:
    //    - Both missing block refs and highest accepted rounds are specified.
    //    - fetch_missing_ancestors is true.
    //    - response returns max_blocks_per_sync blocks.
    // 2. Periodic sync:
    //    - Highest accepted rounds must be specified.
    //    - Missing block refs are optional.
    //    - fetch_missing_ancestors is false (default).
    //    - response returns max_blocks_per_fetch blocks.
    // 3. Commit sync:
    //    - Missing block refs are specified.
    //    - Highest accepted rounds are empty.
    //    - fetch_missing_ancestors is false (default).
    //    - response returns max_blocks_per_fetch blocks.
    pub async fn fetch_blocks(
        &self,
        mut block_refs: Vec<BlockRef>,
        fetch_after_rounds: Vec<Round>,
        fetch_missing_ancestors: bool,
    ) -> ConsensusResult<Vec<Bytes>> {
        if block_refs.is_empty() && (fetch_missing_ancestors || fetch_after_rounds.is_empty()) {
            return Err(ConsensusError::InvalidFetchBlocksRequest("When no block refs are provided, fetch_after_rounds must be provided and fetch_missing_ancestors must be false".to_string()));
        }
        if fetch_missing_ancestors && fetch_after_rounds.is_empty() {
            return Err(ConsensusError::InvalidFetchBlocksRequest(
                "When fetch_missing_ancestors is true, fetch_after_rounds must be provided"
                    .to_string(),
            ));
        }
        if !fetch_after_rounds.is_empty()
            && fetch_after_rounds.len() != self.context.committee.size()
        {
            return Err(ConsensusError::InvalidSizeOfHighestAcceptedRounds(
                fetch_after_rounds.len(),
                self.context.committee.size(),
            ));
        }

        // Finds the suitable limit of # of blocks to return.
        let max_response_num_blocks = if !fetch_after_rounds.is_empty() && !block_refs.is_empty() {
            self.context.parameters.max_blocks_per_sync
        } else {
            self.context.parameters.max_blocks_per_fetch
        };
        if block_refs.len() > max_response_num_blocks {
            block_refs.truncate(max_response_num_blocks);
        }

        // Validate the requested block refs.
        for block in &block_refs {
            if !self.context.committee.is_valid_index(block.author) {
                return Err(ConsensusError::InvalidAuthorityIndex {
                    loc: format!("requested block {}", block),
                    index: block.author,
                    max: self.context.committee.size() - 1,
                });
            }
            if block.round == GENESIS_ROUND {
                return Err(ConsensusError::UnexpectedGenesisBlockRequested);
            }
        }

        // Get the requested blocks first. Commit sync requests (block refs only, no
        // fetch_after_rounds, no fetch_missing_ancestors) read old committed blocks, up
        // to max_blocks_per_fetch of them, so they read the store first to avoid holding
        // the DagState lock across large storage reads. Every other request shape
        // targets recently accepted blocks, capped at max_blocks_per_sync, which are
        // almost always in the DagState cache; reading the store first would only add a
        // wasted store read per request.
        let mut blocks = if fetch_after_rounds.is_empty() {
            self.read_blocks(&block_refs)?
        } else {
            self.dag_state
                .read()
                .get_blocks(&block_refs)
                .into_iter()
                .flatten()
                .collect()
        };

        // When fetch_missing_ancestors is true, fetch missing ancestors of the requested blocks.
        // Otherwise, fetch additional blocks depth-first from the requested block authorities.
        if blocks.len() < max_response_num_blocks && !fetch_after_rounds.is_empty() {
            if fetch_missing_ancestors {
                // Get unique missing ancestor blocks of the requested blocks (validated to be non-empty).
                // fetch_after_rounds will only be used to filter out already accepted blocks.
                let missing_ancestors = blocks
                    .iter()
                    .flat_map(|block| block.ancestors().to_vec())
                    .filter(|block_ref| fetch_after_rounds[block_ref.author] < block_ref.round)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();

                // If there are too many missing ancestors, randomly select a subset to avoid
                // fetching duplicated blocks across peers.
                let selected_num_blocks = max_response_num_blocks
                    .saturating_sub(blocks.len())
                    .min(missing_ancestors.len());
                if selected_num_blocks > 0 {
                    let selected_ancestor_refs = missing_ancestors
                        .choose_multiple(&mut mysten_common::random::get_rng(), selected_num_blocks)
                        .copied()
                        .collect::<Vec<_>>();
                    // Ancestors of recently accepted blocks are in the cache, like the
                    // requested blocks above.
                    let ancestor_blocks = self
                        .dag_state
                        .read()
                        .get_blocks(&selected_ancestor_refs)
                        .into_iter()
                        .flatten();
                    blocks.extend(ancestor_blocks);
                }
            } else {
                // Get additional blocks from authorities with missing block.
                // Compute the fetch round per requested authority, or all authorities.
                let mut limit_rounds = BTreeMap::<AuthorityIndex, Round>::new();
                if block_refs.is_empty() {
                    let dag_state = self.dag_state.read();
                    for (index, _authority) in self.context.committee.authorities() {
                        let last_block = dag_state.get_last_block_for_authority(index);
                        limit_rounds.insert(index, last_block.round());
                    }
                } else {
                    for block_ref in &block_refs {
                        let entry = limit_rounds
                            .entry(block_ref.author)
                            .or_insert(block_ref.round);
                        *entry = (*entry).min(block_ref.round);
                    }
                }

                // Use a min-heap to fetch blocks across authorities in ascending round order.
                // Each entry is (fetch_start_round, authority, limit_round).
                let mut heap = BinaryHeap::new();
                for (authority, limit_round) in &limit_rounds {
                    let fetch_start = fetch_after_rounds[*authority] + 1;
                    if fetch_start < *limit_round {
                        heap.push(Reverse((fetch_start, *authority, *limit_round)));
                    }
                }

                while let Some(Reverse((fetch_start, authority, limit_round))) = heap.pop() {
                    let fetched = self.store.scan_blocks_by_author_in_range(
                        authority,
                        fetch_start,
                        limit_round,
                        1,
                    )?;
                    if let Some(block) = fetched.into_iter().next() {
                        let next_start = block.round() + 1;
                        blocks.push(block);
                        if blocks.len() >= max_response_num_blocks {
                            blocks.truncate(max_response_num_blocks);
                            break;
                        }
                        if next_start < limit_round {
                            heap.push(Reverse((next_start, authority, limit_round)));
                        }
                    }
                }
            }
        }

        // Return the serialized blocks
        let bytes = blocks
            .into_iter()
            .map(|block| block.serialized().clone())
            .collect::<Vec<_>>();
        Ok(bytes)
    }

    /// Reads blocks by refs like `DagState::get_blocks`, but reads the store first so no
    /// storage I/O happens while holding the DagState lock: commit sync requests read up
    /// to `max_blocks_per_fetch` blocks, and a read lock held across large store reads
    /// delays Core's writes on the consensus hot path. Blocks missing from the store are
    /// the most recently accepted ones that are not yet flushed, which are found in
    /// DagState's in-memory cache; only blocks that do not exist at all fall through to
    /// `get_blocks`' internal store read, and honest requesters never ask for those.
    /// Blocks found nowhere are dropped from the result.
    fn read_blocks(&self, block_refs: &[BlockRef]) -> ConsensusResult<Vec<VerifiedBlock>> {
        self.context
            .metrics
            .node_metrics
            .block_sync_service_read_blocks_count
            .inc();
        let mut blocks = self.store.read_blocks(block_refs)?;
        let missing: Vec<(usize, BlockRef)> = blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.is_none())
            .map(|(index, _)| (index, block_refs[index]))
            .collect();
        if !missing.is_empty() {
            let missing_refs: Vec<BlockRef> =
                missing.iter().map(|(_, block_ref)| *block_ref).collect();
            let cached_blocks = self.dag_state.read().get_blocks(&missing_refs);
            for ((index, _), block) in missing.into_iter().zip_debug_eq(cached_blocks) {
                blocks[index] = block;
            }
        }
        Ok(blocks.into_iter().flatten().collect())
    }

    /// Fetches commits and their certifying blocks from the store.
    ///
    /// Returns commits within the specified range along with the blocks that certify
    /// the last commit (if it has reached quorum).
    pub async fn fetch_commits(
        &self,
        commit_range: CommitRange,
    ) -> ConsensusResult<(Vec<TrustedCommit>, Vec<VerifiedBlock>)> {
        // Compute an inclusive end index and bound the maximum number of commits scanned
        let inclusive_end = commit_range.end().min(
            commit_range.start() + self.context.parameters.commit_sync_batch_size as CommitIndex
                - 1,
        );
        let mut commits = self
            .store
            .scan_commits((commit_range.start()..=inclusive_end).into())?;

        let mut certifier_block_refs = vec![];

        // Find the last commit that has reached quorum
        'commit: while let Some(c) = commits.last() {
            let index = c.index();
            let votes = self.store.read_commit_votes(index)?;
            let mut stake_aggregator = StakeAggregator::<QuorumThreshold>::new();
            for v in &votes {
                stake_aggregator.add(v.author, &self.context.committee);
            }
            if stake_aggregator.reached_threshold(&self.context.committee) {
                certifier_block_refs = votes;
                break 'commit;
            } else {
                debug!(
                    "Commit {} votes did not reach quorum to certify, {} < {}, skipping",
                    index,
                    stake_aggregator.stake(),
                    stake_aggregator.threshold(&self.context.committee)
                );
                self.context
                    .metrics
                    .node_metrics
                    .commit_sync_fetch_commits_handler_uncertified_skipped
                    .inc();
                commits.pop();
            }
        }

        // Fetch the certifier blocks
        let certifier_blocks = self
            .store
            .read_blocks(&certifier_block_refs)?
            .into_iter()
            .flatten()
            .collect();

        Ok((commits, certifier_blocks))
    }

    /// Fetches the latest blocks for the specified authorities.
    pub async fn fetch_latest_blocks(
        &self,
        peer: AuthorityIndex,
        authorities: Vec<AuthorityIndex>,
    ) -> ConsensusResult<Vec<Bytes>> {
        if authorities.len() > self.context.committee.size() {
            return Err(ConsensusError::TooManyAuthoritiesProvided(peer));
        }

        // Ensure that those are valid authorities
        for authority in &authorities {
            if !self.context.committee.is_valid_index(*authority) {
                return Err(ConsensusError::InvalidAuthorityIndex {
                    loc: format!("latest block request from peer {}", peer),
                    index: *authority,
                    max: self.context.committee.size() - 1,
                });
            }
        }

        // Read from the dag state to find the latest blocks
        // TODO: at the moment we don't look into the block manager for suspended blocks. Ideally we
        // want in the future if we think we would like to tackle the majority of cases.
        let mut blocks = vec![];
        let dag_state = self.dag_state.read();
        for authority in authorities {
            let block = dag_state.get_last_block_for_authority(authority);

            debug!("Latest block for {authority}: {block:?}");

            // no reason to serve back the genesis block - it's equal as if it has not received any block
            if block.round() != GENESIS_ROUND {
                blocks.push(block);
            }
        }

        // Return the serialised blocks
        let result = blocks
            .into_iter()
            .map(|block| block.serialized().clone())
            .collect::<Vec<_>>();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block::{TestBlock, VerifiedBlock},
        storage::{WriteBatch, mem_store::MemStore},
    };

    fn setup_service() -> (BlockSyncService, Arc<RwLock<DagState>>, Arc<MemStore>) {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let service = BlockSyncService::new(context, dag_state.clone(), store.clone());
        (service, dag_state, store)
    }

    fn test_blocks(round: Round) -> Vec<VerifiedBlock> {
        (0..4)
            .map(|author| VerifiedBlock::new_for_test(TestBlock::new(round, author).build()))
            .collect()
    }

    #[tokio::test]
    async fn test_fetch_blocks_from_store() {
        let (service, _dag_state, store) = setup_service();

        // Blocks in the store but not in the DagState cache, as after cache eviction.
        let blocks = test_blocks(1);
        store
            .write(WriteBatch::default().blocks(blocks.clone()))
            .unwrap();

        let block_refs: Vec<_> = blocks.iter().map(|b| b.reference()).collect();
        let fetched = service
            .fetch_blocks(block_refs, vec![], false)
            .await
            .unwrap();
        assert_eq!(
            fetched,
            blocks
                .iter()
                .map(|b| b.serialized().clone())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_fetch_blocks_unflushed_from_cache() {
        let (service, dag_state, _store) = setup_service();

        // Blocks accepted into the DagState but not yet flushed to the store.
        let blocks = test_blocks(1);
        dag_state.write().accept_blocks(blocks.clone());

        let block_refs: Vec<_> = blocks.iter().map(|b| b.reference()).collect();
        let fetched = service
            .fetch_blocks(block_refs, vec![], false)
            .await
            .unwrap();
        assert_eq!(
            fetched,
            blocks
                .iter()
                .map(|b| b.serialized().clone())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_fetch_blocks_mixed_store_and_cache() {
        let (service, dag_state, store) = setup_service();

        // Round 1 blocks are only in the store, round 2 blocks only in the cache,
        // as when older blocks have been flushed and evicted while recent ones are
        // not yet flushed.
        let flushed_blocks = test_blocks(1);
        store
            .write(WriteBatch::default().blocks(flushed_blocks.clone()))
            .unwrap();
        let unflushed_blocks = test_blocks(2);
        dag_state.write().accept_blocks(unflushed_blocks.clone());

        // Interleave the refs to check the response preserves request order.
        let all_blocks: Vec<_> = flushed_blocks
            .into_iter()
            .zip_debug_eq(unflushed_blocks)
            .flat_map(|(flushed, unflushed)| [flushed, unflushed])
            .collect();
        let block_refs: Vec<_> = all_blocks.iter().map(|b| b.reference()).collect();
        let fetched = service
            .fetch_blocks(block_refs, vec![], false)
            .await
            .unwrap();
        assert_eq!(
            fetched,
            all_blocks
                .iter()
                .map(|b| b.serialized().clone())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_fetch_blocks_live_sync_from_cache() {
        let (service, dag_state, _store) = setup_service();

        // Live sync shape: refs + fetch_after_rounds, served from the DagState cache
        // even when nothing has been flushed to the store.
        let blocks = test_blocks(1);
        dag_state.write().accept_blocks(blocks.clone());

        let block_refs: Vec<_> = blocks.iter().map(|b| b.reference()).collect();
        let fetched = service
            .fetch_blocks(block_refs, vec![0; 4], true)
            .await
            .unwrap();
        assert_eq!(
            fetched,
            blocks
                .iter()
                .map(|b| b.serialized().clone())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_fetch_blocks_missing_blocks_dropped() {
        let (service, _dag_state, store) = setup_service();

        let blocks = test_blocks(1);
        store
            .write(WriteBatch::default().blocks(blocks.clone()))
            .unwrap();

        // Request one existing block and one unknown block.
        let unknown_ref = VerifiedBlock::new_for_test(TestBlock::new(5, 0).build()).reference();
        let fetched = service
            .fetch_blocks(vec![blocks[0].reference(), unknown_ref], vec![], false)
            .await
            .unwrap();
        assert_eq!(fetched, vec![blocks[0].serialized().clone()]);
    }
}
