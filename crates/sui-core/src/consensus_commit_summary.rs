// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared helper for db-shell's consensus-commit summary, used by both the
//! in-process admin API (sui-node) and the direct RocksDB backend (sui-tool).

use consensus_core::{
    BlockAPI, CommitAPI, CommitIndex, CommitRange, CommitRef, TrustedCommit,
    storage::{Store as ConsensusStore, rocksdb_store::RocksDBStore},
};
use itertools::Itertools;
use sui_types::messages_consensus::ConsensusTransaction;

/// Summary of a single consensus commit. `tx_keys` entries for rejected
/// transactions are tagged with a `[rejected]` suffix.
pub struct ConsensusCommitSummary {
    pub commit: TrustedCommit,
    pub tx_keys: Vec<String>,
    /// Refs of blocks not found in storage; typically indicates corruption or a bug.
    pub missing_blocks: Vec<String>,
}

/// Build a summary for the commit at `index`. Returns `None` if not found.
pub fn build_consensus_commit_summary(
    cs: &RocksDBStore,
    index: CommitIndex,
) -> anyhow::Result<Option<ConsensusCommitSummary>> {
    let commits = cs
        .scan_commits(CommitRange::new(index..=index))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let Some(commit) = commits.into_iter().next() else {
        return Ok(None);
    };

    let commit_ref: CommitRef = commit.reference();
    let block_refs: Vec<_> = commit.blocks().to_vec();
    let blocks = cs
        .read_blocks(&block_refs)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let rejected = cs
        .read_rejected_transactions(commit_ref)
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .unwrap_or_default();

    let mut tx_keys: Vec<String> = Vec::new();
    let mut missing_blocks: Vec<String> = Vec::new();
    for (block_ref, block_opt) in block_refs.iter().zip_eq(blocks) {
        let Some(block) = block_opt else {
            missing_blocks.push(format!("{block_ref:?}"));
            continue;
        };
        let rejected_indices: std::collections::HashSet<u16> = rejected
            .get(block_ref)
            .map(|v| v.iter().copied().collect())
            .unwrap_or_default();
        for (i, tx_bytes) in block.transactions_data().iter().enumerate() {
            if let Ok(tx) = bcs::from_bytes::<ConsensusTransaction>(tx_bytes) {
                let key = format!("{:?}", tx.key());
                if rejected_indices.contains(&(i as u16)) {
                    tx_keys.push(format!("{key} [rejected]"));
                } else {
                    tx_keys.push(key);
                }
            }
        }
    }

    Ok(Some(ConsensusCommitSummary {
        commit,
        tx_keys,
        missing_blocks,
    }))
}
