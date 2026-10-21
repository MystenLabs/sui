// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;

use mysten_common::ZipDebugEqIteratorExt;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::storage::LedgerTxSeqDigest;

use crate::RpcError;
use crate::RpcService;

pub(super) fn storage_error(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(tonic::Code::Internal, e.to_string())
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
        .ok_or_else(|| missing_checkpoint_error(service, checkpoint))
}

/// Classify a missing predecessor summary while resolving the tx boundary of
/// `checkpoint`. Range resolution runs before the serving-floor clamp, so a
/// boundary below the floor lands here first: report it as `OutOfRange`, as
/// `list_checkpoints` does. A boundary at or above the floor needs a summary
/// the store retains, so its absence is `Internal`.
fn missing_checkpoint_error(
    service: &RpcService,
    checkpoint: CheckpointSequenceNumber,
) -> RpcError {
    match service.reader.get_lowest_available_checkpoint() {
        Ok(floor) if checkpoint < floor => below_checkpoint_floor(floor),
        _ => RpcError::new(
            tonic::Code::Internal,
            format!(
                "missing checkpoint {} while resolving tx boundary",
                checkpoint - 1
            ),
        ),
    }
}

/// Resolve a half-open checkpoint range to the tx_sequence_number range it
/// spans. Checkpoint 0 resolves to tx_sequence_number 0. An empty checkpoint
/// range can only be of the form [x, x) and maps to an empty transaction
/// range [t, t).
pub(super) fn checkpoint_to_tx_range(
    service: &RpcService,
    checkpoint_range: Range<u64>,
) -> Result<Range<u64>, RpcError> {
    let start = checkpoint_to_tx_boundary(service, checkpoint_range.start)?;
    let end = if checkpoint_range.is_empty() {
        start
    } else {
        checkpoint_to_tx_boundary(service, checkpoint_range.end)?
    };
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
fn lowest_available_tx_seq(service: &RpcService) -> Result<u64, RpcError> {
    let lowest_checkpoint = service.reader.get_lowest_available_checkpoint()?;
    checkpoint_to_tx_boundary(service, lowest_checkpoint)
}

/// Enforce the pruning `floor` on a resolved scan's low end (`start`, tx-seq space).
/// Returns the effective start: unchanged when at/above the floor; clamped up to
/// the floor when the low end was open-ended; or `OutOfRange` when an explicitly
/// requested low end (a `start_checkpoint` or `after` cursor) is below the floor —
/// that data was pruned and is permanently gone.
fn apply_tx_seq_floor(start: u64, explicit_lower: bool, floor: u64) -> Result<u64, RpcError> {
    if start >= floor {
        Ok(start)
    } else if explicit_lower {
        Err(out_of_range(floor))
    } else {
        Ok(floor)
    }
}

/// The serving floor resolved onto a scan: the effective first scannable
/// transaction and its containing checkpoint, after clamping a resolved
/// scan's low end to the pruning floor.
pub(super) struct ServingFloor {
    pub(super) tx_seq: u64,
    pub(super) checkpoint: u64,
}

/// Whether the request sets its own lower bound: a `start_checkpoint` or an `after` cursor. Data
/// below the serving floor is pruned and permanently gone, so such a bound below the floor is
/// `OutOfRange`, while an open-ended low end is raised to the floor.
fn has_explicit_lower_bound(
    start_checkpoint: Option<u64>,
    options: &crate::ledger_history::query_options::QueryOptions,
) -> bool {
    start_checkpoint.is_some() || options.has_after_cursor()
}

/// Rejects a request with no lower bound of its own whose checkpoint `window` lies wholly below
/// the serving floor: every checkpoint it names is pruned and permanently gone, as for an explicit
/// lower bound below the floor, so the request is `OutOfRange` rather than an empty page. A window
/// reaching the floor is served from the floor up, even when the request's cursors leave nothing
/// of it to scan, and an empty window names no checkpoint to reject. `floor` is read only when
/// the window could lie below it.
fn reject_window_below_floor(
    window: &Range<u64>,
    explicit_lower_bound: bool,
    floor: impl FnOnce() -> Result<u64, RpcError>,
) -> Result<(), RpcError> {
    if window.is_empty() || explicit_lower_bound {
        return Ok(());
    }
    let floor = floor()?;
    if window.end <= floor {
        return Err(below_checkpoint_floor(floor));
    }
    Ok(())
}

/// [`reject_window_below_floor`] against this server's serving floor, for the request's resolved
/// checkpoint `window` (half-open, already clamped to the indexed tip).
pub(super) fn reject_window_below_serving_floor(
    service: &RpcService,
    window: &Range<u64>,
    start_checkpoint: Option<u64>,
    options: &crate::ledger_history::query_options::QueryOptions,
) -> Result<(), RpcError> {
    reject_window_below_floor(
        window,
        has_explicit_lower_bound(start_checkpoint, options),
        || service.reader.get_lowest_available_checkpoint(),
    )
}

/// Clamp a resolved scan's low end (`start_tx`, tx-seq space) to the serving
/// floor. Returns the resolved floor only when it actually moved the low end
/// (an implicit-genesis low); an explicitly requested low below the floor
/// (`start_checkpoint` or `after` cursor) errors `OutOfRange` instead, and an
/// untouched low returns `None`. Callers MUST reconcile their interval and
/// watermark metadata from the returned floor, so no watermark ever claims
/// pruned, never-scanned checkpoints: for a nonempty retained intersection,
/// ascending scans raise their entry claim to `checkpoint` and descending
/// scans pin their terminal to it; a floor that consumes the interval leaves
/// the request-derived terminal boundary in place and marks the interval
/// empty.
pub(super) fn clamp_to_serving_floor(
    service: &RpcService,
    start_tx: u64,
    start_checkpoint: Option<u64>,
    options: &crate::ledger_history::query_options::QueryOptions,
) -> Result<Option<ServingFloor>, RpcError> {
    let explicit_lower = has_explicit_lower_bound(start_checkpoint, options);
    let floor = lowest_available_tx_seq(service)?;
    let clamped = apply_tx_seq_floor(start_tx, explicit_lower, floor)?;
    if clamped == start_tx {
        return Ok(None);
    }
    Ok(Some(ServingFloor {
        tx_seq: clamped,
        checkpoint: tx_checkpoint(service, clamped)?,
    }))
}

fn out_of_range(floor: u64) -> RpcError {
    RpcError::new(
        tonic::Code::OutOfRange,
        format!("requested data below earliest available; lowest available tx_seq is {floor}"),
    )
}

fn below_checkpoint_floor(floor: u64) -> RpcError {
    RpcError::new(
        tonic::Code::OutOfRange,
        format!("requested data below earliest available; lowest available checkpoint is {floor}"),
    )
}

/// Checkpoint-space analogue of [`clamp_to_serving_floor`] for scans that
/// walk checkpoint sequence numbers directly (the unfiltered
/// `list_checkpoints` path). Returns the floor checkpoint only when it
/// actually moves the resolved low end (an implicit-genesis low); an
/// explicitly requested low below the floor (`start_checkpoint` or `after`
/// cursor) errors `OutOfRange` — that data was pruned and is permanently
/// gone — and an untouched low returns `None`.
pub(super) fn clamp_checkpoints_to_serving_floor(
    service: &RpcService,
    start: u64,
    start_checkpoint: Option<u64>,
    options: &crate::ledger_history::query_options::QueryOptions,
) -> Result<Option<u64>, RpcError> {
    let explicit_lower = has_explicit_lower_bound(start_checkpoint, options);
    let floor = service.reader.get_lowest_available_checkpoint()?;
    if start >= floor {
        return Ok(None);
    }
    if explicit_lower {
        return Err(below_checkpoint_floor(floor));
    }
    Ok(Some(floor))
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

    fn window_below_floor(
        window: Range<u64>,
        explicit_lower_bound: bool,
        floor: u64,
    ) -> Result<(), RpcError> {
        reject_window_below_floor(&window, explicit_lower_bound, || Ok(floor))
    }

    #[test]
    fn window_wholly_below_floor_without_lower_bound_is_out_of_range() {
        // The window's exclusive end at or below the floor: every checkpoint it names is pruned.
        for window in [0..10, 0..5, 3..10] {
            let status =
                tonic::Status::from(window_below_floor(window.clone(), false, 10).unwrap_err());
            assert_eq!(status.code(), tonic::Code::OutOfRange, "{window:?}");
            assert_eq!(
                status.message(),
                "requested data below earliest available; lowest available checkpoint is 10"
            );
        }
    }

    #[test]
    fn window_reaching_floor_is_served() {
        // The window's last checkpoint is the floor itself, or lies above it.
        for window in [0..11, 9..11, 10..20, 15..20] {
            assert!(
                window_below_floor(window.clone(), false, 10).is_ok(),
                "{window:?}"
            );
        }
    }

    #[test]
    fn window_below_floor_rule_skips_empty_windows_and_explicit_lower_bounds() {
        // An empty window names no checkpoint; an explicit lower bound below the floor is
        // rejected by the serving-floor clamp instead.
        for window in [0..0, 5..5, 10..10] {
            assert!(
                window_below_floor(window.clone(), false, 10).is_ok(),
                "{window:?}"
            );
        }
        window_below_floor(0..10, true, 10).unwrap();

        // Neither reads the floor.
        let unread_floor = || -> Result<u64, RpcError> { panic!("floor read") };
        reject_window_below_floor(&(5..5), false, unread_floor).unwrap();
        reject_window_below_floor(&(0..10), true, unread_floor).unwrap();
    }
}
