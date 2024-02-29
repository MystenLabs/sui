// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::{
    ops::Bound::{Included, Unbounded},
    time::Duration,
};

use consensus_config::AuthorityIndex;
use typed_store::{
    metrics::SamplingInterval,
    reopen,
    rocks::{default_db_options, open_cf_opts, DBMap, MetricConf, ReadWriteOptions},
    Map as _,
};

use super::Store;
use crate::commit::{CommitAPI as _, TrustedCommit};
use crate::{
    block::{BlockDigest, BlockRef, Round, SignedBlock, VerifiedBlock},
    commit::{Commit, CommitIndex},
    error::{ConsensusError, ConsensusResult},
};

/// Persistent storage with RocksDB.
pub(crate) struct RocksDBStore {
    /// Stores SignedBlock by refs.
    blocks: DBMap<(Round, AuthorityIndex, BlockDigest), bytes::Bytes>,
    /// A secondary index that orders refs first by authors.
    digests_by_authorities: DBMap<(AuthorityIndex, Round, BlockDigest), ()>,
    /// Maps commit index to content.
    // TODO: Use Bytes for value. Add CommitDigest to key.
    commits: DBMap<CommitIndex, Commit>,
    /// Stores the last committed rounds per authority.
    last_committed_rounds: DBMap<(), Vec<Round>>,
}

#[allow(unused)]
impl RocksDBStore {
    const BLOCKS_CF: &'static str = "blocks";
    const DIGESTS_BY_AUTHORITIES_CF: &'static str = "digests";
    const COMMITS_CF: &'static str = "commits";
    const LAST_COMMITTED_ROUNDS_CF: &'static str = "last_committed_rounds";

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
                    .optimize_for_write_throughput()
                    // Blocks can get large and they don't need to be compacted.
                    // So keep them in rocksdb blobstore.
                    .optimize_for_large_values_no_scan(1 << 10)
                    .options,
            ),
            (Self::DIGESTS_BY_AUTHORITIES_CF, cf_options.clone()),
            (Self::COMMITS_CF, cf_options.clone()),
            (Self::LAST_COMMITTED_ROUNDS_CF, cf_options.clone()),
        ];
        let rocksdb = open_cf_opts(
            path,
            Some(db_options.options),
            metrics_conf,
            &column_family_options,
        )
        .expect("Cannot open database");

        let (blocks, digests_by_authorities, commits, last_committed_rounds) = reopen!(&rocksdb,
            Self::BLOCKS_CF;<(Round, AuthorityIndex, BlockDigest), bytes::Bytes>,
            Self::DIGESTS_BY_AUTHORITIES_CF;<(AuthorityIndex, Round, BlockDigest), ()>,
            Self::COMMITS_CF;<CommitIndex, Commit>,
            Self::LAST_COMMITTED_ROUNDS_CF;<(), Vec<Round>>
        );

        Self {
            blocks,
            digests_by_authorities,
            commits,
            last_committed_rounds,
        }
    }
}

#[allow(unused)]
impl Store for RocksDBStore {
    fn write(
        &self,
        blocks: Vec<VerifiedBlock>,
        commits: Vec<TrustedCommit>,
        last_committed_rounds: Vec<Round>,
    ) -> ConsensusResult<()> {
        let mut batch = self.blocks.batch();
        for block in blocks {
            let block_ref = block.reference();
            batch.insert_batch(
                &self.blocks,
                [(
                    (block_ref.round, block_ref.author, block_ref.digest),
                    block.serialized(),
                )],
            );
            batch.insert_batch(
                &self.digests_by_authorities,
                [((block_ref.author, block_ref.round, block_ref.digest), ())],
            );
        }
        for commit in commits {
            batch.insert_batch(&self.commits, [(commit.index(), commit.inner())]);
        }
        batch.insert_batch(&self.last_committed_rounds, [((), last_committed_rounds)]);
        batch.write()?;
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
        let mut blocks = vec![];
        for (r, block) in refs.into_iter().zip(results.into_iter()) {
            blocks.push(
                block.unwrap_or_else(|| panic!("Storage inconsistency: block {:?} not found!", r)),
            );
        }
        Ok(blocks)
    }

    // The method returns the last `num_of_rounds` rounds blocks by author in round ascending order.
    fn scan_last_blocks_by_author(
        &self,
        author: AuthorityIndex,
        num_of_rounds: u64,
    ) -> ConsensusResult<Vec<VerifiedBlock>> {
        let mut refs = VecDeque::new();
        for kv in self
            .digests_by_authorities
            .safe_range_iter((
                Included((author, Round::MIN, BlockDigest::MIN)),
                Included((author, Round::MAX, BlockDigest::MAX)),
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
        let Some(commit) = self.commits.safe_iter().skip_to_last().next() else {
            return Ok(None);
        };
        let (_, commit) = commit?;
        Ok(Some(TrustedCommit::new_trusted(commit)))
    }

    fn scan_commits(&self, start_commit_index: CommitIndex) -> ConsensusResult<Vec<TrustedCommit>> {
        let mut commits = vec![];
        for commit in self
            .commits
            .safe_range_iter((Included(start_commit_index), Unbounded))
        {
            let (_, commit) = commit?;
            commits.push(TrustedCommit::new_trusted(commit));
        }
        Ok(commits)
    }

    fn read_last_committed_rounds(&self) -> ConsensusResult<Vec<Round>> {
        let Some(rounds) = self.last_committed_rounds.safe_iter().next() else {
            return Ok(vec![]);
        };
        let (_, last_committed_rounds) = rounds?;
        Ok(last_committed_rounds)
    }
}
