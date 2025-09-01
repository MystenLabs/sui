// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;

use async_graphql::InputObject;

use crate::{
    api::{scalars::uint53::UInt53, types::event::CEvent},
    pagination::Page,
};

#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct EventFilter {
    /// Limit to events that occured strictly after the given checkpoint.
    pub after_checkpoint: Option<UInt53>,

    /// Limit to events in the given checkpoint.
    pub at_checkpoint: Option<UInt53>,

    /// Limit to event that occured strictly before the given checkpoint.
    pub before_checkpoint: Option<UInt53>,
    // TODO: (henry) Implement these filters.
    // pub sender: Option<SuiAddress>,
    // pub transaction_digest: Option<Digest>,
    // pub module: Option<ModuleFilter>,
    // pub type: Option<TypeFilter>,
}

/// The event indices (sequence_number) in a transaction's events array that are within the cursor bounds, inclusively.
/// Event transaction numbers are always returned in ascending order.
pub(super) fn tx_ev_bounds(
    page: &Page<CEvent>,
    tx_sequence_number: u64,
    event_count: usize,
) -> Range<usize> {
    // Find start index from 'after' cursor, defaults to 0
    let ev_lo = page
        .after()
        .filter(|c| c.tx_sequence_number == tx_sequence_number)
        .map(|c| c.ev_sequence_number as usize)
        .unwrap_or(0)
        .min(event_count);

    // Find exclusive end index from 'before' cursor, default to event_count
    let ev_hi = page
        .before()
        .filter(|c| c.tx_sequence_number == tx_sequence_number)
        .map(|c| (c.ev_sequence_number as usize).saturating_add(1))
        .unwrap_or(event_count)
        .max(ev_lo)
        .min(event_count);

    ev_lo..ev_hi
}

/// The transaction sequence number bounds with pagination cursors applied inclusively.
pub(super) fn pg_tx_bounds(
    page: &Page<CEvent>,
    tx_bounds: std::ops::Range<u64>,
) -> std::ops::Range<u64> {
    let pg_lo = page
        .after()
        .map(|c| c.tx_sequence_number)
        .map_or(tx_bounds.start, |tx_lo| tx_lo.max(tx_bounds.start));

    let pg_hi = page
        .before()
        .map(|c| c.tx_sequence_number.saturating_add(1))
        .map_or(tx_bounds.end, |tx_hi| tx_hi.min(tx_bounds.end));

    pg_lo..pg_hi
}
