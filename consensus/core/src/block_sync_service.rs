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

/// A window of consecutive certified commits served to a commit-stream consumer, with
/// the blocks referenced by each commit (in the commit's `blocks()` order) and the vote
/// blocks certifying the last commit of the window.
pub(crate) struct CommitWindow {
    pub(crate) commits: Vec<TrustedCommit>,
    pub(crate) blocks: Vec<Vec<VerifiedBlock>>,
    pub(crate) certifier_blocks: Vec<VerifiedBlock>,
}

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
                    index: block.author,
                    max: self.context.committee.size(),
                });
            }
            if block.round == GENESIS_ROUND {
                return Err(ConsensusError::UnexpectedGenesisBlockRequested);
            }
        }

        // Get the requested blocks first.
        let mut blocks = self
            .dag_state
            .read()
            .get_blocks(&block_refs)
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

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

    /// Returns the next window of certified commits starting at `start`, along with the
    /// blocks referenced by each commit and the vote blocks certifying the window's last
    /// commit. The window is capped at `commit_sync_batch_size` commits.
    ///
    /// Returns `Ok(None)` when no certified commit exists at or after `start`, i.e. the
    /// caller has reached this node's certified commit tip.
    pub async fn serve_commit_window(
        &self,
        start: CommitIndex,
    ) -> ConsensusResult<Option<CommitWindow>> {
        let (commits, certifier_blocks) = self
            .fetch_commits((start..=CommitIndex::MAX).into())
            .await?;
        if commits.is_empty() {
            return Ok(None);
        }

        // Read the blocks of all commits in the window with a single multi_get,
        // sorted by ref for better read locality, then regroup per commit in
        // each commit's `blocks()` order.
        let mut block_refs: Vec<BlockRef> = commits
            .iter()
            .flat_map(|commit| commit.blocks())
            .cloned()
            .collect();
        block_refs.sort();
        let mut fetched_blocks = BTreeMap::new();
        for (block_ref, block) in block_refs
            .iter()
            .zip_debug_eq(self.store.read_blocks(&block_refs)?)
        {
            // Committed blocks at or after `start` (which is at or above the requester's
            // local commit tip) must exist in the store, since committed blocks are not
            // pruned within an epoch.
            let Some(block) = block else {
                return Err(ConsensusError::CommittedBlockNotFound(*block_ref));
            };
            fetched_blocks.insert(*block_ref, block);
        }
        let blocks = commits
            .iter()
            .map(|commit| {
                commit
                    .blocks()
                    .iter()
                    .map(|block_ref| {
                        fetched_blocks
                            .get(block_ref)
                            .expect("Block was just fetched")
                            .clone()
                    })
                    .collect()
            })
            .collect();

        Ok(Some(CommitWindow {
            commits,
            blocks,
            certifier_blocks,
        }))
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
                    index: *authority,
                    max: self.context.committee.size(),
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
    use consensus_config::Parameters;
    use consensus_types::block::BlockDigest;

    use super::*;
    use crate::{
        block::{TestBlock, VerifiedBlock},
        commit::{CommitDigest, CommitRef},
        storage::{WriteBatch, mem_store::MemStore},
    };

    // Builds a store with commits 1..=num_commits, each referencing a single block, where
    // commits 1..=num_certified have a quorum of commit votes recorded.
    fn setup_service(num_commits: u32, num_certified: u32) -> BlockSyncService {
        let (mut context, _keys) = Context::new_for_test(4);
        context.parameters = Parameters {
            commit_sync_batch_size: 5,
            ..context.parameters
        };
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());

        let mut previous_digest = CommitDigest::MIN;
        for index in 1..=num_commits {
            let block = VerifiedBlock::new_for_test(TestBlock::new(index, 0).build());
            let commit = TrustedCommit::new_for_test(
                index,
                previous_digest,
                block.timestamp_ms(),
                block.reference(),
                vec![block.reference()],
            );
            previous_digest = commit.digest();

            // Blocks written after the commit carry votes on it, certifying it.
            let mut vote_blocks = vec![];
            if index <= num_certified {
                for author in 0..3 {
                    let vote_block = VerifiedBlock::new_for_test(
                        TestBlock::new(index + 1, author)
                            .set_commit_votes(vec![CommitRef::new(index, commit.digest())])
                            .build(),
                    );
                    vote_blocks.push(vote_block);
                }
            }

            let mut blocks = vec![block];
            blocks.extend(vote_blocks);
            store
                .write(WriteBatch::default().blocks(blocks).commits(vec![commit]))
                .unwrap();
        }

        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        BlockSyncService::new(context, dag_state, store)
    }

    #[tokio::test]
    async fn test_serve_commit_window_basic() {
        let service = setup_service(8, 6);

        // First window is capped at the batch size (5), with one block per commit and
        // certifier blocks for the last commit of the window.
        let window = service.serve_commit_window(1).await.unwrap().unwrap();
        assert_eq!(
            window.commits.iter().map(|c| c.index()).collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5]
        );
        assert_eq!(window.blocks.len(), 5);
        for (commit, blocks) in window.commits.iter().zip_debug_eq(&window.blocks) {
            assert_eq!(
                blocks.iter().map(|b| b.reference()).collect::<Vec<_>>(),
                commit.blocks().to_vec()
            );
        }
        assert_eq!(window.certifier_blocks.len(), 3);

        // The next window excludes the uncertified tail (commits 7 and 8).
        let window = service.serve_commit_window(6).await.unwrap().unwrap();
        assert_eq!(
            window.commits.iter().map(|c| c.index()).collect::<Vec<_>>(),
            vec![6]
        );
        assert_eq!(window.certifier_blocks.len(), 3);
    }

    #[tokio::test]
    async fn test_serve_commit_window_at_tip() {
        let service = setup_service(8, 6);

        // Only uncertified commits at/after the start index.
        assert!(service.serve_commit_window(7).await.unwrap().is_none());
        // Past the last commit entirely.
        assert!(service.serve_commit_window(9).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_serve_commit_window_missing_block() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        // Construct DagState before writing the incomplete commit, since DagState recovery
        // loads committed subdags from the store and expects their blocks to exist.
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        // A certified commit referencing a block that is not in the store.
        let missing_ref = BlockRef::new(1, AuthorityIndex::new_for_test(0), BlockDigest::MIN);
        let commit =
            TrustedCommit::new_for_test(1, CommitDigest::MIN, 0, missing_ref, vec![missing_ref]);
        let vote_blocks = (0..3)
            .map(|author| {
                VerifiedBlock::new_for_test(
                    TestBlock::new(2, author)
                        .set_commit_votes(vec![CommitRef::new(1, commit.digest())])
                        .build(),
                )
            })
            .collect::<Vec<_>>();
        store
            .write(
                WriteBatch::default()
                    .blocks(vote_blocks)
                    .commits(vec![commit]),
            )
            .unwrap();

        let service = BlockSyncService::new(context, dag_state, store);

        let result = service.serve_commit_window(1).await;
        assert!(matches!(
            result,
            Err(ConsensusError::CommittedBlockNotFound(_))
        ));
    }
}
