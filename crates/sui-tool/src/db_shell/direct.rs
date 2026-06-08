// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Direct RocksDB backend for the db-shell.
//! Used when sui-node is not running and the database files can be opened directly.

use anyhow::{Context, bail};
use consensus_core::{
    CommitAPI, CommitIndex, CommitRange,
    storage::{Store as ConsensusStore, rocksdb_store::RocksDBStore},
};
use std::sync::Arc;
use sui_core::{
    authority::authority_store_tables::AuthorityPerpetualTables, checkpoints::CheckpointStore,
    epoch::committee_store::CommitteeStore,
};
use sui_types::{
    base_types::EpochId,
    digests::{CheckpointContentsDigest, CheckpointDigest, TransactionDigest},
    messages_checkpoint::{CheckpointContents, CheckpointSequenceNumber, VerifiedCheckpoint},
};

use crate::db_shell::{
    backend::{Backend, DirEntry},
    vfs::VfsPath,
};

pub struct DirectBackend {
    pub checkpoint_store: Arc<CheckpointStore>,
    pub committee_store: Arc<CommitteeStore>,
    pub authority_tables: Arc<AuthorityPerpetualTables>,
    pub consensus_store: Option<Arc<RocksDBStore>>,
}

impl DirectBackend {
    fn list_checkpoints_from(
        &self,
        start: Option<CheckpointSequenceNumber>,
        limit: usize,
    ) -> anyhow::Result<Vec<DirEntry>> {
        Ok(self
            .checkpoint_store
            .list_checkpoints_from_seq(start, limit)?
            .into_iter()
            .map(|(seq, _)| DirEntry {
                name: seq.to_string(),
                is_dir: true,
            })
            .collect())
    }

    fn list_digest_dirs(
        &self,
        start: Option<CheckpointDigest>,
        limit: usize,
    ) -> anyhow::Result<Vec<DirEntry>> {
        Ok(self
            .checkpoint_store
            .list_checkpoint_digests(start, limit)?
            .into_iter()
            .map(|d| DirEntry {
                name: d.to_string(),
                is_dir: true,
            })
            .collect())
    }

    fn list_contents_entries(
        &self,
        start: Option<CheckpointContentsDigest>,
        limit: usize,
    ) -> anyhow::Result<Vec<DirEntry>> {
        Ok(self
            .checkpoint_store
            .list_checkpoint_contents_digests(start, limit)?
            .into_iter()
            .map(|d| DirEntry {
                name: d.to_string(),
                is_dir: false,
            })
            .collect())
    }

    fn list_epoch_checkpoint_dirs(
        &self,
        epoch: EpochId,
        start: Option<CheckpointSequenceNumber>,
        limit: usize,
    ) -> anyhow::Result<Vec<DirEntry>> {
        Ok(self
            .checkpoint_store
            .list_epoch_checkpoints(epoch, start, limit)?
            .into_iter()
            .map(|(seq, _)| DirEntry {
                name: seq.to_string(),
                is_dir: false,
            })
            .collect())
    }

    fn list_transactions_from(
        &self,
        start: Option<TransactionDigest>,
        limit: usize,
    ) -> anyhow::Result<Vec<DirEntry>> {
        let tx_digests = self.authority_tables.list_transactions_from(start, limit)?;
        let mut entries = Vec::with_capacity(tx_digests.len() * 2);
        for digest in &tx_digests {
            entries.push(DirEntry {
                name: digest.to_string(),
                is_dir: false,
            });
            if let Ok(Some(fx_digest)) = self.authority_tables.get_executed_effects_digest(digest) {
                entries.push(DirEntry {
                    name: format!("{digest}.fx-{fx_digest}"),
                    is_dir: false,
                });
            }
        }
        Ok(entries)
    }

    fn list_consensus_commits(
        &self,
        start: Option<CommitIndex>,
        limit: usize,
    ) -> anyhow::Result<Vec<DirEntry>> {
        let cs = self.consensus_store.as_ref().ok_or_else(|| {
            anyhow::anyhow!("consensus store not available; use --consensus-db-path")
        })?;
        let start_idx = start.unwrap_or(1);
        let commits = cs
            .scan_commits(CommitRange::new(start_idx..=CommitIndex::MAX))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(commits
            .into_iter()
            .take(limit)
            .map(|c| DirEntry {
                name: c.index().to_string(),
                is_dir: true,
            })
            .collect())
    }

    fn get_checkpoint_by_seq(
        &self,
        seq: CheckpointSequenceNumber,
    ) -> anyhow::Result<VerifiedCheckpoint> {
        self.checkpoint_store
            .get_checkpoint_by_sequence_number(seq)?
            .with_context(|| format!("checkpoint {seq} not found"))
    }

    fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> anyhow::Result<VerifiedCheckpoint> {
        self.checkpoint_store
            .get_checkpoint_by_digest(digest)?
            .with_context(|| format!("checkpoint {digest} not found"))
    }

    fn get_checkpoint_contents(
        &self,
        cp: &VerifiedCheckpoint,
    ) -> anyhow::Result<CheckpointContents> {
        self.checkpoint_store
            .get_checkpoint_contents(&cp.content_digest)?
            .with_context(|| format!("contents for checkpoint {} not found", cp.sequence_number()))
    }

    fn contents_short_text(&self, contents: &CheckpointContents) -> String {
        let mut out = String::new();
        for ed in contents.iter() {
            out.push_str(&format!("{} {}\n", ed.transaction, ed.effects));
        }
        out
    }

    fn contents_short_json(
        &self,
        contents: &CheckpointContents,
    ) -> anyhow::Result<serde_json::Value> {
        let pairs: Vec<_> = contents
            .iter()
            .map(|ed| {
                serde_json::json!({
                    "transaction": ed.transaction.to_string(),
                    "effects": ed.effects.to_string(),
                })
            })
            .collect();
        Ok(serde_json::Value::Array(pairs))
    }

    fn summary_json(&self, cp: &VerifiedCheckpoint) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(cp.data())?)
    }

    fn summary_debug(&self, cp: &VerifiedCheckpoint) -> String {
        format!("{:#?}", cp.data())
    }

    fn summary_bcs(&self, cp: &VerifiedCheckpoint) -> anyhow::Result<Vec<u8>> {
        Ok(bcs::to_bytes(cp.data())?)
    }

    fn contents_json(&self, contents: &CheckpointContents) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(contents)?)
    }

    fn contents_debug(&self, contents: &CheckpointContents) -> String {
        format!("{contents:#?}")
    }

    fn contents_bcs(&self, contents: &CheckpointContents) -> anyhow::Result<Vec<u8>> {
        Ok(bcs::to_bytes(contents)?)
    }

    fn commit_summary_json(&self, index: CommitIndex) -> anyhow::Result<serde_json::Value> {
        let cs = self.consensus_store.as_ref().ok_or_else(|| {
            anyhow::anyhow!("consensus store not available; use --consensus-db-path")
        })?;
        let summary =
            sui_core::consensus_commit_summary::build_consensus_commit_summary(cs, index)?
                .ok_or_else(|| anyhow::anyhow!("commit {index} not found"))?;
        let commit = &summary.commit;
        Ok(serde_json::json!({
            "index": commit.index(),
            "timestamp_ms": commit.timestamp_ms(),
            "leader": commit.leader().to_string(),
            "previous_digest": commit.previous_digest().to_string(),
            "block_count": commit.blocks().len(),
            "transactions": summary.tx_keys,
            "missing_blocks": summary.missing_blocks,
        }))
    }

    fn commit_summary_debug(&self, index: CommitIndex) -> anyhow::Result<String> {
        let json = self.commit_summary_json(index)?;
        Ok(format!("{json:#}"))
    }
}

impl Backend for DirectBackend {
    fn ls_children(&self, path: &VfsPath, limit: usize) -> anyhow::Result<Vec<DirEntry>> {
        match path {
            VfsPath::Root => Ok(vec![
                DirEntry {
                    name: "epochs".into(),
                    is_dir: true,
                },
                DirEntry {
                    name: "checkpoints".into(),
                    is_dir: true,
                },
                DirEntry {
                    name: "checkpoint-contents".into(),
                    is_dir: true,
                },
                DirEntry {
                    name: "transactions".into(),
                    is_dir: true,
                },
                DirEntry {
                    name: "consensus".into(),
                    is_dir: true,
                },
            ]),
            VfsPath::Epochs => {
                let epochs = self.committee_store.list_epochs(None, limit)?;
                Ok(epochs
                    .into_iter()
                    .map(|(id, _)| DirEntry {
                        name: id.to_string(),
                        is_dir: true,
                    })
                    .collect())
            }
            VfsPath::Epoch(_) => Ok(vec![
                DirEntry {
                    name: "first-checkpoint".into(),
                    is_dir: false,
                },
                DirEntry {
                    name: "last-checkpoint".into(),
                    is_dir: false,
                },
                DirEntry {
                    name: "committee".into(),
                    is_dir: false,
                },
                DirEntry {
                    name: "checkpoints".into(),
                    is_dir: true,
                },
            ]),
            VfsPath::EpochCheckpoints(epoch) => {
                self.list_epoch_checkpoint_dirs(*epoch, None, limit)
            }
            VfsPath::CheckpointsRoot => Ok(vec![
                DirEntry {
                    name: "seq".into(),
                    is_dir: true,
                },
                DirEntry {
                    name: "digest".into(),
                    is_dir: true,
                },
            ]),
            VfsPath::CheckpointsSeqRoot => self.list_checkpoints_from(None, limit),
            VfsPath::CheckpointsBySeq(_) => Ok(vec![
                DirEntry {
                    name: "summary".into(),
                    is_dir: false,
                },
                DirEntry {
                    name: "contents".into(),
                    is_dir: false,
                },
                DirEntry {
                    name: "contents-short".into(),
                    is_dir: false,
                },
            ]),
            VfsPath::CheckpointsDigestRoot => self.list_digest_dirs(None, limit),
            VfsPath::CheckpointsByDigest(_) => Ok(vec![
                DirEntry {
                    name: "summary".into(),
                    is_dir: false,
                },
                DirEntry {
                    name: "contents".into(),
                    is_dir: false,
                },
                DirEntry {
                    name: "contents-short".into(),
                    is_dir: false,
                },
            ]),
            VfsPath::CheckpointContentsRoot => self.list_contents_entries(None, limit),
            VfsPath::TransactionsRoot => self.list_transactions_from(None, limit),
            VfsPath::ConsensusRoot => Ok(vec![
                DirEntry {
                    name: "latest".into(),
                    is_dir: false,
                },
                DirEntry {
                    name: "commits".into(),
                    is_dir: true,
                },
            ]),
            VfsPath::ConsensusCommitsRoot => self.list_consensus_commits(None, limit),
            VfsPath::ConsensusCommitDir(_) => Ok(vec![DirEntry {
                name: "summary".into(),
                is_dir: false,
            }]),
            _ => bail!("'{}' is not a directory", path),
        }
    }

    fn ls_cursor(&self, path: &VfsPath, limit: usize) -> anyhow::Result<Vec<DirEntry>> {
        match path {
            VfsPath::CheckpointsBySeq(seq) => self.list_checkpoints_from(Some(*seq), limit),
            VfsPath::CheckpointsByDigest(d) => self.list_digest_dirs(Some(*d), limit),
            VfsPath::EpochCheckpointBySeq(epoch, seq) => {
                self.list_epoch_checkpoint_dirs(*epoch, Some(*seq), limit)
            }
            VfsPath::CheckpointContentsEntry(d) => self.list_contents_entries(Some(*d), limit),
            VfsPath::TransactionEntry(d) => self.list_transactions_from(Some(*d), limit),
            VfsPath::ConsensusCommitDir(i) => self.list_consensus_commits(Some(*i), limit),
            // For non-cursor paths, fall back to children listing.
            _ => self.ls_children(path, limit),
        }
    }

    fn read_json(&self, path: &VfsPath) -> anyhow::Result<serde_json::Value> {
        match path {
            VfsPath::EpochFirstCheckpoint(epoch) => {
                let seq = self
                    .checkpoint_store
                    .get_epoch_first_checkpoint_seq(*epoch)?
                    .with_context(|| format!("no data for epoch {epoch}"))?;
                let cp = self.get_checkpoint_by_seq(seq)?;
                self.summary_json(&cp)
            }
            VfsPath::EpochLastCheckpoint(epoch) => {
                let cp = self
                    .checkpoint_store
                    .get_epoch_last_checkpoint(*epoch)?
                    .with_context(|| format!("no last checkpoint for epoch {epoch}"))?;
                self.summary_json(&cp)
            }
            VfsPath::EpochCommittee(epoch) => {
                let committee = self
                    .committee_store
                    .get_committee(epoch)?
                    .with_context(|| format!("no committee for epoch {epoch}"))?;
                Ok(serde_json::to_value(committee.as_ref())?)
            }
            VfsPath::EpochCheckpointBySeq(_epoch, seq) => {
                let cp = self.get_checkpoint_by_seq(*seq)?;
                self.summary_json(&cp)
            }
            VfsPath::EpochCheckpointByDigest(_epoch, digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                self.summary_json(&cp)
            }
            VfsPath::CheckpointSeqSummary(seq) => {
                let cp = self.get_checkpoint_by_seq(*seq)?;
                self.summary_json(&cp)
            }
            VfsPath::CheckpointSeqContents(seq) => {
                let cp = self.get_checkpoint_by_seq(*seq)?;
                let contents = self.get_checkpoint_contents(&cp)?;
                self.contents_json(&contents)
            }
            VfsPath::CheckpointSeqContentsShort(seq) => {
                let cp = self.get_checkpoint_by_seq(*seq)?;
                let contents = self.get_checkpoint_contents(&cp)?;
                self.contents_short_json(&contents)
            }
            VfsPath::CheckpointDigestSummary(digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                self.summary_json(&cp)
            }
            VfsPath::CheckpointDigestContents(digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                let contents = self.get_checkpoint_contents(&cp)?;
                self.contents_json(&contents)
            }
            VfsPath::CheckpointDigestContentsShort(digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                let contents = self.get_checkpoint_contents(&cp)?;
                self.contents_short_json(&contents)
            }
            VfsPath::CheckpointContentsEntry(digest) => {
                let contents = self
                    .checkpoint_store
                    .get_checkpoint_contents(digest)?
                    .with_context(|| format!("checkpoint contents {digest} not found"))?;
                self.contents_json(&contents)
            }
            VfsPath::TransactionEntry(digest) => {
                let tx = self
                    .authority_tables
                    .get_transaction(digest)?
                    .with_context(|| format!("transaction {digest} not found"))?;
                Ok(serde_json::to_value(&tx)?)
            }
            VfsPath::TransactionEffectsEntry(tx_digest, fx_digest) => {
                let effects = self
                    .authority_tables
                    .get_effects_by_digest(fx_digest)?
                    .with_context(|| format!("effects {fx_digest} for tx {tx_digest} not found"))?;
                Ok(serde_json::to_value(&effects)?)
            }
            VfsPath::ConsensusLatest => {
                let cs = self
                    .consensus_store
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("--consensus-db-path not provided"))?;
                let commit = cs
                    .read_last_commit()?
                    .ok_or_else(|| anyhow::anyhow!("no commits yet"))?;
                Ok(serde_json::json!({ "index": commit.index() }))
            }
            VfsPath::ConsensusCommitSummary(index) => self.commit_summary_json(*index),
            _ => bail!("'{}' is not readable", path),
        }
    }

    fn read_debug(&self, path: &VfsPath) -> anyhow::Result<String> {
        match path {
            VfsPath::EpochFirstCheckpoint(epoch) => {
                let seq = self
                    .checkpoint_store
                    .get_epoch_first_checkpoint_seq(*epoch)?
                    .with_context(|| format!("no data for epoch {epoch}"))?;
                let cp = self.get_checkpoint_by_seq(seq)?;
                Ok(self.summary_debug(&cp))
            }
            VfsPath::EpochLastCheckpoint(epoch) => {
                let cp = self
                    .checkpoint_store
                    .get_epoch_last_checkpoint(*epoch)?
                    .with_context(|| format!("no last checkpoint for epoch {epoch}"))?;
                Ok(self.summary_debug(&cp))
            }
            VfsPath::EpochCommittee(epoch) => {
                let committee = self
                    .committee_store
                    .get_committee(epoch)?
                    .with_context(|| format!("no committee for epoch {epoch}"))?;
                Ok(format!("{:#?}", committee.as_ref()))
            }
            VfsPath::EpochCheckpointBySeq(_epoch, seq) => {
                let cp = self.get_checkpoint_by_seq(*seq)?;
                Ok(self.summary_debug(&cp))
            }
            VfsPath::EpochCheckpointByDigest(_epoch, digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                Ok(self.summary_debug(&cp))
            }
            VfsPath::CheckpointSeqSummary(seq) => {
                let cp = self.get_checkpoint_by_seq(*seq)?;
                Ok(self.summary_debug(&cp))
            }
            VfsPath::CheckpointSeqContents(seq) => {
                let cp = self.get_checkpoint_by_seq(*seq)?;
                let contents = self.get_checkpoint_contents(&cp)?;
                Ok(self.contents_debug(&contents))
            }
            VfsPath::CheckpointSeqContentsShort(seq) => {
                let cp = self.get_checkpoint_by_seq(*seq)?;
                let contents = self.get_checkpoint_contents(&cp)?;
                Ok(self.contents_short_text(&contents))
            }
            VfsPath::CheckpointDigestSummary(digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                Ok(self.summary_debug(&cp))
            }
            VfsPath::CheckpointDigestContents(digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                let contents = self.get_checkpoint_contents(&cp)?;
                Ok(self.contents_debug(&contents))
            }
            VfsPath::CheckpointDigestContentsShort(digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                let contents = self.get_checkpoint_contents(&cp)?;
                Ok(self.contents_short_text(&contents))
            }
            VfsPath::CheckpointContentsEntry(digest) => {
                let contents = self
                    .checkpoint_store
                    .get_checkpoint_contents(digest)?
                    .with_context(|| format!("checkpoint contents {digest} not found"))?;
                Ok(self.contents_debug(&contents))
            }
            VfsPath::TransactionEntry(digest) => {
                let tx = self
                    .authority_tables
                    .get_transaction(digest)?
                    .with_context(|| format!("transaction {digest} not found"))?;
                Ok(format!("{tx:#?}"))
            }
            VfsPath::TransactionEffectsEntry(tx_digest, fx_digest) => {
                let effects = self
                    .authority_tables
                    .get_effects_by_digest(fx_digest)?
                    .with_context(|| format!("effects {fx_digest} for tx {tx_digest} not found"))?;
                Ok(format!("{effects:#?}"))
            }
            VfsPath::ConsensusLatest => {
                let cs = self
                    .consensus_store
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("--consensus-db-path not provided"))?;
                let commit = cs
                    .read_last_commit()?
                    .ok_or_else(|| anyhow::anyhow!("no commits yet"))?;
                Ok(commit.index().to_string())
            }
            VfsPath::ConsensusCommitSummary(index) => self.commit_summary_debug(*index),
            _ => bail!("'{}' is not readable", path),
        }
    }

    fn read_bcs(&self, path: &VfsPath) -> anyhow::Result<Vec<u8>> {
        match path {
            VfsPath::EpochFirstCheckpoint(epoch) => {
                let seq = self
                    .checkpoint_store
                    .get_epoch_first_checkpoint_seq(*epoch)?
                    .with_context(|| format!("no data for epoch {epoch}"))?;
                let cp = self.get_checkpoint_by_seq(seq)?;
                self.summary_bcs(&cp)
            }
            VfsPath::EpochLastCheckpoint(epoch) => {
                let cp = self
                    .checkpoint_store
                    .get_epoch_last_checkpoint(*epoch)?
                    .with_context(|| format!("no last checkpoint for epoch {epoch}"))?;
                self.summary_bcs(&cp)
            }
            VfsPath::EpochCommittee(epoch) => {
                let committee = self
                    .committee_store
                    .get_committee(epoch)?
                    .with_context(|| format!("no committee for epoch {epoch}"))?;
                Ok(bcs::to_bytes(committee.as_ref())?)
            }
            VfsPath::EpochCheckpointBySeq(_epoch, seq) => {
                let cp = self.get_checkpoint_by_seq(*seq)?;
                self.summary_bcs(&cp)
            }
            VfsPath::EpochCheckpointByDigest(_epoch, digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                self.summary_bcs(&cp)
            }
            VfsPath::CheckpointSeqSummary(seq) => {
                let cp = self.get_checkpoint_by_seq(*seq)?;
                self.summary_bcs(&cp)
            }
            VfsPath::CheckpointSeqContents(seq) => {
                let cp = self.get_checkpoint_by_seq(*seq)?;
                let contents = self.get_checkpoint_contents(&cp)?;
                self.contents_bcs(&contents)
            }
            VfsPath::CheckpointDigestSummary(digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                self.summary_bcs(&cp)
            }
            VfsPath::CheckpointDigestContents(digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                let contents = self.get_checkpoint_contents(&cp)?;
                self.contents_bcs(&contents)
            }
            VfsPath::CheckpointContentsEntry(digest) => {
                let contents = self
                    .checkpoint_store
                    .get_checkpoint_contents(digest)?
                    .with_context(|| format!("checkpoint contents {digest} not found"))?;
                self.contents_bcs(&contents)
            }
            VfsPath::TransactionEntry(digest) => {
                let tx = self
                    .authority_tables
                    .get_transaction(digest)?
                    .with_context(|| format!("transaction {digest} not found"))?;
                Ok(bcs::to_bytes(&tx)?)
            }
            VfsPath::TransactionEffectsEntry(tx_digest, fx_digest) => {
                let effects = self
                    .authority_tables
                    .get_effects_by_digest(fx_digest)?
                    .with_context(|| format!("effects {fx_digest} for tx {tx_digest} not found"))?;
                Ok(bcs::to_bytes(&effects)?)
            }
            _ => bail!(
                "'{}' is not readable or bcs not supported for this entry",
                path
            ),
        }
    }

    fn delete(&self, _path: &VfsPath) -> anyhow::Result<()> {
        bail!("delete not yet implemented for direct mode")
    }
}
