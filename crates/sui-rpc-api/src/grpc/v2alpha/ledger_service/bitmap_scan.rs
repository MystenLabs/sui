// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use roaring::RoaringBitmap;
use sui_inverted_index::BitmapBucketIteratorSource;
use sui_inverted_index::BitmapQuery;
use sui_inverted_index::IndexDimension;
use sui_inverted_index::ScanDirection;
use sui_inverted_index::Watermarked;
use sui_inverted_index::dense_universe_buckets;
use sui_inverted_index::eval_bitmap_query_bucket_iter;
use sui_types::storage::LedgerBitmapBucketIterator;
use tokio_util::sync::CancellationToken;

use crate::RpcError;
use crate::RpcService;

use super::chunked_scan::cancelled;
use super::ledger_read::remaining_range_after;

pub(super) const TX_BITMAP_BUCKET_SIZE: u64 = 65_536;
// Must match the writer's `EVENT_BUCKET_SIZE` in sui-core's rpc_index and
// sui-kvstore's `event_bitmap_index::BUCKET_SIZE` (2^28).
pub(super) const EVENT_BITMAP_BUCKET_SIZE: u64 = 268_435_456;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LedgerBitmapKind {
    Transaction,
    Event,
}

pub(super) struct DrainedBitmapHits {
    pub(super) items: Vec<u64>,
    pub(super) pending_bucket: Option<PendingBitmapBucket>,
    pub(super) next_range: Option<Range<u64>>,
    pub(super) buckets_scanned: usize,
    /// Furthest progress watermark coalesced from the evaluator during this
    /// chunk, in absolute member-id space (the slowest source's frontier). The
    /// caller resolves it to a checkpoint and emits a scan watermark only
    /// when the chunk produced no items — item chunks carry their own watermark.
    pub(super) coalesced_frontier: Option<u64>,
    /// The per-request bucket-scan budget was exhausted: the query stops with
    /// `ScanLimit` rather than continuing.
    pub(super) scan_limit_hit: bool,
}

/// Evaluated bitmap state carried between blocking chunks.
///
/// The inverted-index evaluator yields `(bucket_id, RoaringBitmap)`. Holding the
/// bitmap plus an absolute remaining range lets us resume a dense bucket without
/// flattening it into an unbounded `Vec<u64>` or rereading it from RocksDB.
#[derive(Clone)]
pub(super) struct PendingBitmapBucket {
    bucket_id: u64,
    bitmap: RoaringBitmap,
    remaining: Range<u64>,
}

impl PendingBitmapBucket {
    fn new(
        bucket_id: u64,
        bitmap: RoaringBitmap,
        range: &Range<u64>,
        bucket_size: u64,
    ) -> Option<Self> {
        let bucket_start = bucket_id * bucket_size;
        let bucket_end = bucket_start + bucket_size;
        let remaining = range.start.max(bucket_start)..range.end.min(bucket_end);
        (!remaining.is_empty() && !bitmap.is_empty()).then_some(Self {
            bucket_id,
            bitmap,
            remaining,
        })
    }

    fn drain_into(
        &mut self,
        out: &mut Vec<u64>,
        hit_limit: usize,
        bucket_size: u64,
        direction: ScanDirection,
    ) {
        while out.len() < hit_limit {
            let Some(seq) = self.next_seq(bucket_size, direction) else {
                break;
            };
            out.push(seq);
            self.remaining =
                remaining_range_after(self.remaining.clone(), seq, direction.is_ascending())
                    .unwrap_or(0..0);
        }
    }

    fn has_hits(&self, bucket_size: u64, direction: ScanDirection) -> bool {
        self.next_seq(bucket_size, direction).is_some()
    }

    fn next_seq(&self, bucket_size: u64, direction: ScanDirection) -> Option<u64> {
        if self.remaining.is_empty() {
            return None;
        }

        let bucket_start = self.bucket_id * bucket_size;
        let lo = (self.remaining.start - bucket_start) as u32;
        let hi = (self.remaining.end - bucket_start) as u32;
        let bit = if direction.is_ascending() {
            self.bitmap.range(lo..hi).next()
        } else {
            self.bitmap.range(lo..hi).next_back()
        }?;
        Some(bucket_start + u64::from(bit))
    }

    #[cfg(test)]
    fn remaining(&self) -> Range<u64> {
        self.remaining.clone()
    }
}

#[derive(Clone)]
struct RpcIndexesBitmapSource<'a> {
    service: &'a RpcService,
    kind: LedgerBitmapKind,
    bucket_size: u64,
    scan_budget: BitmapScanBudget,
    cancel: CancellationToken,
}

impl<'a> RpcIndexesBitmapSource<'a> {
    fn new(
        service: &'a RpcService,
        kind: LedgerBitmapKind,
        bucket_size: u64,
        scan_budget: BitmapScanBudget,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            service,
            kind,
            bucket_size,
            scan_budget,
            cancel,
        }
    }
}

pub(super) fn drain_bitmap_hits_with_budget(
    service: RpcService,
    kind: LedgerBitmapKind,
    bucket_size: u64,
    query: BitmapQuery,
    pending_bucket: Option<PendingBitmapBucket>,
    range: Option<Range<u64>>,
    direction: ScanDirection,
    hit_limit: usize,
    scan_budget: usize,
    cancel: &CancellationToken,
) -> Result<DrainedBitmapHits, RpcError> {
    if hit_limit == 0 {
        return Ok(DrainedBitmapHits {
            items: Vec::new(),
            pending_bucket,
            next_range: range,
            buckets_scanned: 0,
            coalesced_frontier: None,
            scan_limit_hit: false,
        });
    }

    // The iterator evaluator in `sui-inverted-index` is synchronous, so the
    // borrowed RocksDB iterators stay on this blocking task for the whole chunk.
    // That avoids a runtime dependency in this path while keeping BigTable on
    // the async stream evaluator.
    let scan_budget_start = scan_budget;
    let scan_budget = BitmapScanBudget::new(scan_budget_start);
    let source = RpcIndexesBitmapSource::new(
        &service,
        kind,
        bucket_size,
        scan_budget.clone(),
        cancel.clone(),
    );
    let state = drain_watermarked_buckets(
        move |scan_range| {
            eval_bitmap_query_bucket_iter(source, query, scan_range, bucket_size, direction)
        },
        pending_bucket,
        range,
        bucket_size,
        direction,
        hit_limit,
        cancel,
    )?;
    Ok(DrainedBitmapHits {
        items: state.items,
        pending_bucket: state.pending_bucket,
        next_range: state.next_range,
        buckets_scanned: scan_budget_start - scan_budget.remaining(),
        coalesced_frontier: state.coalesced_frontier,
        scan_limit_hit: state.scan_limit_hit,
    })
}

/// Item type of the iterative bitmap evaluator: a matching bucket, a progress
/// watermark, or a scan error (e.g. `BitmapScanBudgetExceeded`).
type WatermarkedBucketItem = anyhow::Result<Watermarked<(u64, RoaringBitmap)>>;

/// Loop state from draining a watermarked-bucket iterator, independent of budget
/// accounting (the caller adds `buckets_scanned` from its `BitmapScanBudget`).
struct DrainLoopState {
    items: Vec<u64>,
    pending_bucket: Option<PendingBitmapBucket>,
    next_range: Option<Range<u64>>,
    coalesced_frontier: Option<u64>,
    scan_limit_hit: bool,
}

/// Drain matching member ids until `hit_limit` items, the iterator ends, or it
/// signals `BitmapScanBudgetExceeded`. `open_iter` builds the iterator over a
/// scan range and is invoked at most once, lazily — only when a fresh scan beyond
/// `pending_bucket` is needed — so resuming a dense pending bucket never opens a
/// RocksDB iterator. On budget exhaustion the resume range is anchored past the
/// coalesced frontier; the caller decides whether that is the per-request cap
/// (terminal `ScanLimit`) or a per-chunk cap (resume in the next chunk).
fn drain_watermarked_buckets<I>(
    open_iter: impl FnOnce(Range<u64>) -> I,
    mut pending_bucket: Option<PendingBitmapBucket>,
    mut next_range: Option<Range<u64>>,
    bucket_size: u64,
    direction: ScanDirection,
    hit_limit: usize,
    cancel: &CancellationToken,
) -> Result<DrainLoopState, RpcError>
where
    I: Iterator<Item = WatermarkedBucketItem>,
{
    let mut open_iter = Some(open_iter);
    let mut iter: Option<I> = None;
    let mut iter_range: Option<Range<u64>> = None;
    let mut out = Vec::new();
    // Furthest progress watermark seen this chunk. The evaluator emits watermarks
    // monotonically in scan direction, so last-seen is furthest.
    let mut coalesced_frontier: Option<u64> = None;
    let mut scan_limit_hit = false;
    while out.len() < hit_limit {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        if let Some(mut bucket) = pending_bucket.take() {
            bucket.drain_into(&mut out, hit_limit, bucket_size, direction);
            if bucket.has_hits(bucket_size, direction) {
                pending_bucket = Some(bucket);
                break;
            }
            continue;
        }

        let Some(scan_range) = next_range.clone() else {
            break;
        };
        if iter.is_none() {
            let open = open_iter
                .take()
                .expect("bitmap query iterator is only opened once");
            iter_range = Some(scan_range.clone());
            iter = Some(open(scan_range));
        }

        let iter_range = iter_range
            .as_ref()
            .expect("bitmap iterator range is set before polling");
        match iter
            .as_mut()
            .expect("bitmap iterator is set before polling")
            .next()
        {
            Some(Ok(Watermarked::Item((bucket_id, bitmap)))) => {
                next_range =
                    remaining_range_after_bucket(iter_range, bucket_id, bucket_size, direction);
                pending_bucket =
                    PendingBitmapBucket::new(bucket_id, bitmap, iter_range, bucket_size);
            }
            // Progress watermark: advance the chunk frontier without consuming
            // item budget. Lets the caller emit a scan watermark when the chunk
            // matches nothing across a sparse gap.
            Some(Ok(Watermarked::Watermark(pos))) => {
                coalesced_frontier = Some(pos);
            }
            Some(Err(e)) => {
                // Budget exhaustion is a graceful stop, not an error: the
                // combinators emit the frontier watermark before the error, so
                // `coalesced_frontier` already holds the resume point. Anchor the
                // resume range past the frontier; the caller decides whether this
                // is the per-request cap (terminal `ScanLimit`) or a per-chunk cap
                // (resume here in the next chunk).
                if e.downcast_ref::<BitmapScanBudgetExceeded>().is_some() {
                    scan_limit_hit = true;
                    next_range = coalesced_frontier.and_then(|f| {
                        remaining_range_after(iter_range.clone(), f, direction.is_ascending())
                    });
                    break;
                }
                let code = if e.downcast_ref::<BitmapScanCancelled>().is_some() {
                    tonic::Code::Cancelled
                } else {
                    tonic::Code::Internal
                };
                return Err(RpcError::new(code, e.to_string()));
            }
            None => {
                next_range = None;
                break;
            }
        }
    }
    Ok(DrainLoopState {
        items: out,
        pending_bucket,
        next_range,
        coalesced_frontier,
        scan_limit_hit,
    })
}

fn remaining_range_after_bucket(
    range: &Range<u64>,
    bucket_id: u64,
    bucket_size: u64,
    direction: ScanDirection,
) -> Option<Range<u64>> {
    let remaining = if direction.is_ascending() {
        ((bucket_id + 1) * bucket_size).max(range.start)..range.end
    } else {
        range.start..(bucket_id * bucket_size).min(range.end)
    };
    (!remaining.is_empty()).then_some(remaining)
}

impl<'a> BitmapBucketIteratorSource<'a> for RpcIndexesBitmapSource<'a> {
    type Iter = RpcIndexesBitmapIterator<'a>;

    fn scan_bucket_iter(
        &self,
        dimension_key: Vec<u8>,
        range: Range<u64>,
        direction: ScanDirection,
    ) -> Self::Iter {
        RpcIndexesBitmapIterator::new(
            self.service,
            self.kind,
            self.bucket_size,
            self.scan_budget.clone(),
            self.cancel.clone(),
            dimension_key,
            range,
            direction,
        )
    }
}

/// Inner bucket source: a stored RocksDB scan, or the synthesized dense
/// tx-universe sequence — `IndexDimension::TxUniverse` is query-only and
/// never reaches storage.
enum BitmapBucketIter<'a> {
    Stored(LedgerBitmapBucketIterator<'a>),
    Universe(Box<dyn Iterator<Item = (u64, RoaringBitmap)> + Send + 'a>),
}

struct RpcIndexesBitmapIterator<'a> {
    scan_budget: BitmapScanBudget,
    cancel: CancellationToken,
    iter: Option<BitmapBucketIter<'a>>,
    finished: bool,
    initial_error: Option<anyhow::Error>,
    /// This leaf has not yet charged a bucket. Its first bucket is reserved
    /// (always allowed) so every leaf emits its first watermark; see
    /// [`BitmapScanBudget::take_first`].
    first: bool,
}

impl<'a> RpcIndexesBitmapIterator<'a> {
    fn new(
        service: &'a RpcService,
        kind: LedgerBitmapKind,
        bucket_size: u64,
        scan_budget: BitmapScanBudget,
        cancel: CancellationToken,
        dimension_key: Vec<u8>,
        range: Range<u64>,
        direction: ScanDirection,
    ) -> Self {
        if range.is_empty() {
            return Self {
                scan_budget,
                cancel,
                iter: None,
                finished: true,
                initial_error: None,
                first: true,
            };
        }

        // The tx universe is dense (every tx_seq in range is real), so it is
        // synthesized rather than read from storage. Gated on the tx kind: the
        // key is only ever produced by the tx filter layer, and in event-space
        // a dense universe would be semantically wrong.
        if kind == LedgerBitmapKind::Transaction
            && dimension_key.first() == Some(&IndexDimension::TxUniverse.tag_byte())
        {
            return Self {
                scan_budget,
                cancel,
                finished: false,
                iter: Some(BitmapBucketIter::Universe(Box::new(
                    dense_universe_buckets(range, bucket_size, direction),
                ))),
                initial_error: None,
                first: true,
            };
        }

        let start_bucket = range.start / bucket_size;
        let end_bucket_exclusive = (range.end - 1) / bucket_size + 1;
        let iter = service
            .reader
            .inner()
            .indexes()
            .ok_or_else(|| anyhow::anyhow!("rpc indexes are disabled"))
            .and_then(|indexes| {
                let descending = !direction.is_ascending();
                let iter = match kind {
                    LedgerBitmapKind::Transaction => indexes.transaction_bitmap_bucket_iter(
                        dimension_key,
                        start_bucket,
                        end_bucket_exclusive,
                        descending,
                    ),
                    LedgerBitmapKind::Event => indexes.event_bitmap_bucket_iter(
                        dimension_key,
                        start_bucket,
                        end_bucket_exclusive,
                        descending,
                    ),
                };
                iter.map_err(|e| anyhow::anyhow!(e.to_string()))
            });

        let (iter, initial_error) = match iter {
            Ok(iter) => (Some(BitmapBucketIter::Stored(iter)), None),
            Err(e) => (None, Some(e)),
        };
        Self {
            scan_budget,
            cancel,
            finished: false,
            iter,
            initial_error,
            first: true,
        }
    }

    fn read_next_bucket(&mut self) -> Option<anyhow::Result<(u64, RoaringBitmap)>> {
        if self.cancel.is_cancelled() {
            self.finished = true;
            return Some(Err(BitmapScanCancelled.into()));
        }
        let Some(iter) = self.iter.as_mut() else {
            self.finished = true;
            return None;
        };

        let next = match iter {
            BitmapBucketIter::Stored(iter) => match iter.next() {
                Some(Ok(bucket)) => Some(Ok((bucket.bucket_id, bucket.bitmap))),
                Some(Err(e)) => Some(Err(anyhow::anyhow!(e.to_string()))),
                None => None,
            },
            BitmapBucketIter::Universe(iter) => iter.next().map(Ok),
        };
        match next {
            Some(Ok(bucket)) => {
                if self.first {
                    // Reserved: a leaf's first bucket is always allowed so it
                    // emits its first watermark even if sibling leaves already
                    // drained the shared budget. Without it a starved leaf
                    // never reports a watermark, its merge slot stays `None`,
                    // and the scan ends with a cursorless `ScanLimit` — a
                    // client livelock. See [`BitmapScanBudget::take_first`].
                    self.scan_budget.take_first();
                    self.first = false;
                } else if let Err(e) = self.scan_budget.take_one() {
                    self.finished = true;
                    return Some(Err(e));
                }
                Some(Ok(bucket))
            }
            None => {
                self.finished = true;
                None
            }
            Some(Err(e)) => {
                self.finished = true;
                Some(Err(e))
            }
        }
    }
}

impl Iterator for RpcIndexesBitmapIterator<'_> {
    type Item = anyhow::Result<(u64, RoaringBitmap)>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(e) = self.initial_error.take() {
            self.finished = true;
            return Some(Err(e));
        }
        if self.finished {
            return None;
        }

        self.read_next_bucket()
    }
}

#[derive(Clone)]
struct BitmapScanBudget {
    remaining: Rc<Cell<usize>>,
}

impl BitmapScanBudget {
    fn new(scan_budget: usize) -> Self {
        Self {
            remaining: Rc::new(Cell::new(scan_budget)),
        }
    }

    fn take_one(&self) -> anyhow::Result<()> {
        let remaining = self.remaining.get();
        if remaining == 0 {
            return Err(BitmapScanBudgetExceeded.into());
        }
        self.remaining.set(remaining - 1);
        Ok(())
    }

    /// Charge a leaf's mandatory first bucket: decrements when it can but
    /// always succeeds, so every leaf is guaranteed to emit its first
    /// watermark regardless of how sibling leaves drained the shared pool.
    /// Saturating at 0 only undercounts a first bucket taken after the pool
    /// was already exhausted; the common `budget >> leaves` case stays exact.
    fn take_first(&self) {
        self.remaining.set(self.remaining.get().saturating_sub(1));
    }

    fn remaining(&self) -> usize {
        self.remaining.get()
    }
}

#[derive(Debug)]
struct BitmapScanBudgetExceeded;

impl std::fmt::Display for BitmapScanBudgetExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("bitmap scan budget exhausted")
    }
}

impl std::error::Error for BitmapScanBudgetExceeded {}

#[derive(Debug)]
struct BitmapScanCancelled;

impl std::fmt::Display for BitmapScanCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("bitmap scan cancelled")
    }
}

impl std::error::Error for BitmapScanCancelled {}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(bits: &[u32]) -> RoaringBitmap {
        let mut bitmap = RoaringBitmap::new();
        for bit in bits {
            bitmap.insert(*bit);
        }
        bitmap
    }

    #[test]
    fn pending_bitmap_bucket_drains_ascending_and_resumes() {
        let mut bucket = PendingBitmapBucket::new(2, bitmap(&[1, 3, 5]), &(200..210), 100).unwrap();
        let mut out = Vec::new();

        bucket.drain_into(&mut out, 2, 100, ScanDirection::Ascending);

        assert_eq!(out, vec![201, 203]);
        assert_eq!(bucket.remaining(), 204..210);
        assert_eq!(bucket.next_seq(100, ScanDirection::Ascending), Some(205));
    }

    #[test]
    fn pending_bitmap_bucket_drains_descending_and_resumes() {
        let mut bucket = PendingBitmapBucket::new(2, bitmap(&[1, 3, 5]), &(200..210), 100).unwrap();
        let mut out = Vec::new();

        bucket.drain_into(&mut out, 2, 100, ScanDirection::Descending);

        assert_eq!(out, vec![205, 203]);
        assert_eq!(bucket.remaining(), 200..203);
        assert_eq!(bucket.next_seq(100, ScanDirection::Descending), Some(201));
    }

    #[test]
    fn pending_bitmap_bucket_trims_to_requested_range() {
        let mut bucket =
            PendingBitmapBucket::new(2, bitmap(&[1, 3, 5, 7]), &(203..207), 100).unwrap();
        let mut out = Vec::new();

        bucket.drain_into(&mut out, usize::MAX, 100, ScanDirection::Ascending);

        assert_eq!(out, vec![203, 205]);
        assert!(!bucket.has_hits(100, ScanDirection::Ascending));
    }

    #[test]
    fn budget_first_bucket_is_always_allowed() {
        let budget = BitmapScanBudget::new(1);
        assert!(budget.take_one().is_ok());
        assert_eq!(budget.remaining(), 0);
        // The pool is drained, so `take_one` now fails — but a leaf reaching it
        // still emits its reserved first bucket via `take_first`, which
        // saturates at 0 instead of erroring. This is what guarantees every
        // leaf reports a watermark and the scan never ends cursorless.
        assert!(budget.take_one().is_err());
        budget.take_first();
        assert_eq!(budget.remaining(), 0);
    }

    #[test]
    fn bitmap_scan_cancelled_is_downcastable_via_anyhow() {
        let e: anyhow::Error = BitmapScanCancelled.into();
        assert!(e.downcast_ref::<BitmapScanCancelled>().is_some());
        assert!(e.downcast_ref::<BitmapScanBudgetExceeded>().is_none());
    }

    fn wm(pos: u64) -> WatermarkedBucketItem {
        Ok(Watermarked::Watermark(pos))
    }

    fn hit(bucket_id: u64, bits: &[u32]) -> WatermarkedBucketItem {
        Ok(Watermarked::Item((bucket_id, bitmap(bits))))
    }

    fn budget_exceeded() -> WatermarkedBucketItem {
        Err(BitmapScanBudgetExceeded.into())
    }

    fn drain(
        events: Vec<WatermarkedBucketItem>,
        pending: Option<PendingBitmapBucket>,
        range: Option<Range<u64>>,
        bucket_size: u64,
        direction: ScanDirection,
        hit_limit: usize,
    ) -> DrainLoopState {
        let cancel = CancellationToken::new();
        drain_watermarked_buckets(
            move |_range| events.into_iter(),
            pending,
            range,
            bucket_size,
            direction,
            hit_limit,
            &cancel,
        )
        .expect("drain succeeds")
    }

    #[test]
    fn budget_exceeded_anchors_resume_past_frontier_ascending() {
        let state = drain(
            vec![wm(10), wm(25), budget_exceeded()],
            None,
            Some(0..100),
            1,
            ScanDirection::Ascending,
            10,
        );
        assert!(state.scan_limit_hit);
        assert!(state.items.is_empty());
        assert_eq!(state.coalesced_frontier, Some(25));
        // Resume strictly past the frontier, still within the scan range.
        assert_eq!(state.next_range, Some(26..100));
        assert!(state.pending_bucket.is_none());
    }

    #[test]
    fn budget_exceeded_anchors_resume_past_frontier_descending() {
        let state = drain(
            vec![wm(80), wm(40), budget_exceeded()],
            None,
            Some(0..100),
            1,
            ScanDirection::Descending,
            10,
        );
        assert!(state.scan_limit_hit);
        assert_eq!(state.coalesced_frontier, Some(40));
        // Descending resume is the range below the frontier (exclusive).
        assert_eq!(state.next_range, Some(0..40));
    }

    #[test]
    fn budget_exceeded_without_frontier_leaves_no_resume_range() {
        // No watermark before exhaustion: nothing to resume from, so the caller
        // must treat this as terminal rather than re-scan the same range.
        let state = drain(
            vec![budget_exceeded()],
            None,
            Some(0..100),
            1,
            ScanDirection::Ascending,
            10,
        );
        assert!(state.scan_limit_hit);
        assert_eq!(state.coalesced_frontier, None);
        assert_eq!(state.next_range, None);
    }

    #[test]
    fn natural_end_clears_resume_range() {
        let state = drain(
            vec![hit(0, &[1, 3, 5])],
            None,
            Some(0..1000),
            100,
            ScanDirection::Ascending,
            10,
        );
        assert!(!state.scan_limit_hit);
        assert_eq!(state.items, vec![1, 3, 5]);
        assert_eq!(state.next_range, None);
        assert!(state.pending_bucket.is_none());
    }

    #[test]
    fn hit_limit_preserves_pending_bucket() {
        // hit_limit stops mid-bucket; the partially drained bucket is preserved
        // for the next chunk without ending the scan.
        let state = drain(
            vec![hit(0, &[1, 3, 5])],
            None,
            Some(0..1000),
            100,
            ScanDirection::Ascending,
            2,
        );
        assert!(!state.scan_limit_hit);
        assert_eq!(state.items, vec![1, 3]);
        assert!(state.pending_bucket.is_some());
    }
}
