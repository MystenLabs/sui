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

/// Reject list queries when ledger-history indexing is disabled. The v2alpha
/// list APIs read the history indexes (`tx_seq_digest`, transaction/event
/// bitmaps); when an operator has not enabled `ledger_history_indexing` those
/// column families are absent, so the API is unsupported rather than merely
/// empty. Gates both indexing and serving the same way `authenticated_events`
/// does.
pub(super) fn ensure_ledger_history_enabled(service: &RpcService) -> Result<(), RpcError> {
    if !service.config.ledger_history_indexing() {
        return Err(RpcError::new(
            tonic::Code::Unimplemented,
            "ledger history indexing is disabled",
        ));
    }
    Ok(())
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

/// The serving floor in tx-seq space: the first tx_sequence_number whose
/// checkpoint contents are still retained (the first tx of the lowest available
/// checkpoint). This is the perpetual-store pruning floor, NOT the index's own
/// floor — the index prunes *after* the perpetual store, so it can still hold
/// `tx_seq_digest` rows for transactions whose contents are already gone. Serving
/// must be bounded by content availability, matching `get_transaction`.
///
/// `checkpoint_to_tx_boundary` reads the lowest available checkpoint's predecessor
/// from `certified_checkpoints`, which is retained across pruning, so this resolves
/// even at the floor.
pub(super) fn lowest_available_tx_seq(service: &RpcService) -> Result<u64, RpcError> {
    let lowest_checkpoint = service.reader.get_lowest_available_checkpoint()?;
    checkpoint_to_tx_boundary(service, lowest_checkpoint)
}

/// Enforce the pruning `floor` on a resolved scan's low end (`start`, tx-seq space).
/// Returns the effective start: unchanged when at/above the floor; clamped up to
/// the floor when the low end was open-ended; or `OutOfRange` when an explicitly
/// requested low end (a `start_checkpoint` or `after` cursor) is below the floor —
/// that data was pruned and is permanently gone.
pub(super) fn apply_tx_seq_floor(
    start: u64,
    explicit_lower: bool,
    floor: u64,
) -> Result<u64, RpcError> {
    if start >= floor {
        Ok(start)
    } else if explicit_lower {
        Err(out_of_range(floor))
    } else {
        Ok(floor)
    }
}

fn out_of_range(floor: u64) -> RpcError {
    RpcError::new(
        tonic::Code::OutOfRange,
        format!("requested data below earliest available; lowest available tx_seq is {floor}"),
    )
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
        .map(|(row, tx_seq)| row.ok_or_else(|| missing_row_error(service, tx_seq)))
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
                    return Err(missing_row_error(service, expected));
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
                return Err(missing_row_error(service, expected));
            }
        }
    }

    Ok(rows)
}

/// Resolve a transaction sequence number to its containing checkpoint.
pub(super) fn tx_checkpoint(service: &RpcService, tx_seq: u64) -> Result<u64, RpcError> {
    let checkpoint = get_tx_seq_digest_multi(service, &[tx_seq])?
        .into_iter()
        .next()
        .expect("multi-get of one tx_seq returns one row")
        .checkpoint_number;
    Ok(checkpoint)
}

/// Resolve a u64 scan frontier to the checkpoint that bounds the covered range.
/// The watermark proves coverage up to `frontier - 1` ascending or `frontier`
/// descending. Returns `None` only when the ascending frontier is at genesis,
/// where nothing is yet covered.
pub(super) fn sequence_frontier_checkpoint(
    service: &RpcService,
    frontier: u64,
    ascending: bool,
) -> Result<Option<u64>, RpcError> {
    let lookup_position = if ascending {
        match frontier.checked_sub(1) {
            Some(position) => position,
            None => return Ok(None),
        }
    } else {
        frontier
    };
    tx_checkpoint(service, lookup_position).map(Some)
}

/// Classify a missing `tx_seq_digest` row encountered during a scan. The serving
/// floor can advance after a range is resolved (a concurrent prune), so a row
/// missing below the *current* floor was pruned mid-scan — permanently gone,
/// surfaced as `OutOfRange`. A row missing at/above the floor is a genuine
/// inconsistency (`Internal`). A failed floor lookup conservatively reports the
/// latter.
fn missing_row_error(service: &RpcService, tx_seq: u64) -> RpcError {
    match lowest_available_tx_seq(service) {
        Ok(floor) if tx_seq < floor => out_of_range(floor),
        _ => missing_tx_seq_digest(tx_seq),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_tx_seq_floor_passes_through_at_or_above_floor() {
        assert_eq!(apply_tx_seq_floor(10, false, 10).unwrap(), 10);
        assert_eq!(apply_tx_seq_floor(15, true, 10).unwrap(), 15);
    }

    #[test]
    fn apply_tx_seq_floor_clamps_open_ended_low_end() {
        // No explicit low bound below the floor → clamp up to the floor.
        assert_eq!(apply_tx_seq_floor(0, false, 10).unwrap(), 10);
        assert_eq!(apply_tx_seq_floor(7, false, 10).unwrap(), 10);
    }

    #[test]
    fn apply_tx_seq_floor_errors_on_explicit_below_floor() {
        for start in [0u64, 9] {
            let status = tonic::Status::from(apply_tx_seq_floor(start, true, 10).unwrap_err());
            assert_eq!(status.code(), tonic::Code::OutOfRange);
        }
    }

    #[test]
    fn apply_tx_seq_floor_zero_floor_never_clamps_or_errors() {
        // Empty/unpruned index (floor 0): every start is at/above the floor.
        assert_eq!(apply_tx_seq_floor(0, true, 0).unwrap(), 0);
        assert_eq!(apply_tx_seq_floor(0, false, 0).unwrap(), 0);
        assert_eq!(apply_tx_seq_floor(123, true, 0).unwrap(), 123);
    }
}
