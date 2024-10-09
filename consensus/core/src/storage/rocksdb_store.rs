// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::VecDeque, ops::Bound::Included, time::Duration};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use sui_macros::fail_point;
use typed_store::{
    metrics::SamplingInterval,
    reopen,
    rocks::{default_db_options, open_cf_opts, DBMap, MetricConf, ReadWriteOptions},
    Map as _,
};

use super::{CommitInfo, Store, WriteBatch};
use crate::{
    block::{BlockAPI as _, BlockDigest, BlockRef, Round, SignedBlock, Slot, VerifiedBlock},
    commit::{CommitAPI as _, CommitDigest, CommitIndex, CommitRange, CommitRef, TrustedCommit},
    error::{ConsensusError, ConsensusResult},
};

/// Persistent storage with RocksDB.
pub(crate) struct RocksDBStore {
    /// Stores SignedBlock by refs.
    blocks: DBMap<(Round, AuthorityIndex, BlockDigest), Bytes>,
    /// A secondary index that orders refs first by authors.
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
    pub(crate) fn new(path: &str) -> Self {
        // Consensus data has high write throughput (all transactions) and is rarely read
        // (only during recovery and when helping peers catch up).
        let db_options = default_db_options().optimize_db_for_write_throughput(2);
        let mut metrics_conf = MetricConf::new("consensus");
        metrics_conf.read_sample_interval = SamplingInterval::new(Duration::from_secs(60), 0);
        let cf_options = default_db_options().optimize_for_write_throughput().options;
        let column_family_options = vec![
            (
                Self::BLOCKS_CF,
                default_db_options()
                    .optimize_for_write_throughput_no_deletion()
                    // Using larger block is ok since there is not much point reads on the cf.
                    .set_block_options(512, 128 << 10)
                    .options,
            ),
            (Self::DIGESTS_BY_AUTHORITIES_CF, cf_options.clone()),
            (Self::COMMITS_CF, cf_options.clone()),
            (Self::COMMIT_VOTES_CF, cf_options.clone()),
            (Self::COMMIT_INFO_CF, cf_options.clone()),
        ];
        let rocksdb = open_cf_opts(
            path,
            Some(db_options.options),
            metrics_conf,
            &column_family_options,
        )
        .expect("Cannot open database");

        let (blocks, digests_by_authorities, commits, commit_votes, commit_info) = reopen!(&rocksdb,
            Self::BLOCKS_CF;<(Round, AuthorityIndex, BlockDigest), bytes::Bytes>,
            Self::DIGESTS_BY_AUTHORITIES_CF;<(AuthorityIndex, Round, BlockDigest), ()>,
            Self::COMMITS_CF;<(CommitIndex, CommitDigest), Bytes>,
            Self::COMMIT_VOTES_CF;<(CommitIndex, CommitDigest, BlockRef), ()>,
            Self::COMMIT_INFO_CF;<(CommitIndex, CommitDigest), CommitInfo>
        );

        Self {
            blocks,
            digests_by_authorities,
            commits,
            commit_votes,
            commit_info,
        }
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

    fn contains_block_at_slot(&self, slot: Slot) -> ConsensusResult<bool> {
        let found = self
            .digests_by_authorities
            .safe_range_iter((
                Included((slot.authority, slot.round, BlockDigest::MIN)),
                Included((slot.authority, slot.round, BlockDigest::MAX)),
            ))
            .next()
            .is_some();
        Ok(found)
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
            .safe_range_iter((
                Included((author, Round::MIN, BlockDigest::MIN)),
                Included((author, before_round, BlockDigest::MAX)),
            ))
            .skip_to_last()
            .reverse()
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
        let Some(result) = self.commits.safe_iter().skip_to_last().next() else {
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
        let Some(result) = self.commit_info.safe_iter().skip_to_last().next() else {
            return Ok(None);
        };
        let (key, commit_info) = result.map_err(ConsensusError::RocksDBFailure)?;
        Ok(Some((CommitRef::new(key.0, key.1), commit_info)))
    }
}
