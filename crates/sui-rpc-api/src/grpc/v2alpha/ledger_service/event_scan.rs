// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Bound;
use std::ops::Range;

use crate::ledger_history::query_options::EventPosition;
use sui_inverted_index::BitmapQuery;
use sui_inverted_index::ScanDirection;
use sui_inverted_index::event_seq;
use sui_types::storage::LedgerTxSeqDigest;
use tokio_util::sync::CancellationToken;

use crate::RpcError;
use crate::RpcService;
use crate::ledger_history::query_options::EventScanBounds;

use super::bitmap_scan::EVENT_BITMAP_BUCKET_SIZE;
use super::bitmap_scan::LedgerBitmapKind;
use super::bitmap_scan::PendingBitmapBucket;
use super::bitmap_scan::drain_bitmap_hits_with_budget;
use super::ledger_read::get_tx_seq_digest_rows;
use super::ledger_read::tx_checkpoint;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EventRef {
    pub(super) position: EventPosition,
    pub(super) tx_seq_digest: Option<LedgerTxSeqDigest>,
}

fn bound_to_tuple(bound: Bound<EventPosition>) -> Bound<(u64, u32)> {
    match bound {
        Bound::Included(position) => Bound::Included((position.tx_seq, position.event_index)),
        Bound::Excluded(position) => Bound::Excluded((position.tx_seq, position.event_index)),
        Bound::Unbounded => Bound::Unbounded,
    }
}

fn bounds_to_packed(bounds: &EventScanBounds) -> Range<u64> {
    event_seq::packed_range(bound_to_tuple(bounds.lo), bound_to_tuple(bounds.hi))
}

fn bounds_from_packed(range: Range<u64>) -> EventScanBounds {
    let (start_tx, start_event) = event_seq::decode_event_seq(range.start);
    let (end_tx, end_event) = event_seq::decode_event_seq(range.end);
    EventScanBounds {
        lo: Bound::Included(EventPosition {
            tx_seq: start_tx,
            event_index: start_event,
        }),
        hi: Bound::Excluded(EventPosition {
            tx_seq: end_tx,
            event_index: end_event,
        }),
    }
}

pub(super) struct DrainedEventHits {
    pub(super) items: Vec<EventPosition>,
    pub(super) pending_bucket: Option<PendingBitmapBucket>,
    pub(super) next_bounds: Option<EventScanBounds>,
    pub(super) buckets_scanned: usize,
    pub(super) frontier: Option<EventPosition>,
    pub(super) scan_limit_hit: bool,
}

pub(super) fn drain_event_bitmap_hits(
    service: RpcService,
    query: BitmapQuery,
    pending_bucket: Option<PendingBitmapBucket>,
    bounds: Option<EventScanBounds>,
    direction: ScanDirection,
    hit_limit: usize,
    scan_budget: usize,
    cancel: &CancellationToken,
) -> Result<DrainedEventHits, RpcError> {
    let packed_range = bounds.map(|bounds| bounds_to_packed(&bounds));
    let hits = drain_bitmap_hits_with_budget(
        service,
        LedgerBitmapKind::Event,
        EVENT_BITMAP_BUCKET_SIZE,
        query,
        pending_bucket,
        packed_range,
        direction,
        hit_limit,
        scan_budget,
        cancel,
    )?;

    Ok(DrainedEventHits {
        items: hits
            .items
            .into_iter()
            .map(|event_seq| {
                let (tx_seq, event_index) = event_seq::decode_event_seq(event_seq);
                EventPosition {
                    tx_seq,
                    event_index,
                }
            })
            .collect(),
        pending_bucket: hits.pending_bucket,
        next_bounds: hits.next_range.map(bounds_from_packed),
        buckets_scanned: hits.buckets_scanned,
        frontier: hits.coalesced_frontier.map(|event_seq| {
            let (tx_seq, event_index) = event_seq::decode_event_seq(event_seq);
            EventPosition {
                tx_seq,
                event_index,
            }
        }),
        scan_limit_hit: hits.scan_limit_hit,
    })
}

pub(super) struct UnfilteredScan {
    pub(super) refs: Vec<EventRef>,
    pub(super) next_bounds: Option<EventScanBounds>,
    pub(super) rows_scanned: usize,
    pub(super) scan_limit_hit: bool,
    pub(super) frontier: Option<EventPosition>,
}

pub(super) fn next_unfiltered_event_refs(
    service: &RpcService,
    bounds: &EventScanBounds,
    ascending: bool,
    event_ref_limit: usize,
    row_scan_limit: usize,
) -> Result<UnfilteredScan, RpcError> {
    let Some(tx_range) = tx_range_for_bounds(bounds) else {
        return Ok(UnfilteredScan {
            refs: Vec::new(),
            next_bounds: None,
            rows_scanned: 0,
            scan_limit_hit: false,
            frontier: None,
        });
    };

    let rows = get_tx_seq_digest_rows(service, tx_range, !ascending, row_scan_limit)?;
    let mut refs = Vec::with_capacity(event_ref_limit);
    let mut next_bounds = None;
    let mut rows_scanned = 0;

    for row in rows {
        rows_scanned += 1;
        let filled_next = push_event_refs_for_row_until_limit(
            &mut refs,
            row,
            *bounds,
            ascending,
            event_ref_limit,
        );
        if refs.len() == event_ref_limit {
            return Ok(UnfilteredScan {
                refs,
                next_bounds: filled_next,
                rows_scanned,
                scan_limit_hit: false,
                frontier: None,
            });
        }
        next_bounds = remaining_bounds_after_scanned_tx(*bounds, row.tx_sequence_number, ascending);
    }

    let scan_limit_hit = rows_scanned == row_scan_limit && next_bounds.is_some();
    let frontier = if scan_limit_hit {
        next_bounds
            .as_ref()
            .and_then(|bounds| frontier_from_resume_bounds(bounds, ascending))
    } else {
        None
    };

    Ok(UnfilteredScan {
        refs,
        next_bounds,
        rows_scanned,
        scan_limit_hit,
        frontier,
    })
}

pub(super) fn event_frontier_checkpoint(
    service: &RpcService,
    frontier: EventPosition,
    ascending: bool,
) -> Result<Option<u64>, RpcError> {
    let lookup_tx = if ascending {
        if frontier.event_index > 0 {
            frontier.tx_seq
        } else {
            match frontier.tx_seq.checked_sub(1) {
                Some(tx_seq) => tx_seq,
                None => return Ok(None),
            }
        }
    } else {
        frontier.tx_seq
    };
    tx_checkpoint(service, lookup_tx).map(Some)
}

fn tx_range_for_bounds(bounds: &EventScanBounds) -> Option<Range<u64>> {
    let start_tx = match bounds.lo {
        Bound::Included(position) | Bound::Excluded(position) => position.tx_seq,
        Bound::Unbounded => 0,
    };
    let end_tx = match bounds.hi {
        Bound::Excluded(position) if position.event_index == 0 => position.tx_seq,
        Bound::Included(position) | Bound::Excluded(position) => position.tx_seq.saturating_add(1),
        Bound::Unbounded => u64::MAX,
    };
    (start_tx < end_tx).then_some(start_tx..end_tx)
}

fn push_event_refs_for_row_until_limit(
    refs: &mut Vec<EventRef>,
    row: LedgerTxSeqDigest,
    bounds: EventScanBounds,
    ascending: bool,
    event_ref_limit: usize,
) -> Option<EventScanBounds> {
    if row.event_count == 0 {
        return None;
    }

    let mut next_bounds = None;
    if ascending {
        for event_index in 0..row.event_count {
            let position = EventPosition {
                tx_seq: row.tx_sequence_number,
                event_index,
            };
            if !bounds.contains(position) {
                continue;
            }
            refs.push(EventRef {
                position,
                tx_seq_digest: Some(row),
            });
            next_bounds = remaining_bounds_after_event(bounds, position, ascending);
            if refs.len() == event_ref_limit {
                return next_bounds;
            }
        }
    } else {
        for event_index in (0..row.event_count).rev() {
            let position = EventPosition {
                tx_seq: row.tx_sequence_number,
                event_index,
            };
            if !bounds.contains(position) {
                continue;
            }
            refs.push(EventRef {
                position,
                tx_seq_digest: Some(row),
            });
            next_bounds = remaining_bounds_after_event(bounds, position, ascending);
            if refs.len() == event_ref_limit {
                return next_bounds;
            }
        }
    }

    next_bounds
}

fn remaining_bounds_after_event(
    mut bounds: EventScanBounds,
    position: EventPosition,
    ascending: bool,
) -> Option<EventScanBounds> {
    if ascending {
        bounds.lo = Bound::Excluded(position);
    } else {
        bounds.hi = Bound::Excluded(position);
    }
    (!bounds.is_empty()).then_some(bounds)
}

fn remaining_bounds_after_scanned_tx(
    mut bounds: EventScanBounds,
    tx_seq: u64,
    ascending: bool,
) -> Option<EventScanBounds> {
    if ascending {
        bounds.lo = Bound::Included(EventPosition::start_of_tx(tx_seq.saturating_add(1)));
    } else {
        bounds.hi = Bound::Excluded(EventPosition::start_of_tx(tx_seq));
    }
    (!bounds.is_empty()).then_some(bounds)
}

fn frontier_from_resume_bounds(bounds: &EventScanBounds, ascending: bool) -> Option<EventPosition> {
    if ascending {
        match bounds.lo {
            Bound::Included(position) | Bound::Excluded(position) => Some(position),
            Bound::Unbounded => None,
        }
    } else {
        match bounds.hi {
            Bound::Excluded(position) | Bound::Included(position) => Some(position),
            Bound::Unbounded => None,
        }
    }
}
