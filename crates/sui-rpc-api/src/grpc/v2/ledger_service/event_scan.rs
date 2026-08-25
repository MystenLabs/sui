// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Bound;
use std::ops::Range;

use crate::ledger_history::query_options::IntraTxCoordinate;
use sui_inverted_index::BitmapQuery;
use sui_inverted_index::ScanDirection;
use sui_inverted_index::event_seq;
use sui_types::storage::LedgerTxSeqDigest;
use tokio_util::sync::CancellationToken;

use crate::RpcError;
use crate::RpcService;
use crate::ledger_history::query_options::IntraTxScanBounds;

use super::bitmap_scan::EVENT_BITMAP_BUCKET_SIZE;
use super::bitmap_scan::LedgerBitmapKind;
use super::bitmap_scan::PendingBitmapBucket;
use super::bitmap_scan::drain_bitmap_hits_with_budget;
use super::ledger_read::get_tx_seq_digest_rows;
use super::ledger_read::tx_checkpoint;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EventRef {
    pub(super) position: IntraTxCoordinate,
    pub(super) tx_seq_digest: Option<LedgerTxSeqDigest>,
}

pub(super) struct DrainedEventHits {
    pub(super) items: Vec<IntraTxCoordinate>,
    pub(super) pending_bucket: Option<PendingBitmapBucket>,
    pub(super) next_bounds: Option<IntraTxScanBounds>,
    pub(super) buckets_scanned: usize,
    pub(super) frontier: Option<IntraTxCoordinate>,
    pub(super) chunk_scan_limit_reached: bool,
}

pub(super) struct UnfilteredScan {
    pub(super) refs: Vec<EventRef>,
    pub(super) next_bounds: Option<IntraTxScanBounds>,
    pub(super) rows_scanned: usize,
    pub(super) row_limit_reached: bool,
    pub(super) frontier: Option<IntraTxCoordinate>,
}

fn bounds_from_packed(range: Range<u64>) -> IntraTxScanBounds {
    IntraTxScanBounds {
        lo: Bound::Included(event_seq::decode_event_seq(range.start).into()),
        hi: Bound::Excluded(event_seq::decode_event_seq(range.end).into()),
    }
}

pub(super) fn drain_event_bitmap_hits(
    service: RpcService,
    query: BitmapQuery,
    pending_bucket: Option<PendingBitmapBucket>,
    bounds: Option<IntraTxScanBounds>,
    direction: ScanDirection,
    hit_limit: usize,
    scan_budget: usize,
    cancel: &CancellationToken,
) -> Result<DrainedEventHits, RpcError> {
    let packed_range = bounds.map(|bounds| event_seq::packed_range(bounds.lo, bounds.hi));
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
            .map(|seq| IntraTxCoordinate::from(event_seq::decode_event_seq(seq)))
            .collect(),
        pending_bucket: hits.pending_bucket,
        next_bounds: hits.next_range.map(bounds_from_packed),
        buckets_scanned: hits.buckets_scanned,
        frontier: hits
            .coalesced_frontier
            .map(|seq| IntraTxCoordinate::from(event_seq::decode_event_seq(seq))),
        chunk_scan_limit_reached: hits.chunk_scan_limit_reached,
    })
}

pub(super) fn next_unfiltered_event_refs(
    service: &RpcService,
    bounds: &IntraTxScanBounds,
    ascending: bool,
    event_ref_limit: usize,
    row_scan_limit: usize,
) -> Result<UnfilteredScan, RpcError> {
    let Some(tx_range) = bounds.tx_range() else {
        return Ok(UnfilteredScan {
            refs: Vec::new(),
            next_bounds: None,
            rows_scanned: 0,
            row_limit_reached: false,
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
                row_limit_reached: false,
                frontier: None,
            });
        }
        next_bounds = remaining_bounds_after_scanned_tx(*bounds, row.tx_sequence_number, ascending);
    }

    let row_limit_reached = rows_scanned == row_scan_limit && next_bounds.is_some();
    let frontier = if row_limit_reached {
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
        row_limit_reached,
        frontier,
    })
}

pub(super) fn event_frontier_checkpoint(
    service: &RpcService,
    frontier: IntraTxCoordinate,
    ascending: bool,
) -> Result<Option<u64>, RpcError> {
    let lookup_tx = if ascending {
        if frontier.index > 0 {
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

fn push_event_refs_for_row_until_limit(
    refs: &mut Vec<EventRef>,
    row: LedgerTxSeqDigest,
    bounds: IntraTxScanBounds,
    ascending: bool,
    event_ref_limit: usize,
) -> Option<IntraTxScanBounds> {
    if row.event_count == 0 {
        return None;
    }

    let mut next_bounds = None;
    if ascending {
        for index in 0..row.event_count {
            let position = IntraTxCoordinate {
                tx_seq: row.tx_sequence_number,
                index,
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
        for index in (0..row.event_count).rev() {
            let position = IntraTxCoordinate {
                tx_seq: row.tx_sequence_number,
                index,
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
    mut bounds: IntraTxScanBounds,
    position: IntraTxCoordinate,
    ascending: bool,
) -> Option<IntraTxScanBounds> {
    if ascending {
        bounds.lo = Bound::Excluded(position);
    } else {
        bounds.hi = Bound::Excluded(position);
    }
    (!bounds.is_empty()).then_some(bounds)
}

fn remaining_bounds_after_scanned_tx(
    mut bounds: IntraTxScanBounds,
    tx_seq: u64,
    ascending: bool,
) -> Option<IntraTxScanBounds> {
    if ascending {
        bounds.lo = Bound::Included(IntraTxCoordinate::start_of_tx(tx_seq.saturating_add(1)));
    } else {
        bounds.hi = Bound::Excluded(IntraTxCoordinate::start_of_tx(tx_seq));
    }
    (!bounds.is_empty()).then_some(bounds)
}

fn frontier_from_resume_bounds(
    bounds: &IntraTxScanBounds,
    ascending: bool,
) -> Option<IntraTxCoordinate> {
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
