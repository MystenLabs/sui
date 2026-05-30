// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Direct RocksDB backend for the db-shell.
//! Used when sui-node is not running and the database files can be opened directly.

use anyhow::{Context, bail};
use std::sync::Arc;
use sui_core::{checkpoints::CheckpointStore, epoch::committee_store::CommitteeStore};
use sui_types::{
    base_types::EpochId,
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::{CheckpointContents, CheckpointSequenceNumber, VerifiedCheckpoint},
};

use crate::db_shell::{
    backend::{Backend, DirEntry},
    vfs::VfsPath,
};

pub struct DirectBackend {
    pub checkpoint_store: Arc<CheckpointStore>,
    pub committee_store: Arc<CommitteeStore>,
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
            ]),
            VfsPath::CheckpointContentsRoot => self.list_contents_entries(None, limit),
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
            VfsPath::CheckpointDigestSummary(digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                self.summary_json(&cp)
            }
            VfsPath::CheckpointDigestContents(digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                let contents = self.get_checkpoint_contents(&cp)?;
                self.contents_json(&contents)
            }
            VfsPath::CheckpointContentsEntry(digest) => {
                let contents = self
                    .checkpoint_store
                    .get_checkpoint_contents(digest)?
                    .with_context(|| format!("checkpoint contents {digest} not found"))?;
                self.contents_json(&contents)
            }
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
            VfsPath::CheckpointDigestSummary(digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                Ok(self.summary_debug(&cp))
            }
            VfsPath::CheckpointDigestContents(digest) => {
                let cp = self.get_checkpoint_by_digest(digest)?;
                let contents = self.get_checkpoint_contents(&cp)?;
                Ok(self.contents_debug(&contents))
            }
            VfsPath::CheckpointContentsEntry(digest) => {
                let contents = self
                    .checkpoint_store
                    .get_checkpoint_contents(digest)?
                    .with_context(|| format!("checkpoint contents {digest} not found"))?;
                Ok(self.contents_debug(&contents))
            }
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
            _ => bail!("'{}' is not readable", path),
        }
    }

    fn delete(&self, _path: &VfsPath) -> anyhow::Result<()> {
        bail!("delete not yet implemented for direct mode")
    }
}
