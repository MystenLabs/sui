// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, VecDeque},
    ops::Bound::Included,
    time::Duration,
};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use sui_macros::fail_point;
use typed_store::{
    metrics::SamplingInterval,
    rocks::{default_db_options, DBMap, DBMapTableConfigMap, MetricConf},
    DBMapUtils, Map as _,
};

use super::{CommitInfo, Store, WriteBatch};
use crate::{
    block::{BlockAPI as _, BlockDigest, BlockRef, Round, SignedBlock, VerifiedBlock},
    commit::{CommitAPI as _, CommitDigest, CommitIndex, CommitRange, CommitRef, TrustedCommit},
    error::{ConsensusError, ConsensusResult},
};

/// Persistent storage with RocksDB.
#[derive(DBMapUtils)]
#[cfg_attr(tidehunter, tidehunter)]
pub(crate) struct RocksDBStore {
    /// Stores SignedBlock by refs.
    blocks: DBMap<(Round, AuthorityIndex, BlockDigest), Bytes>,
    /// A secondary index that orders refs first by authors.
    #[rename = "digests"]
    digests_by_authorities: DBMap<(AuthorityIndex, Round, BlockDigest), ()>,
    /// Maps commit index to Commit.
    commits: DBMap<(CommitIndex, CommitDigest), Bytes>,
    /// Collects votes on commits.
    /// TODO: batch multiple votes into a single row.
    commit_votes: DBMap<(CommitIndex, CommitDigest, BlockRef), ()>,
    /// Stores info related to Commit that helps recovery.
    commit_info: DBMap<(CommitIndex, CommitDigest), CommitInfo>,
}

impl RocksDBStore {
    const BLOCKS_CF: &'static str = "blocks";
    const DIGESTS_BY_AUTHORITIES_CF: &'static str = "digests";
    const COMMITS_CF: &'static str = "commits";
    const COMMIT_VOTES_CF: &'static str = "commit_votes";
    const COMMIT_INFO_CF: &'static str = "commit_info";

    /// Creates a new instance of RocksDB storage.
    #[cfg(not(tidehunter))]
    pub(crate) fn new(path: &str) -> Self {
        // Consensus data has high write throughput (all transactions) and is rarely read
        // (only during recovery and when helping peers catch up).
        let db_options = default_db_options().optimize_db_for_write_throughput(2);
        let mut metrics_conf = MetricConf::new("consensus");
        metrics_conf.read_sample_interval = SamplingInterval::new(Duration::from_secs(60), 0);
        let cf_options = default_db_options().optimize_for_write_throughput();
        let column_family_options = DBMapTableConfigMap::new(BTreeMap::from([
            (
                Self::BLOCKS_CF.to_string(),
                default_db_options()
                    .optimize_for_write_throughput_no_deletion()
                    // Using larger block is ok since there is not much point reads on the cf.
                    .set_block_options(512, 128 << 10),
            ),
            (
                Self::DIGESTS_BY_AUTHORITIES_CF.to_string(),
                cf_options.clone(),
            ),
            (Self::COMMITS_CF.to_string(), cf_options.clone()),
            (Self::COMMIT_VOTES_CF.to_string(), cf_options.clone()),
            (Self::COMMIT_INFO_CF.to_string(), cf_options.clone()),
        ]));
        Self::open_tables_read_write(
            path.into(),
            metrics_conf,
            Some(db_options.options),
            Some(column_family_options),
        )
    }

    #[cfg(tidehunter)]
    pub(crate) fn new(path: &str) -> Self {
        tracing::warn!("Consensus store using tidehunter");
        use typed_store::tidehunter_util::{KeyIndexing, KeyType, ThConfig};
        const MUTEXES: usize = 1024;
        let index_digest_key = KeyIndexing::key_reduction(36, 0..12);
        let index_index_digest_key = KeyIndexing::key_reduction(40, 0..24);
        let commit_vote_key = KeyIndexing::key_reduction(76, 0..60);
        let u32_prefix = KeyType::prefix_uniform(2, 4);
        let u64_prefix = KeyType::prefix_uniform(6, 4);
        let configs = vec![
            (
                Self::BLOCKS_CF.to_string(),
                ThConfig::new_with_indexing(
                    index_index_digest_key.clone(),
                    MUTEXES,
                    u32_prefix.clone(),
                ),
            ),
            (
                Self::DIGESTS_BY_AUTHORITIES_CF.to_string(),
                ThConfig::new_with_indexing(
                    index_index_digest_key.clone(),
                    MUTEXES,
                    u64_prefix.clone(),
                ),
            ),
            (
                Self::COMMITS_CF.to_string(),
                ThConfig::new_with_indexing(index_digest_key.clone(), MUTEXES, u32_prefix.clone()),
            ),
            (
                Self::COMMIT_VOTES_CF.to_string(),
                ThConfig::new_with_indexing(commit_vote_key, 1024, u32_prefix.clone()),
            ),
            (
                Self::COMMIT_INFO_CF.to_string(),
                ThConfig::new_with_indexing(index_digest_key.clone(), MUTEXES, u32_prefix.clone()),
            ),
        ];
        Self::open_tables_read_write(
            path.into(),
            MetricConf::new("consensus")
                .with_sampling(SamplingInterval::new(Duration::from_secs(60), 0)),
            configs.into_iter().collect(),
        )
    }
}

impl Store for RocksDBStore {
    fn write(&self, write_batch: WriteBatch) -> ConsensusResult<()> {
        fail_point!("consensus-store-before-write");

        let mut batch = self.blocks.batch();
        for block in write_batch.blocks {
            let block_ref = block.reference();
            batch
                .insert_batch(
                    &self.blocks,
                    [(
                        (block_ref.round, block_ref.author, block_ref.digest),
                        block.serialized(),
                    )],
                )
                .map_err(ConsensusError::RocksDBFailure)?;
            batch
                .insert_batch(
                    &self.digests_by_authorities,
                    [((block_ref.author, block_ref.round, block_ref.digest), ())],
                )
                .map_err(ConsensusError::RocksDBFailure)?;
            for vote in block.commit_votes() {
                batch
                    .insert_batch(
                        &self.commit_votes,
                        [((vote.index, vote.digest, block_ref), ())],
                    )
                    .map_err(ConsensusError::RocksDBFailure)?;
            }
        }

        for commit in write_batch.commits {
            batch
                .insert_batch(
                    &self.commits,
                    [((commit.index(), commit.digest()), commit.serialized())],
                )
                .map_err(ConsensusError::RocksDBFailure)?;
        }

        for (commit_ref, commit_info) in write_batch.commit_info {
            batch
                .insert_batch(
                    &self.commit_info,
                    [((commit_ref.index, commit_ref.digest), commit_info)],
                )
                .map_err(ConsensusError::RocksDBFailure)?;
        }

        batch.write()?;
        fail_point!("consensus-store-after-write");
        Ok(())
    }

    fn read_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<Option<VerifiedBlock>>> {
        let keys = refs
            .iter()
            .map(|r| (r.round, r.author, r.digest))
            .collect::<Vec<_>>();
        let serialized = self.blocks.multi_get(keys)?;
        let mut blocks = vec![];
        for (key, serialized) in refs.iter().zip(serialized) {
            if let Some(serialized) = serialized {
                let signed_block: SignedBlock =
                    bcs::from_bytes(&serialized).map_err(ConsensusError::MalformedBlock)?;
                // Only accepted blocks should have been written to storage.
                let block = VerifiedBlock::new_verified(signed_block, serialized);
                // Makes sure block data is not corrupted, by comparing digests.
                assert_eq!(*key, block.reference());
                blocks.push(Some(block));
            } else {
                blocks.push(None);
            }
        }
        Ok(blocks)
    }

    fn contains_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<bool>> {
        let refs = refs
            .iter()
            .map(|r| (r.round, r.author, r.digest))
            .collect::<Vec<_>>();
        let exist = self.blocks.multi_contains_keys(refs)?;
        Ok(exist)
    }

    fn scan_blocks_by_author(
        &self,
        author: AuthorityIndex,
        start_round: Round,
    ) -> ConsensusResult<Vec<VerifiedBlock>> {
        let mut refs = vec![];
        for kv in self.digests_by_authorities.safe_range_iter((
            Included((author, start_round, BlockDigest::MIN)),
            Included((author, Round::MAX, BlockDigest::MAX)),
        )) {
            let ((author, round, digest), _) = kv?;
            refs.push(BlockRef::new(round, author, digest));
        }
        let results = self.read_blocks(refs.as_slice())?;
        let mut blocks = Vec::with_capacity(refs.len());
        for (r, block) in refs.into_iter().zip(results.into_iter()) {
            blocks.push(
                block.unwrap_or_else(|| panic!("Storage inconsistency: block {:?} not found!", r)),
            );
        }
        Ok(blocks)
    }

    // The method returns the last `num_of_rounds` rounds blocks by author in round ascending order.
    // When a `before_round` is defined then the blocks of round `<=before_round` are returned. If not
    // then the max value for round will be used as cut off.
    fn scan_last_blocks_by_author(
        &self,
        author: AuthorityIndex,
        num_of_rounds: u64,
        before_round: Option<Round>,
    ) -> ConsensusResult<Vec<VerifiedBlock>> {
        let before_round = before_round.unwrap_or(Round::MAX);
        let mut refs = VecDeque::new();
        for kv in self
            .digests_by_authorities
            .reversed_safe_iter_with_bounds(
                Some((author, Round::MIN, BlockDigest::MIN)),
                Some((author, before_round, BlockDigest::MAX)),
            )?
            .take(num_of_rounds as usize)
        {
            let ((author, round, digest), _) = kv?;
            refs.push_front(BlockRef::new(round, author, digest));
        }
        let results = self.read_blocks(refs.as_slices().0)?;
        let mut blocks = vec![];
        for (r, block) in refs.into_iter().zip(results.into_iter()) {
            blocks.push(
                block.unwrap_or_else(|| panic!("Storage inconsistency: block {:?} not found!", r)),
            );
        }
        Ok(blocks)
    }

    fn read_last_commit(&self) -> ConsensusResult<Option<TrustedCommit>> {
        let Some(result) = self
            .commits
            .reversed_safe_iter_with_bounds(None, None)?
            .next()
        else {
            return Ok(None);
        };
        let ((_index, digest), serialized) = result?;
        let commit = TrustedCommit::new_trusted(
            bcs::from_bytes(&serialized).map_err(ConsensusError::MalformedCommit)?,
            serialized,
        );
        assert_eq!(commit.digest(), digest);
        Ok(Some(commit))
    }

    fn scan_commits(&self, range: CommitRange) -> ConsensusResult<Vec<TrustedCommit>> {
        let mut commits = vec![];
        for result in self.commits.safe_range_iter((
            Included((range.start(), CommitDigest::MIN)),
            Included((range.end(), CommitDigest::MAX)),
        )) {
            let ((_index, digest), serialized) = result?;
            let commit = TrustedCommit::new_trusted(
                bcs::from_bytes(&serialized).map_err(ConsensusError::MalformedCommit)?,
                serialized,
            );
            assert_eq!(commit.digest(), digest);
            commits.push(commit);
        }
        Ok(commits)
    }

    fn read_commit_votes(&self, commit_index: CommitIndex) -> ConsensusResult<Vec<BlockRef>> {
        let mut votes = Vec::new();
        for vote in self.commit_votes.safe_range_iter((
            Included((commit_index, CommitDigest::MIN, BlockRef::MIN)),
            Included((commit_index, CommitDigest::MAX, BlockRef::MAX)),
        )) {
            let ((_, _, block_ref), _) = vote?;
            votes.push(block_ref);
        }
        Ok(votes)
    }

    fn read_last_commit_info(&self) -> ConsensusResult<Option<(CommitRef, CommitInfo)>> {
        let Some(result) = self
            .commit_info
            .reversed_safe_iter_with_bounds(None, None)?
            .next()
        else {
            return Ok(None);
        };
        let (key, commit_info) = result.map_err(ConsensusError::RocksDBFailure)?;
        Ok(Some((CommitRef::new(key.0, key.1), commit_info)))
    }
}
