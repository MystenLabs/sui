// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Virtual filesystem path types for the db-shell.

use anyhow::anyhow;
use std::fmt;
use sui_types::{
    base_types::EpochId,
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VfsPath {
    Root,
    Epochs,
    Epoch(EpochId),
    EpochFirstCheckpoint(EpochId),
    EpochLastCheckpoint(EpochId),
    EpochCommittee(EpochId),
    EpochCheckpoints(EpochId),
    EpochCheckpointBySeq(EpochId, CheckpointSequenceNumber),
    EpochCheckpointByDigest(EpochId, CheckpointDigest),
    CheckpointsRoot,
    CheckpointsSeqRoot,
    /// `/checkpoints/seq/<seq>`: directory containing `summary` and `contents`.
    /// When given as an explicit `ls` argument, acts as a start cursor.
    CheckpointsBySeq(CheckpointSequenceNumber),
    CheckpointSeqSummary(CheckpointSequenceNumber),
    CheckpointSeqContents(CheckpointSequenceNumber),
    CheckpointsDigestRoot,
    CheckpointsByDigest(CheckpointDigest),
    CheckpointDigestSummary(CheckpointDigest),
    CheckpointDigestContents(CheckpointDigest),
    CheckpointContentsRoot,
    CheckpointContentsEntry(CheckpointContentsDigest),
}

impl VfsPath {
    pub fn is_dir(&self) -> bool {
        matches!(
            self,
            VfsPath::Root
                | VfsPath::Epochs
                | VfsPath::Epoch(_)
                | VfsPath::EpochCheckpoints(_)
                | VfsPath::CheckpointsRoot
                | VfsPath::CheckpointsSeqRoot
                | VfsPath::CheckpointsBySeq(_)
                | VfsPath::CheckpointsDigestRoot
                | VfsPath::CheckpointsByDigest(_)
                | VfsPath::CheckpointContentsRoot
        )
    }

    /// True when this path, used as an explicit `ls` argument, should be treated as a
    /// start cursor in the parent namespace rather than listing its own children.
    pub fn is_ls_cursor(&self) -> bool {
        matches!(
            self,
            VfsPath::CheckpointsBySeq(_)
                | VfsPath::CheckpointsByDigest(_)
                | VfsPath::EpochCheckpointBySeq(_, _)
                | VfsPath::CheckpointContentsEntry(_)
        )
    }

    /// Return the parent path, or `None` if already at root.
    pub fn parent(&self) -> Option<VfsPath> {
        match self {
            VfsPath::Root => None,
            VfsPath::Epochs | VfsPath::CheckpointsRoot | VfsPath::CheckpointContentsRoot => {
                Some(VfsPath::Root)
            }
            VfsPath::Epoch(_) => Some(VfsPath::Epochs),
            VfsPath::EpochFirstCheckpoint(e)
            | VfsPath::EpochLastCheckpoint(e)
            | VfsPath::EpochCommittee(e)
            | VfsPath::EpochCheckpoints(e) => Some(VfsPath::Epoch(*e)),
            VfsPath::EpochCheckpointBySeq(e, _) | VfsPath::EpochCheckpointByDigest(e, _) => {
                Some(VfsPath::EpochCheckpoints(*e))
            }
            VfsPath::CheckpointsSeqRoot | VfsPath::CheckpointsDigestRoot => {
                Some(VfsPath::CheckpointsRoot)
            }
            VfsPath::CheckpointsBySeq(_) => Some(VfsPath::CheckpointsSeqRoot),
            VfsPath::CheckpointSeqSummary(s) | VfsPath::CheckpointSeqContents(s) => {
                Some(VfsPath::CheckpointsBySeq(*s))
            }
            VfsPath::CheckpointsByDigest(_) => Some(VfsPath::CheckpointsDigestRoot),
            VfsPath::CheckpointDigestSummary(d) | VfsPath::CheckpointDigestContents(d) => {
                Some(VfsPath::CheckpointsByDigest(*d))
            }
            VfsPath::CheckpointContentsEntry(_) => Some(VfsPath::CheckpointContentsRoot),
        }
    }
}

impl fmt::Display for VfsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VfsPath::Root => write!(f, "/"),
            VfsPath::Epochs => write!(f, "/epochs"),
            VfsPath::Epoch(e) => write!(f, "/epochs/{e}"),
            VfsPath::EpochFirstCheckpoint(e) => write!(f, "/epochs/{e}/first-checkpoint"),
            VfsPath::EpochLastCheckpoint(e) => write!(f, "/epochs/{e}/last-checkpoint"),
            VfsPath::EpochCommittee(e) => write!(f, "/epochs/{e}/committee"),
            VfsPath::EpochCheckpoints(e) => write!(f, "/epochs/{e}/checkpoints"),
            VfsPath::EpochCheckpointBySeq(e, s) => write!(f, "/epochs/{e}/checkpoints/{s}"),
            VfsPath::EpochCheckpointByDigest(e, d) => write!(f, "/epochs/{e}/checkpoints/{d}"),
            VfsPath::CheckpointsRoot => write!(f, "/checkpoints"),
            VfsPath::CheckpointsSeqRoot => write!(f, "/checkpoints/seq"),
            VfsPath::CheckpointsBySeq(s) => write!(f, "/checkpoints/seq/{s}"),
            VfsPath::CheckpointSeqSummary(s) => write!(f, "/checkpoints/seq/{s}/summary"),
            VfsPath::CheckpointSeqContents(s) => write!(f, "/checkpoints/seq/{s}/contents"),
            VfsPath::CheckpointsDigestRoot => write!(f, "/checkpoints/digest"),
            VfsPath::CheckpointsByDigest(d) => write!(f, "/checkpoints/digest/{d}"),
            VfsPath::CheckpointDigestSummary(d) => write!(f, "/checkpoints/digest/{d}/summary"),
            VfsPath::CheckpointDigestContents(d) => write!(f, "/checkpoints/digest/{d}/contents"),
            VfsPath::CheckpointContentsRoot => write!(f, "/checkpoint-contents"),
            VfsPath::CheckpointContentsEntry(d) => write!(f, "/checkpoint-contents/{d}"),
        }
    }
}

pub fn parse_path(s: &str) -> anyhow::Result<VfsPath> {
    let parts: Vec<&str> = s
        .trim_start_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();

    let v = match parts.as_slice() {
        [] => VfsPath::Root,
        ["epochs"] => VfsPath::Epochs,
        ["epochs", e] => VfsPath::Epoch(e.parse().map_err(|_| anyhow!("invalid epoch: '{e}'"))?),
        ["epochs", e, "first-checkpoint"] => {
            VfsPath::EpochFirstCheckpoint(e.parse().map_err(|_| anyhow!("invalid epoch: '{e}'"))?)
        }
        ["epochs", e, "last-checkpoint"] => {
            VfsPath::EpochLastCheckpoint(e.parse().map_err(|_| anyhow!("invalid epoch: '{e}'"))?)
        }
        ["epochs", e, "committee"] => {
            VfsPath::EpochCommittee(e.parse().map_err(|_| anyhow!("invalid epoch: '{e}'"))?)
        }
        ["epochs", e, "checkpoints"] => {
            VfsPath::EpochCheckpoints(e.parse().map_err(|_| anyhow!("invalid epoch: '{e}'"))?)
        }
        ["epochs", e, "checkpoints", ref_str] => {
            let epoch = e.parse().map_err(|_| anyhow!("invalid epoch: '{e}'"))?;
            if let Ok(seq) = ref_str.parse::<CheckpointSequenceNumber>() {
                VfsPath::EpochCheckpointBySeq(epoch, seq)
            } else {
                let digest: CheckpointDigest = ref_str
                    .parse()
                    .map_err(|_| anyhow!("invalid checkpoint ref: '{ref_str}'"))?;
                VfsPath::EpochCheckpointByDigest(epoch, digest)
            }
        }
        ["checkpoints"] => VfsPath::CheckpointsRoot,
        ["checkpoints", "seq"] => VfsPath::CheckpointsSeqRoot,
        ["checkpoints", "seq", s] => VfsPath::CheckpointsBySeq(
            s.parse()
                .map_err(|_| anyhow!("invalid sequence number: '{s}'"))?,
        ),
        ["checkpoints", "seq", s, "summary"] => VfsPath::CheckpointSeqSummary(
            s.parse()
                .map_err(|_| anyhow!("invalid sequence number: '{s}'"))?,
        ),
        ["checkpoints", "seq", s, "contents"] => VfsPath::CheckpointSeqContents(
            s.parse()
                .map_err(|_| anyhow!("invalid sequence number: '{s}'"))?,
        ),
        ["checkpoints", "digest"] => VfsPath::CheckpointsDigestRoot,
        ["checkpoints", "digest", d] => VfsPath::CheckpointsByDigest(
            d.parse()
                .map_err(|_| anyhow!("invalid checkpoint digest: '{d}'"))?,
        ),
        ["checkpoints", "digest", d, "summary"] => VfsPath::CheckpointDigestSummary(
            d.parse()
                .map_err(|_| anyhow!("invalid checkpoint digest: '{d}'"))?,
        ),
        ["checkpoints", "digest", d, "contents"] => VfsPath::CheckpointDigestContents(
            d.parse()
                .map_err(|_| anyhow!("invalid checkpoint digest: '{d}'"))?,
        ),
        ["checkpoint-contents"] => VfsPath::CheckpointContentsRoot,
        ["checkpoint-contents", d] => VfsPath::CheckpointContentsEntry(
            d.parse()
                .map_err(|_| anyhow!("invalid contents digest: '{d}'"))?,
        ),
        _ => return Err(anyhow!("unknown path: '{s}'")),
    };
    Ok(v)
}

/// Resolve a path string (absolute or relative) against a CWD.
pub fn resolve_path(cwd: &VfsPath, path: &str) -> anyhow::Result<VfsPath> {
    if path.starts_with('/') {
        return parse_path(path);
    }
    let cwd_str = cwd.to_string();
    let mut parts: Vec<&str> = cwd_str
        .trim_start_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    let absolute = format!("/{}", parts.join("/"));
    parse_path(&absolute)
}
