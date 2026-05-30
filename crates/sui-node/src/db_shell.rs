// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! REST API handlers for the db-shell command's proxy mode.
//!
//! Example curl usage:
//!   curl 'http://127.0.0.1:1337/db-shell/ls?path=/checkpoints/seq&limit=10'
//!   curl 'http://127.0.0.1:1337/db-shell/read?path=/checkpoints/seq/1234/summary&format=json'
//!   curl 'http://127.0.0.1:1337/db-shell/read?path=/checkpoints/seq/1234/summary&format=debug'
//!   curl 'http://127.0.0.1:1337/db-shell/read?path=/checkpoints/seq/1234/summary&format=bcs'
//!   curl -X DELETE 'http://127.0.0.1:1337/db-shell/delete?path=/checkpoints/seq/1234/summary'

use crate::admin::AppState;
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use base64::Engine;
use consensus_core::{
    BlockAPI as _, CommitAPI as _, CommitRange,
    storage::{Store as ConsensusStore, rocksdb_store::RocksDBStore},
};
use serde::{Deserialize, Serialize};
use serde_json::value::Value as JsonValue;
use std::sync::Arc;
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::{
    base_types::EpochId,
    digests::{
        CheckpointContentsDigest, CheckpointDigest, TransactionDigest, TransactionEffectsDigest,
    },
    messages_checkpoint::CheckpointSequenceNumber,
};

pub const DEFAULT_LIMIT: usize = 30;

#[derive(Debug, Deserialize)]
pub struct LsParams {
    pub path: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// When true, treat the final path component as a start cursor.
    #[serde(default)]
    pub cursor: bool,
}

#[derive(Debug, Deserialize)]
pub struct ReadParams {
    pub path: String,
    #[serde(default = "default_format")]
    pub format: ReadFormat,
}

#[derive(Debug, Deserialize)]
pub struct DeleteParams {
    pub path: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum ReadFormat {
    Json,
    Debug,
    Bcs,
    RawBcs,
}

fn default_limit() -> usize {
    DEFAULT_LIMIT
}

fn default_format() -> ReadFormat {
    ReadFormat::Json
}

#[derive(Debug, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
}

// ─── unified error type ───────────────────────────────────────────────────────

pub(crate) struct ApiError(StatusCode, String);

impl<E: std::fmt::Display> From<(StatusCode, E)> for ApiError {
    fn from((status, e): (StatusCode, E)) -> Self {
        ApiError(status, e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

fn bad_request(msg: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.to_string())
}

fn not_found(msg: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, msg.to_string())
}

fn internal(msg: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, msg.to_string())
}

fn not_implemented(msg: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::NOT_IMPLEMENTED, msg.to_string())
}

// ─── path parsing ─────────────────────────────────────────────────────────────

#[derive(Debug)]
enum VfsPath {
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
    CheckpointsBySeq(CheckpointSequenceNumber),
    CheckpointSeqSummary(CheckpointSequenceNumber),
    CheckpointSeqContents(CheckpointSequenceNumber),
    CheckpointSeqContentsShort(CheckpointSequenceNumber),
    CheckpointsDigestRoot,
    CheckpointsByDigest(CheckpointDigest),
    CheckpointDigestSummary(CheckpointDigest),
    CheckpointDigestContents(CheckpointDigest),
    CheckpointDigestContentsShort(CheckpointDigest),
    CheckpointContentsRoot,
    CheckpointContentsEntry(CheckpointContentsDigest),
    TransactionsRoot,
    TransactionEntry(TransactionDigest),
    TransactionEffectsEntry(TransactionDigest, TransactionEffectsDigest),
    ConsensusRoot,
    ConsensusLatest,
    ConsensusCommitsRoot,
    ConsensusCommitDir(u32),
    ConsensusCommitSummary(u32),
}

fn parse_path(s: &str) -> Result<VfsPath, ApiError> {
    let parts: Vec<&str> = s
        .trim_start_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();

    let r = match parts.as_slice() {
        [] => VfsPath::Root,
        ["epochs"] => VfsPath::Epochs,
        ["epochs", e] => VfsPath::Epoch(
            e.parse()
                .map_err(|_| bad_request(format!("invalid epoch: {e}")))?,
        ),
        ["epochs", e, "first-checkpoint"] => VfsPath::EpochFirstCheckpoint(
            e.parse()
                .map_err(|_| bad_request(format!("invalid epoch: {e}")))?,
        ),
        ["epochs", e, "last-checkpoint"] => VfsPath::EpochLastCheckpoint(
            e.parse()
                .map_err(|_| bad_request(format!("invalid epoch: {e}")))?,
        ),
        ["epochs", e, "committee"] => VfsPath::EpochCommittee(
            e.parse()
                .map_err(|_| bad_request(format!("invalid epoch: {e}")))?,
        ),
        ["epochs", e, "checkpoints"] => VfsPath::EpochCheckpoints(
            e.parse()
                .map_err(|_| bad_request(format!("invalid epoch: {e}")))?,
        ),
        ["epochs", e, "checkpoints", ref_str] => {
            let epoch = e
                .parse()
                .map_err(|_| bad_request(format!("invalid epoch: {e}")))?;
            if let Ok(seq) = ref_str.parse::<CheckpointSequenceNumber>() {
                VfsPath::EpochCheckpointBySeq(epoch, seq)
            } else {
                let digest: CheckpointDigest = ref_str
                    .parse()
                    .map_err(|_| bad_request(format!("invalid checkpoint ref: {ref_str}")))?;
                VfsPath::EpochCheckpointByDigest(epoch, digest)
            }
        }
        ["checkpoints"] => VfsPath::CheckpointsRoot,
        ["checkpoints", "seq"] => VfsPath::CheckpointsSeqRoot,
        ["checkpoints", "seq", s] => VfsPath::CheckpointsBySeq(
            s.parse()
                .map_err(|_| bad_request(format!("invalid sequence: {s}")))?,
        ),
        ["checkpoints", "seq", s, "summary"] => VfsPath::CheckpointSeqSummary(
            s.parse()
                .map_err(|_| bad_request(format!("invalid sequence: {s}")))?,
        ),
        ["checkpoints", "seq", s, "contents"] => VfsPath::CheckpointSeqContents(
            s.parse()
                .map_err(|_| bad_request(format!("invalid sequence: {s}")))?,
        ),
        ["checkpoints", "seq", s, "contents-short"] => VfsPath::CheckpointSeqContentsShort(
            s.parse()
                .map_err(|_| bad_request(format!("invalid sequence: {s}")))?,
        ),
        ["checkpoints", "digest"] => VfsPath::CheckpointsDigestRoot,
        ["checkpoints", "digest", d] => VfsPath::CheckpointsByDigest(
            d.parse()
                .map_err(|_| bad_request(format!("invalid digest: {d}")))?,
        ),
        ["checkpoints", "digest", d, "summary"] => VfsPath::CheckpointDigestSummary(
            d.parse()
                .map_err(|_| bad_request(format!("invalid digest: {d}")))?,
        ),
        ["checkpoints", "digest", d, "contents"] => VfsPath::CheckpointDigestContents(
            d.parse()
                .map_err(|_| bad_request(format!("invalid digest: {d}")))?,
        ),
        ["checkpoints", "digest", d, "contents-short"] => VfsPath::CheckpointDigestContentsShort(
            d.parse()
                .map_err(|_| bad_request(format!("invalid digest: {d}")))?,
        ),
        ["checkpoint-contents"] => VfsPath::CheckpointContentsRoot,
        ["checkpoint-contents", d] => VfsPath::CheckpointContentsEntry(
            d.parse()
                .map_err(|_| bad_request(format!("invalid contents digest: {d}")))?,
        ),
        ["transactions"] => VfsPath::TransactionsRoot,
        ["transactions", seg] => parse_transaction_seg(seg)?,
        ["consensus"] => VfsPath::ConsensusRoot,
        ["consensus", "latest"] => VfsPath::ConsensusLatest,
        ["consensus", "commits"] => VfsPath::ConsensusCommitsRoot,
        ["consensus", "commits", i] => VfsPath::ConsensusCommitDir(
            i.parse()
                .map_err(|_| bad_request(format!("invalid commit index: {i}")))?,
        ),
        ["consensus", "commits", i, "summary"] => VfsPath::ConsensusCommitSummary(
            i.parse()
                .map_err(|_| bad_request(format!("invalid commit index: {i}")))?,
        ),
        _ => return Err(bad_request(format!("unknown path: {s}"))),
    };
    Ok(r)
}

fn parse_transaction_seg(seg: &str) -> Result<VfsPath, ApiError> {
    if let Some((tx_str, fx_str)) = seg.split_once(".fx-") {
        let tx: TransactionDigest = tx_str
            .parse()
            .map_err(|_| bad_request(format!("invalid transaction digest: {tx_str}")))?;
        let fx: TransactionEffectsDigest = fx_str
            .parse()
            .map_err(|_| bad_request(format!("invalid effects digest: {fx_str}")))?;
        Ok(VfsPath::TransactionEffectsEntry(tx, fx))
    } else {
        let tx: TransactionDigest = seg
            .parse()
            .map_err(|_| bad_request(format!("invalid transaction digest: {seg}")))?;
        Ok(VfsPath::TransactionEntry(tx))
    }
}

// ─── ls handler ───────────────────────────────────────────────────────────────

pub(crate) async fn handle_ls(
    State(state): State<Arc<AppState>>,
    Query(params): Query<LsParams>,
) -> Result<Json<Vec<DirEntry>>, ApiError> {
    let path = parse_path(&params.path)?;
    let limit = params.limit.min(1000);
    let cp_store = state.node.clone_checkpoint_store();
    let committee_store = state.node.clone_committee_store();
    let auth_store = state.node.clone_authority_store();
    let consensus_store = state.node.clone_consensus_store();

    let entries = match (&path, params.cursor) {
        (VfsPath::CheckpointsBySeq(seq), true) => {
            list_checkpoints_from_seq(&cp_store, Some(*seq), limit)?
        }
        (VfsPath::CheckpointsByDigest(d), true) => {
            list_checkpoint_digests_from(&cp_store, Some(*d), limit)?
        }
        (VfsPath::EpochCheckpointBySeq(epoch, seq), true) => {
            list_epoch_checkpoints_from(&cp_store, *epoch, Some(*seq), limit)?
        }
        (VfsPath::CheckpointContentsEntry(d), true) => {
            list_checkpoint_contents_from(&cp_store, Some(*d), limit)?
        }
        (VfsPath::TransactionEntry(d), true) => {
            list_transactions_from(&auth_store, Some(*d), limit)?
        }
        (VfsPath::ConsensusCommitDir(idx), true) => {
            list_consensus_commits_from(consensus_store.as_deref(), Some(*idx), limit)?
        }
        _ => list_children(
            &path,
            &cp_store,
            &committee_store,
            &auth_store,
            consensus_store.as_deref(),
            limit,
        )?,
    };

    Ok(Json(entries))
}

fn list_children(
    path: &VfsPath,
    cp_store: &sui_core::checkpoints::CheckpointStore,
    committee_store: &sui_core::epoch::committee_store::CommitteeStore,
    auth_store: &sui_core::authority::AuthorityStore,
    consensus_store: Option<&RocksDBStore>,
    limit: usize,
) -> Result<Vec<DirEntry>, ApiError> {
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
            let epochs = committee_store
                .list_epochs(None, limit)
                .map_err(|e| internal(e))?;
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
            list_epoch_checkpoints_from(cp_store, *epoch, None, limit)
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
        VfsPath::CheckpointsSeqRoot => list_checkpoints_from_seq(cp_store, None, limit),
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
        VfsPath::CheckpointsDigestRoot => list_checkpoint_digests_from(cp_store, None, limit),
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
        VfsPath::CheckpointContentsRoot => list_checkpoint_contents_from(cp_store, None, limit),
        VfsPath::TransactionsRoot => list_transactions_from(auth_store, None, limit),
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
        VfsPath::ConsensusCommitsRoot => list_consensus_commits_from(consensus_store, None, limit),
        VfsPath::ConsensusCommitDir(_) => Ok(vec![DirEntry {
            name: "summary".into(),
            is_dir: false,
        }]),
        _ => Err(bad_request(format!("path is not a directory"))),
    }
}

fn list_checkpoints_from_seq(
    cp_store: &sui_core::checkpoints::CheckpointStore,
    start: Option<CheckpointSequenceNumber>,
    limit: usize,
) -> Result<Vec<DirEntry>, ApiError> {
    cp_store
        .list_checkpoints_from_seq(start, limit)
        .map(|items| {
            items
                .into_iter()
                .map(|(seq, _)| DirEntry {
                    name: seq.to_string(),
                    is_dir: true,
                })
                .collect()
        })
        .map_err(|e| internal(e))
}

fn list_checkpoint_digests_from(
    cp_store: &sui_core::checkpoints::CheckpointStore,
    start: Option<CheckpointDigest>,
    limit: usize,
) -> Result<Vec<DirEntry>, ApiError> {
    cp_store
        .list_checkpoint_digests(start, limit)
        .map(|items| {
            items
                .into_iter()
                .map(|d| DirEntry {
                    name: d.to_string(),
                    is_dir: true,
                })
                .collect()
        })
        .map_err(|e| internal(e))
}

fn list_checkpoint_contents_from(
    cp_store: &sui_core::checkpoints::CheckpointStore,
    start: Option<CheckpointContentsDigest>,
    limit: usize,
) -> Result<Vec<DirEntry>, ApiError> {
    cp_store
        .list_checkpoint_contents_digests(start, limit)
        .map(|items| {
            items
                .into_iter()
                .map(|d| DirEntry {
                    name: d.to_string(),
                    is_dir: false,
                })
                .collect()
        })
        .map_err(|e| internal(e))
}

fn list_epoch_checkpoints_from(
    cp_store: &sui_core::checkpoints::CheckpointStore,
    epoch: EpochId,
    start: Option<CheckpointSequenceNumber>,
    limit: usize,
) -> Result<Vec<DirEntry>, ApiError> {
    cp_store
        .list_epoch_checkpoints(epoch, start, limit)
        .map(|items| {
            items
                .into_iter()
                .map(|(seq, _)| DirEntry {
                    name: seq.to_string(),
                    is_dir: false,
                })
                .collect()
        })
        .map_err(|e| internal(e))
}

fn list_transactions_from(
    auth_store: &sui_core::authority::AuthorityStore,
    start: Option<TransactionDigest>,
    limit: usize,
) -> Result<Vec<DirEntry>, ApiError> {
    let tx_digests = auth_store
        .list_transactions_from(start, limit)
        .map_err(|e| internal(e))?;
    let mut entries = Vec::with_capacity(tx_digests.len() * 2);
    for digest in &tx_digests {
        entries.push(DirEntry {
            name: digest.to_string(),
            is_dir: false,
        });
        if let Ok(Some(fx_digest)) = auth_store.get_executed_effects_digest_for_tx(digest) {
            entries.push(DirEntry {
                name: format!("{digest}.fx-{fx_digest}"),
                is_dir: false,
            });
        }
    }
    Ok(entries)
}

fn list_consensus_commits_from(
    consensus_store: Option<&RocksDBStore>,
    start: Option<u32>,
    limit: usize,
) -> Result<Vec<DirEntry>, ApiError> {
    let cs = consensus_store.ok_or_else(|| {
        not_implemented("consensus store not available (node is not a validator)")
    })?;
    let start_idx = start.unwrap_or(0);
    let end_idx = start_idx.saturating_add(limit as u32);
    let commits = cs
        .scan_commits(CommitRange::new(start_idx..=end_idx))
        .map_err(|e| internal(e))?;
    Ok(commits
        .into_iter()
        .map(|c| DirEntry {
            name: c.index().to_string(),
            is_dir: true,
        })
        .collect())
}

fn render_consensus_commit_summary(
    consensus_store: Option<&RocksDBStore>,
    index: u32,
    format: ReadFormat,
) -> Result<Response, ApiError> {
    let cs = consensus_store.ok_or_else(|| {
        not_implemented("consensus store not available (node is not a validator)")
    })?;
    let commits = cs
        .scan_commits(CommitRange::new(index..=index))
        .map_err(|e| internal(e))?;
    let commit = commits
        .into_iter()
        .next()
        .ok_or_else(|| not_found(format!("consensus commit {index} not found")))?;

    let block_refs: Vec<_> = commit.blocks().to_vec();
    let blocks = cs.read_blocks(&block_refs).map_err(|e| internal(e))?;

    let mut tx_keys: Vec<String> = Vec::new();
    for block_opt in blocks {
        let block = match block_opt {
            Some(b) => b,
            None => continue,
        };
        for tx_bytes in block.transactions_data() {
            if let Ok(tx) = bcs::from_bytes::<ConsensusTransaction>(tx_bytes) {
                tx_keys.push(format!("{:?}", tx.key()));
            }
        }
    }

    match format {
        ReadFormat::Json => {
            let val = serde_json::json!({
                "index": commit.index(),
                "transactions": tx_keys,
            });
            Ok(Json(val).into_response())
        }
        ReadFormat::Debug | ReadFormat::Bcs | ReadFormat::RawBcs => {
            let mut text = format!("commit {}\n", commit.index());
            for key in &tx_keys {
                text.push_str(&format!("  {key}\n"));
            }
            Ok(text.into_response())
        }
    }
}

// ─── read handler ─────────────────────────────────────────────────────────────

pub(crate) async fn handle_read(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ReadParams>,
) -> Result<Response, ApiError> {
    let path = parse_path(&params.path)?;
    let cp_store = state.node.clone_checkpoint_store();
    let committee_store = state.node.clone_committee_store();
    let auth_store = state.node.clone_authority_store();
    let consensus_store = state.node.clone_consensus_store();
    resolve_read(
        &path,
        &cp_store,
        &committee_store,
        &auth_store,
        consensus_store.as_deref(),
        params.format,
    )
}

fn resolve_read(
    path: &VfsPath,
    cp_store: &sui_core::checkpoints::CheckpointStore,
    committee_store: &sui_core::epoch::committee_store::CommitteeStore,
    auth_store: &sui_core::authority::AuthorityStore,
    consensus_store: Option<&RocksDBStore>,
    format: ReadFormat,
) -> Result<Response, ApiError> {
    match path {
        VfsPath::EpochFirstCheckpoint(epoch) => {
            let first_seq = cp_store
                .get_epoch_first_checkpoint_seq(*epoch)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("no data for epoch {epoch}")))?;
            let cp = cp_store
                .get_checkpoint_by_sequence_number(first_seq)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("checkpoint {first_seq} not found")))?;
            render_summary(cp.data(), format)
        }
        VfsPath::EpochLastCheckpoint(epoch) => {
            let cp = cp_store
                .get_epoch_last_checkpoint(*epoch)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("no last checkpoint for epoch {epoch}")))?;
            render_summary(cp.data(), format)
        }
        VfsPath::EpochCommittee(epoch) => {
            let committee = committee_store
                .get_committee(epoch)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("no committee for epoch {epoch}")))?;
            render_value(committee.as_ref(), format)
        }
        VfsPath::EpochCheckpointBySeq(_epoch, seq) => {
            let cp = cp_store
                .get_checkpoint_by_sequence_number(*seq)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("checkpoint {seq} not found")))?;
            render_summary(cp.data(), format)
        }
        VfsPath::EpochCheckpointByDigest(_epoch, digest) => {
            let cp = cp_store
                .get_checkpoint_by_digest(digest)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("checkpoint {digest} not found")))?;
            render_summary(cp.data(), format)
        }
        VfsPath::CheckpointSeqSummary(seq) => {
            let cp = cp_store
                .get_checkpoint_by_sequence_number(*seq)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("checkpoint {seq} not found")))?;
            render_summary(cp.data(), format)
        }
        VfsPath::CheckpointSeqContents(seq) => {
            let cp = cp_store
                .get_checkpoint_by_sequence_number(*seq)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("checkpoint {seq} not found")))?;
            let contents = cp_store
                .get_checkpoint_contents(&cp.content_digest)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("contents for checkpoint {seq} not found")))?;
            render_value(&contents, format)
        }
        VfsPath::CheckpointSeqContentsShort(seq) => {
            let cp = cp_store
                .get_checkpoint_by_sequence_number(*seq)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("checkpoint {seq} not found")))?;
            let contents = cp_store
                .get_checkpoint_contents(&cp.content_digest)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("contents for checkpoint {seq} not found")))?;
            render_contents_short(&contents, format)
        }
        VfsPath::CheckpointDigestSummary(digest) => {
            let cp = cp_store
                .get_checkpoint_by_digest(digest)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("checkpoint {digest} not found")))?;
            render_summary(cp.data(), format)
        }
        VfsPath::CheckpointDigestContents(digest) => {
            let cp = cp_store
                .get_checkpoint_by_digest(digest)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("checkpoint {digest} not found")))?;
            let contents = cp_store
                .get_checkpoint_contents(&cp.content_digest)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("contents for checkpoint {digest} not found")))?;
            render_value(&contents, format)
        }
        VfsPath::CheckpointDigestContentsShort(digest) => {
            let cp = cp_store
                .get_checkpoint_by_digest(digest)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("checkpoint {digest} not found")))?;
            let contents = cp_store
                .get_checkpoint_contents(&cp.content_digest)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("contents for checkpoint {digest} not found")))?;
            render_contents_short(&contents, format)
        }
        VfsPath::CheckpointContentsEntry(digest) => {
            let contents = cp_store
                .get_checkpoint_contents(digest)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("checkpoint contents {digest} not found")))?;
            render_value(&contents, format)
        }
        VfsPath::TransactionEntry(digest) => {
            let tx = auth_store
                .get_transaction_block(digest)
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found(format!("transaction {digest} not found")))?;
            render_value(tx.data(), format)
        }
        VfsPath::TransactionEffectsEntry(tx_digest, fx_digest) => {
            let effects = auth_store
                .get_effects(fx_digest)
                .map_err(|e| internal(e))?
                .ok_or_else(|| {
                    not_found(format!("effects {fx_digest} for tx {tx_digest} not found"))
                })?;
            render_value(&effects, format)
        }
        VfsPath::ConsensusLatest => {
            let cs = consensus_store.ok_or_else(|| {
                not_implemented("consensus store not available (node is not a validator)")
            })?;
            let commit = cs
                .read_last_commit()
                .map_err(|e| internal(e))?
                .ok_or_else(|| not_found("no commits yet"))?;
            let index = commit.index();
            match format {
                ReadFormat::Json => Ok(Json(serde_json::json!({ "index": index })).into_response()),
                _ => Ok(index.to_string().into_response()),
            }
        }
        VfsPath::ConsensusCommitSummary(index) => {
            render_consensus_commit_summary(consensus_store, *index, format)
        }
        VfsPath::Epoch(epoch) => Err(bad_request(format!("epoch {epoch} is a directory"))),
        _ => Err(bad_request("path is not a readable file")),
    }
}

fn render_contents_short(
    contents: &sui_types::messages_checkpoint::CheckpointContents,
    format: ReadFormat,
) -> Result<Response, ApiError> {
    match format {
        ReadFormat::Json => {
            let pairs: Vec<JsonValue> = contents
                .iter()
                .map(|ed| {
                    serde_json::json!({
                        "transaction": ed.transaction.to_string(),
                        "effects": ed.effects.to_string(),
                    })
                })
                .collect();
            Ok(Json(JsonValue::Array(pairs)).into_response())
        }
        ReadFormat::Debug => {
            let mut text = String::new();
            for ed in contents.iter() {
                text.push_str(&format!("{} {}\n", ed.transaction, ed.effects));
            }
            Ok(text.into_response())
        }
        ReadFormat::Bcs | ReadFormat::RawBcs => Err(bad_request(
            "bcs not supported for contents-short; use 'contents' instead",
        )),
    }
}

fn render_summary<T>(value: &T, format: ReadFormat) -> Result<Response, ApiError>
where
    T: serde::Serialize + std::fmt::Debug,
{
    render_value(value, format)
}

fn render_value<T>(value: &T, format: ReadFormat) -> Result<Response, ApiError>
where
    T: serde::Serialize + std::fmt::Debug,
{
    match format {
        ReadFormat::Json => {
            let v: JsonValue = serde_json::to_value(value)
                .map_err(|e| internal(format!("serialize error: {e}")))?;
            Ok(Json(v).into_response())
        }
        ReadFormat::Debug => Ok(format!("{value:#?}").into_response()),
        ReadFormat::Bcs => {
            let bytes = bcs::to_bytes(value).map_err(|e| internal(format!("bcs error: {e}")))?;
            Ok(base64::engine::general_purpose::STANDARD
                .encode(&bytes)
                .into_response())
        }
        ReadFormat::RawBcs => {
            let bytes = bcs::to_bytes(value).map_err(|e| internal(format!("bcs error: {e}")))?;
            let mut headers = HeaderMap::new();
            headers.insert(
                axum::http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            Ok((headers, bytes).into_response())
        }
    }
}

// ─── delete handler ───────────────────────────────────────────────────────────

pub(crate) async fn handle_delete(
    State(_state): State<Arc<AppState>>,
    Query(params): Query<DeleteParams>,
) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        format!("delete not yet implemented for path: {}", params.path),
    )
}
