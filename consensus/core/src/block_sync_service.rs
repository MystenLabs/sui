// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round};
use parking_lot::RwLock;
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

    /// Fetches blocks from the DAG state based on the provided block references.
    ///
    /// This method handles two types of requests:
    /// 1. Missing block for block sync (when highest_accepted_rounds is provided):
    ///    - Returns up to max_blocks_per_sync blocks
    ///    - Can fetch ancestors if breadth_first is true
    ///    - Can fetch additional blocks from authorities with missing blocks
    /// 2. Committed block for commit sync (when highest_accepted_rounds is empty):
    ///    - Returns up to max_blocks_per_fetch blocks
    ///    - Simple fetch of requested blocks only
    pub async fn fetch_blocks(
        &self,
        mut block_refs: Vec<BlockRef>,
        highest_accepted_rounds: Vec<Round>,
        breadth_first: bool,
    ) -> ConsensusResult<Vec<Bytes>> {
        // Validate highest_accepted_rounds if provided
        if !highest_accepted_rounds.is_empty()
            && highest_accepted_rounds.len() != self.context.committee.size()
        {
            return Err(ConsensusError::InvalidSizeOfHighestAcceptedRounds(
                highest_accepted_rounds.len(),
                self.context.committee.size(),
            ));
        }

        // Determine max response size based on request type
        let max_response_num_blocks = if !highest_accepted_rounds.is_empty() {
            self.context.parameters.max_blocks_per_sync
        } else {
            self.context.parameters.max_blocks_per_fetch
        };

        if block_refs.len() > max_response_num_blocks {
            block_refs.truncate(max_response_num_blocks);
        }

        // Validate the requested block refs
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

        // Get requested blocks from store
        let blocks = if !highest_accepted_rounds.is_empty() {
            block_refs.sort();
            block_refs.dedup();
            let mut blocks = self
                .dag_state
                .read()
                .get_blocks(&block_refs)
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();

            if breadth_first {
                // Get unique missing ancestor blocks of the requested blocks
                let mut missing_ancestors = blocks
                    .iter()
                    .flat_map(|block| block.ancestors().to_vec())
                    .filter(|block_ref| highest_accepted_rounds[block_ref.author] < block_ref.round)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();

                // If there are too many missing ancestors, randomly select a subset
                let selected_num_blocks = max_response_num_blocks.saturating_sub(blocks.len());
                if selected_num_blocks < missing_ancestors.len() {
                    use rand::seq::SliceRandom;
                    missing_ancestors = missing_ancestors
                        .choose_multiple(&mut mysten_common::random::get_rng(), selected_num_blocks)
                        .copied()
                        .collect::<Vec<_>>();
                }
                let ancestor_blocks = self.dag_state.read().get_blocks(&missing_ancestors);
                blocks.extend(ancestor_blocks.into_iter().flatten());
            } else {
                // Get additional blocks from authorities with missing blocks (depth-first approach)
                use std::collections::BTreeMap;
                let mut lowest_missing_rounds = BTreeMap::<AuthorityIndex, Round>::new();
                for block_ref in blocks.iter().map(|b| b.reference()) {
                    let entry = lowest_missing_rounds
                        .entry(block_ref.author)
                        .or_insert(block_ref.round);
                    *entry = (*entry).min(block_ref.round);
                }

                // Retrieve additional blocks per authority
                let dag_state = self.dag_state.read();
                for (authority, lowest_missing_round) in lowest_missing_rounds {
                    let highest_accepted_round = highest_accepted_rounds[authority];
                    if highest_accepted_round >= lowest_missing_round {
                        continue;
                    }
                    let missing_blocks = dag_state.get_cached_blocks_in_range(
                        authority,
                        highest_accepted_round + 1,
                        lowest_missing_round,
                        self.context
                            .parameters
                            .max_blocks_per_sync
                            .saturating_sub(blocks.len()),
                    );
                    blocks.extend(missing_blocks);
                    if blocks.len() >= self.context.parameters.max_blocks_per_sync {
                        blocks.truncate(self.context.parameters.max_blocks_per_sync);
                        break;
                    }
                }
            }

            blocks
        } else {
            // Simple fetch for commit sync
            self.dag_state
                .read()
                .get_blocks(&block_refs)
                .into_iter()
                .flatten()
                .collect()
        };

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
        let mut blocks = vec![];
        let dag_state = self.dag_state.read();
        for authority in authorities {
            let block = dag_state.get_last_block_for_authority(authority);

            debug!("Latest block for {authority}: {block:?}");

            // No reason to serve back the genesis block
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
