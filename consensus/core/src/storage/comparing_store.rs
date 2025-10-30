// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering},
};

use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round, TransactionIndex};

use super::{
    rocksdb_store::RocksDBStore, tidehunter_store::TidehunterStore, CommitInfo, Store, WriteBatch,
};
use crate::{
    block::VerifiedBlock,
    commit::{CommitIndex, CommitRange, CommitRef, TrustedCommit},
    error::ConsensusResult,
};

/// Store implementation that holds both RocksDBStore and TidehunterStore
/// and compares their results, logging any discrepancies.
pub struct ComparingStore {
    rocksdb_store: RocksDBStore,
    tidehunter_store: TidehunterStore,
    discrepancy_count: AtomicUsize,
}

impl ComparingStore {
    const MAX_DISCREPANCIES_TO_LOG: usize = 20;

    pub fn new(base_path: &str) -> Self {
        let rocksdb_path = format!("{}/rocks", base_path);
        let tidehunter_path = format!("{}/tidehunter", base_path);

        Self {
            rocksdb_store: RocksDBStore::new(&rocksdb_path),
            tidehunter_store: TidehunterStore::new(&tidehunter_path),
            discrepancy_count: AtomicUsize::new(0),
        }
    }

    fn should_log_discrepancy(&self) -> bool {
        let count = self.discrepancy_count.fetch_add(1, Ordering::Relaxed);
        if count < Self::MAX_DISCREPANCIES_TO_LOG {
            true
        } else if count == Self::MAX_DISCREPANCIES_TO_LOG {
            tracing::warn!("[ComparingStore] Reached maximum of {} logged discrepancies, suppressing further logs", Self::MAX_DISCREPANCIES_TO_LOG);
            false
        } else {
            false
        }
    }

    fn compare_blocks(&self, method: &str, rocks: &[Option<VerifiedBlock>], tide: &[Option<VerifiedBlock>]) {
        if rocks.len() != tide.len() {
            if self.should_log_discrepancy() {
                tracing::error!(
                    "[ComparingStore::{method}] Length mismatch: rocksdb={}, tidehunter={}",
                    rocks.len(),
                    tide.len()
                );
            }
            return;
        }

        for (i, (rock_block, tide_block)) in rocks.iter().zip(tide.iter()).enumerate() {
            match (rock_block, tide_block) {
                (Some(r), Some(t)) => {
                    if r.reference() != t.reference() {
                        if self.should_log_discrepancy() {
                            tracing::error!(
                                "[ComparingStore::{method}] Block reference mismatch at index {}: rocksdb={:?}, tidehunter={:?}",
                                i,
                                r.reference(),
                                t.reference()
                            );
                        }
                    }
                    if r.serialized() != t.serialized() {
                        if self.should_log_discrepancy() {
                            tracing::error!(
                                "[ComparingStore::{method}] Block serialization mismatch at index {} for ref {:?}",
                                i,
                                r.reference()
                            );
                        }
                    }
                }
                (None, None) => {}
                (Some(r), None) => {
                    if self.should_log_discrepancy() {
                        tracing::error!(
                            "[ComparingStore::{method}] Block at index {} exists in rocksdb ({:?}) but not in tidehunter",
                            i,
                            r.reference()
                        );
                    }
                }
                (None, Some(t)) => {
                    if self.should_log_discrepancy() {
                        tracing::error!(
                            "[ComparingStore::{method}] Block at index {} exists in tidehunter ({:?}) but not in rocksdb",
                            i,
                            t.reference()
                        );
                    }
                }
            }
        }
    }

    fn compare_commits(&self, method: &str, rocks: &[TrustedCommit], tide: &[TrustedCommit]) {
        if rocks.len() != tide.len() {
            if self.should_log_discrepancy() {
                tracing::error!(
                    "[ComparingStore::{method}] Length mismatch: rocksdb={}, tidehunter={}",
                    rocks.len(),
                    tide.len()
                );
            }
            return;
        }

        for (i, (rock_commit, tide_commit)) in rocks.iter().zip(tide.iter()).enumerate() {
            if rock_commit.reference() != tide_commit.reference() {
                if self.should_log_discrepancy() {
                    tracing::error!(
                        "[ComparingStore::{method}] Commit reference mismatch at index {}: rocksdb={:?}, tidehunter={:?}",
                        i,
                        rock_commit.reference(),
                        tide_commit.reference()
                    );
                }
            }
            if rock_commit.serialized() != tide_commit.serialized() {
                if self.should_log_discrepancy() {
                    tracing::error!(
                        "[ComparingStore::{method}] Commit serialization mismatch at index {} for ref {:?}",
                        i,
                        rock_commit.reference()
                    );
                }
            }
        }
    }

    fn compare_block_refs(&self, method: &str, rocks: &[BlockRef], tide: &[BlockRef]) {
        if rocks.len() != tide.len() {
            if self.should_log_discrepancy() {
                tracing::error!(
                    "[ComparingStore::{method}] Length mismatch: rocksdb={}, tidehunter={}",
                    rocks.len(),
                    tide.len()
                );
            }
            return;
        }

        for (i, (rock_ref, tide_ref)) in rocks.iter().zip(tide.iter()).enumerate() {
            if rock_ref != tide_ref {
                if self.should_log_discrepancy() {
                    tracing::error!(
                        "[ComparingStore::{method}] BlockRef mismatch at index {}: rocksdb={:?}, tidehunter={:?}",
                        i,
                        rock_ref,
                        tide_ref
                    );
                }
            }
        }
    }

    fn compare_bool_vec(&self, method: &str, rocks: &[bool], tide: &[bool]) {
        if rocks.len() != tide.len() {
            if self.should_log_discrepancy() {
                tracing::error!(
                    "[ComparingStore::{method}] Length mismatch: rocksdb={}, tidehunter={}",
                    rocks.len(),
                    tide.len()
                );
            }
            return;
        }

        for (i, (rock_val, tide_val)) in rocks.iter().zip(tide.iter()).enumerate() {
            if rock_val != tide_val {
                if self.should_log_discrepancy() {
                    tracing::error!(
                        "[ComparingStore::{method}] Value mismatch at index {}: rocksdb={}, tidehunter={}",
                        i,
                        rock_val,
                        tide_val
                    );
                }
            }
        }
    }

    fn compare_option_commit(
        &self,
        method: &str,
        rocks: &Option<TrustedCommit>,
        tide: &Option<TrustedCommit>,
    ) {
        match (rocks, tide) {
            (Some(r), Some(t)) => {
                if r.reference() != t.reference() {
                    if self.should_log_discrepancy() {
                        tracing::error!(
                            "[ComparingStore::{method}] Commit reference mismatch: rocksdb={:?}, tidehunter={:?}",
                            r.reference(),
                            t.reference()
                        );
                    }
                }
                if r.serialized() != t.serialized() {
                    if self.should_log_discrepancy() {
                        tracing::error!(
                            "[ComparingStore::{method}] Commit serialization mismatch for ref {:?}",
                            r.reference()
                        );
                    }
                }
            }
            (None, None) => {}
            (Some(r), None) => {
                if self.should_log_discrepancy() {
                    tracing::error!(
                        "[ComparingStore::{method}] Commit exists in rocksdb ({:?}) but not in tidehunter",
                        r.reference()
                    );
                }
            }
            (None, Some(t)) => {
                if self.should_log_discrepancy() {
                    tracing::error!(
                        "[ComparingStore::{method}] Commit exists in tidehunter ({:?}) but not in rocksdb",
                        t.reference()
                    );
                }
            }
        }
    }

    fn compare_option_commit_info(
        &self,
        method: &str,
        rocks: &Option<(CommitRef, CommitInfo)>,
        tide: &Option<(CommitRef, CommitInfo)>,
    ) {
        match (rocks, tide) {
            (Some((r_ref, _r_info)), Some((t_ref, _t_info))) => {
                if r_ref != t_ref {
                    if self.should_log_discrepancy() {
                        tracing::error!(
                            "[ComparingStore::{method}] CommitRef mismatch: rocksdb={:?}, tidehunter={:?}",
                            r_ref,
                            t_ref
                        );
                    }
                }
            }
            (None, None) => {}
            (Some((r_ref, _)), None) => {
                if self.should_log_discrepancy() {
                    tracing::error!(
                        "[ComparingStore::{method}] CommitInfo exists in rocksdb ({:?}) but not in tidehunter",
                        r_ref
                    );
                }
            }
            (None, Some((t_ref, _))) => {
                if self.should_log_discrepancy() {
                    tracing::error!(
                        "[ComparingStore::{method}] CommitInfo exists in tidehunter ({:?}) but not in rocksdb",
                        t_ref
                    );
                }
            }
        }
    }

    fn compare_option_commit_ref(
        &self,
        method: &str,
        rocks: &Option<CommitRef>,
        tide: &Option<CommitRef>,
    ) {
        match (rocks, tide) {
            (Some(r), Some(t)) => {
                if r != t {
                    if self.should_log_discrepancy() {
                        tracing::error!(
                            "[ComparingStore::{method}] CommitRef mismatch: rocksdb={:?}, tidehunter={:?}",
                            r,
                            t
                        );
                    }
                }
            }
            (None, None) => {}
            (Some(r), None) => {
                if self.should_log_discrepancy() {
                    tracing::error!(
                        "[ComparingStore::{method}] CommitRef exists in rocksdb ({:?}) but not in tidehunter",
                        r
                    );
                }
            }
            (None, Some(t)) => {
                if self.should_log_discrepancy() {
                    tracing::error!(
                        "[ComparingStore::{method}] CommitRef exists in tidehunter ({:?}) but not in rocksdb",
                        t
                    );
                }
            }
        }
    }

    fn compare_finalized_commits(
        &self,
        method: &str,
        rocks: &[(CommitRef, BTreeMap<BlockRef, Vec<TransactionIndex>>)],
        tide: &[(CommitRef, BTreeMap<BlockRef, Vec<TransactionIndex>>)],
    ) {
        if rocks.len() != tide.len() {
            if self.should_log_discrepancy() {
                tracing::error!(
                    "[ComparingStore::{method}] Length mismatch: rocksdb={}, tidehunter={}",
                    rocks.len(),
                    tide.len()
                );
            }
            return;
        }

        for (i, ((r_ref, r_map), (t_ref, t_map))) in rocks.iter().zip(tide.iter()).enumerate() {
            if r_ref != t_ref {
                if self.should_log_discrepancy() {
                    tracing::error!(
                        "[ComparingStore::{method}] CommitRef mismatch at index {}: rocksdb={:?}, tidehunter={:?}",
                        i,
                        r_ref,
                        t_ref
                    );
                }
            }
            if r_map != t_map {
                if self.should_log_discrepancy() {
                    tracing::error!(
                        "[ComparingStore::{method}] Rejected transactions map mismatch at index {} for ref {:?}",
                        i,
                        r_ref
                    );
                }
            }
        }
    }
}

impl Store for ComparingStore {
    fn write(&self, write_batch: WriteBatch) -> ConsensusResult<()> {
        tracing::debug!("[ComparingStore::write] Writing batch with {} blocks, {} commits, {} commit_info, {} finalized_commits",
            write_batch.blocks.len(),
            write_batch.commits.len(),
            write_batch.commit_info.len(),
            write_batch.finalized_commits.len()
        );

        let result_rocks = self.rocksdb_store.write(WriteBatch {
            blocks: write_batch.blocks.clone(),
            commits: write_batch.commits.clone(),
            commit_info: write_batch.commit_info.clone(),
            finalized_commits: write_batch.finalized_commits.clone(),
        });

        let result_tide = self.tidehunter_store.write(write_batch);

        match (&result_rocks, &result_tide) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(e), Ok(())) => {
                if self.should_log_discrepancy() {
                    tracing::error!("[ComparingStore::write] RocksDB failed but Tidehunter succeeded: {:?}", e);
                }
                result_rocks
            }
            (Ok(()), Err(e)) => {
                if self.should_log_discrepancy() {
                    tracing::error!("[ComparingStore::write] Tidehunter failed but RocksDB succeeded: {:?}", e);
                }
                result_tide
            }
            (Err(e1), Err(e2)) => {
                if self.should_log_discrepancy() {
                    tracing::error!("[ComparingStore::write] Both stores failed - RocksDB: {:?}, Tidehunter: {:?}", e1, e2);
                }
                result_rocks
            }
        }
    }

    fn read_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<Option<VerifiedBlock>>> {
        let result_rocks = self.rocksdb_store.read_blocks(refs)?;
        let result_tide = self.tidehunter_store.read_blocks(refs)?;

        self.compare_blocks("read_blocks", &result_rocks, &result_tide);

        Ok(result_tide)
    }

    fn contains_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<bool>> {
        let result_rocks = self.rocksdb_store.contains_blocks(refs)?;
        let result_tide = self.tidehunter_store.contains_blocks(refs)?;

        self.compare_bool_vec("contains_blocks", &result_rocks, &result_tide);

        Ok(result_tide)
    }

    fn scan_blocks_by_author(
        &self,
        authority: AuthorityIndex,
        start_round: Round,
    ) -> ConsensusResult<Vec<VerifiedBlock>> {
        let result_rocks = self.rocksdb_store.scan_blocks_by_author(authority, start_round)?;
        let result_tide = self.tidehunter_store.scan_blocks_by_author(authority, start_round)?;

        let rocks_opts: Vec<Option<VerifiedBlock>> = result_rocks.iter().map(|b| Some(b.clone())).collect();
        let tide_opts: Vec<Option<VerifiedBlock>> = result_tide.iter().map(|b| Some(b.clone())).collect();
        self.compare_blocks("scan_blocks_by_author", &rocks_opts, &tide_opts);

        Ok(result_tide)
    }

    fn scan_last_blocks_by_author(
        &self,
        author: AuthorityIndex,
        num_of_rounds: u64,
        before_round: Option<Round>,
    ) -> ConsensusResult<Vec<VerifiedBlock>> {
        let result_rocks = self.rocksdb_store.scan_last_blocks_by_author(author, num_of_rounds, before_round)?;
        let result_tide = self.tidehunter_store.scan_last_blocks_by_author(author, num_of_rounds, before_round)?;

        let rocks_opts: Vec<Option<VerifiedBlock>> = result_rocks.iter().map(|b| Some(b.clone())).collect();
        let tide_opts: Vec<Option<VerifiedBlock>> = result_tide.iter().map(|b| Some(b.clone())).collect();
        self.compare_blocks("scan_last_blocks_by_author", &rocks_opts, &tide_opts);

        Ok(result_tide)
    }

    fn read_last_commit(&self) -> ConsensusResult<Option<TrustedCommit>> {
        let result_rocks = self.rocksdb_store.read_last_commit()?;
        let result_tide = self.tidehunter_store.read_last_commit()?;

        self.compare_option_commit("read_last_commit", &result_rocks, &result_tide);

        Ok(result_tide)
    }

    fn scan_commits(&self, range: CommitRange) -> ConsensusResult<Vec<TrustedCommit>> {
        let result_rocks = self.rocksdb_store.scan_commits(range.clone())?;
        let result_tide = self.tidehunter_store.scan_commits(range)?;

        self.compare_commits("scan_commits", &result_rocks, &result_tide);

        Ok(result_tide)
    }

    fn read_commit_votes(&self, commit_index: CommitIndex) -> ConsensusResult<Vec<BlockRef>> {
        let result_rocks = self.rocksdb_store.read_commit_votes(commit_index)?;
        let result_tide = self.tidehunter_store.read_commit_votes(commit_index)?;

        self.compare_block_refs("read_commit_votes", &result_rocks, &result_tide);

        Ok(result_tide)
    }

    fn read_last_commit_info(&self) -> ConsensusResult<Option<(CommitRef, CommitInfo)>> {
        let result_rocks = self.rocksdb_store.read_last_commit_info()?;
        let result_tide = self.tidehunter_store.read_last_commit_info()?;

        self.compare_option_commit_info("read_last_commit_info", &result_rocks, &result_tide);

        Ok(result_tide)
    }

    fn read_last_finalized_commit(&self) -> ConsensusResult<Option<CommitRef>> {
        let result_rocks = self.rocksdb_store.read_last_finalized_commit()?;
        let result_tide = self.tidehunter_store.read_last_finalized_commit()?;

        self.compare_option_commit_ref("read_last_finalized_commit", &result_rocks, &result_tide);

        Ok(result_tide)
    }

    fn scan_finalized_commits(
        &self,
        range: CommitRange,
    ) -> ConsensusResult<Vec<(CommitRef, BTreeMap<BlockRef, Vec<TransactionIndex>>)>> {
        let result_rocks = self.rocksdb_store.scan_finalized_commits(range.clone())?;
        let result_tide = self.tidehunter_store.scan_finalized_commits(range)?;

        self.compare_finalized_commits("scan_finalized_commits", &result_rocks, &result_tide);

        Ok(result_tide)
    }
}
