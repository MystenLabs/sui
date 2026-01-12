// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;

use crate::api::scalars::cursor::JsonCursor;
use crate::pagination::Page;

/// Trait for cursors used in checkpoint-based scanning operations.
/// Provides access to checkpoint and transaction sequence numbers for bounds computation.
pub(crate) trait ScanCursor {
    /// The checkpoint sequence number for this cursor position.
    fn cp_sequence_number(&self) -> u64;

    /// The transaction index within the checkpoint for this cursor position.
    fn tx_sequence_number(&self) -> u64;
}

/// Extension trait for scan cursors that also track event position within a transaction.
pub(crate) trait ScanCursorWithEvent: ScanCursor {
    /// The event index within the transaction for this cursor position.
    fn ev_sequence_number(&self) -> u64;
}

/// The transaction index bounds `[tx_lo, tx_hi)` within a checkpoint based on cursor positions.
pub(crate) fn cp_tx_bounds<C: ScanCursor>(
    page: &Page<JsonCursor<C>>,
    cp_sequence_number: u64,
    tx_count: usize,
) -> Range<usize> {
    let tx_lo = page
        .after()
        .filter(|c| c.cp_sequence_number() == cp_sequence_number)
        .map(|c| c.tx_sequence_number() as usize)
        .unwrap_or(0)
        .min(tx_count);

    let tx_hi = page
        .before()
        .filter(|c| c.cp_sequence_number() == cp_sequence_number)
        .map(|c| (c.tx_sequence_number() as usize).saturating_add(1))
        .unwrap_or(tx_count)
        .max(tx_lo)
        .min(tx_count);

    tx_lo..tx_hi
}

/// The event index bounds `[ev_lo, ev_hi)` within a transaction for scan operations.
pub(crate) fn cp_ev_bounds<C: ScanCursorWithEvent>(
    page: &Page<JsonCursor<C>>,
    cp_sequence_number: u64,
    tx_idx: usize,
    ev_count: usize,
) -> Range<usize> {
    let ev_lo = page
        .after()
        .filter(|c| {
            c.cp_sequence_number() == cp_sequence_number && c.tx_sequence_number() == tx_idx as u64
        })
        .map(|c| c.ev_sequence_number() as usize)
        .unwrap_or(0)
        .min(ev_count);

    let ev_hi = page
        .before()
        .filter(|c| {
            c.cp_sequence_number() == cp_sequence_number && c.tx_sequence_number() == tx_idx as u64
        })
        .map(|c| (c.ev_sequence_number() as usize).saturating_add(1))
        .unwrap_or(ev_count)
        .max(ev_lo)
        .min(ev_count);

    ev_lo..ev_hi
}
