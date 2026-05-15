// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;

use mysten_common::ZipDebugEqIteratorExt;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::storage::LedgerTxSeqDigest;

use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;

pub(super) fn storage_error(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(tonic::Code::Internal, e.to_string())
}

pub(super) fn validate_checkpoint_bounds(
    start_checkpoint: Option<u64>,
    end_checkpoint: Option<u64>,
) -> Result<(), RpcError> {
    let start = start_checkpoint.unwrap_or(0);
    if let Some(end) = end_checkpoint
        && end < start
    {
        return Err(FieldViolation::new("end_checkpoint")
            .with_description("end_checkpoint must be greater than or equal to start_checkpoint")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    Ok(())
}

pub(super) fn checkpoint_hi_exclusive(service: &RpcService) -> Result<u64, RpcError> {
    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(|| RpcError::new(tonic::Code::Unavailable, "rpc indexes are disabled"))?;
    let checkpoint = indexes
        .get_highest_indexed_checkpoint_seq_number()
        .map_err(storage_error)?
        .ok_or_else(|| RpcError::new(tonic::Code::Unavailable, "rpc index is empty"))?;
    checkpoint
        .checked_add(1)
        .ok_or_else(|| RpcError::new(tonic::Code::Internal, "checkpoint bound overflow"))
}

pub(super) fn checkpoint_to_tx_boundary(
    service: &RpcService,
    checkpoint: CheckpointSequenceNumber,
) -> Result<u64, RpcError> {
    if checkpoint == 0 {
        return Ok(0);
    }
    service
        .reader
        .inner()
        .get_checkpoint_by_sequence_number(checkpoint - 1)
        .map(|checkpoint| checkpoint.data().network_total_transactions)
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                format!(
                    "missing checkpoint {} while resolving tx boundary",
                    checkpoint - 1
                ),
            )
        })
}

pub(super) fn checkpoint_to_tx_range(
    service: &RpcService,
    checkpoint_range: Range<u64>,
) -> Result<Range<u64>, RpcError> {
    let start = checkpoint_to_tx_boundary(service, checkpoint_range.start)?;
    let end = checkpoint_to_tx_boundary(service, checkpoint_range.end)?;
    Ok(start..end)
}

pub(super) fn get_tx_seq_digest_multi(
    service: &RpcService,
    tx_seqs: &[u64],
) -> Result<Vec<LedgerTxSeqDigest>, RpcError> {
    if tx_seqs.is_empty() {
        return Ok(Vec::new());
    }

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(|| RpcError::new(tonic::Code::Unavailable, "rpc indexes are disabled"))?;
    indexes
        .ledger_tx_seq_digest_multi_get(tx_seqs)
        .map_err(storage_error)?
        .into_iter()
        .zip_debug_eq(tx_seqs.iter().copied())
        .map(|(row, tx_seq)| row.ok_or_else(|| missing_tx_seq_digest(tx_seq)))
        .collect()
}

pub(super) fn get_tx_seq_digest_rows(
    service: &RpcService,
    range: Range<u64>,
    descending: bool,
    row_limit: usize,
) -> Result<Vec<LedgerTxSeqDigest>, RpcError> {
    if range.is_empty() || row_limit == 0 {
        return Ok(Vec::new());
    }

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(|| RpcError::new(tonic::Code::Unavailable, "rpc indexes are disabled"))?;
    let mut iter = indexes
        .ledger_tx_seq_digest_iter(range.start, range.end, descending)
        .map_err(storage_error)?;
    let mut rows = Vec::with_capacity(row_limit);
    let mut expected = if descending {
        range.end - 1
    } else {
        range.start
    };

    while rows.len() < row_limit {
        match iter.next() {
            Some(result) => {
                let row = result.map_err(storage_error)?;
                if row.tx_sequence_number != expected {
                    return Err(missing_tx_seq_digest(expected));
                }
                rows.push(row);

                if descending {
                    if expected == range.start {
                        break;
                    }
                    expected -= 1;
                } else {
                    expected += 1;
                    if expected >= range.end {
                        break;
                    }
                }
            }
            None => {
                return Err(missing_tx_seq_digest(expected));
            }
        }
    }

    Ok(rows)
}

/// Resolve a bitmap scan frontier (absolute member-id position) to the
/// checkpoint that bounds the covered range. The watermark proves coverage up to
/// `frontier - 1` ascending (the last position emitted) or `frontier` descending;
/// `decode_tx_seq` maps that position to the transaction whose checkpoint we
/// look up. Returns `None` only when the ascending frontier is at genesis, where
/// nothing is yet covered. Frontiers stay within the request's contiguous tx
/// range, so the exact `tx_seq` lookup always resolves.
pub(super) fn resolve_frontier_checkpoint(
    service: &RpcService,
    frontier: u64,
    ascending: bool,
    decode_tx_seq: impl FnOnce(u64) -> u64,
) -> Result<Option<u64>, RpcError> {
    let lookup_position = if ascending {
        match frontier.checked_sub(1) {
            Some(p) => p,
            None => return Ok(None),
        }
    } else {
        frontier
    };
    let tx_seq = decode_tx_seq(lookup_position);
    let checkpoint = get_tx_seq_digest_multi(service, &[tx_seq])?
        .into_iter()
        .next()
        .expect("multi-get of one tx_seq returns one row")
        .checkpoint_number;
    Ok(Some(checkpoint))
}

fn missing_tx_seq_digest(tx_seq: u64) -> RpcError {
    RpcError::new(
        tonic::Code::Internal,
        format!("missing tx_seq_digest row for tx_seq {tx_seq}"),
    )
}

pub(super) fn remaining_range_after(
    range: Range<u64>,
    position: u64,
    ascending: bool,
) -> Option<Range<u64>> {
    let remaining = if ascending {
        position.saturating_add(1)..range.end
    } else {
        range.start..position
    };
    (!remaining.is_empty()).then_some(remaining)
}
