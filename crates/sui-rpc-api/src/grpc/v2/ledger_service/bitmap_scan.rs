// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use roaring::RoaringBitmap;
use sui_inverted_index::BitmapBucketIteratorSource;
use sui_inverted_index::BitmapQuery;
use sui_inverted_index::DenseUniverseBuckets;
use sui_inverted_index::IndexDimension;
use sui_inverted_index::LeafStop;
use sui_inverted_index::ScanDirection;
use sui_inverted_index::ScanStop;
use sui_inverted_index::SeekableBucketIterator;
use sui_inverted_index::SkipPolicy;
use sui_inverted_index::WatermarkPacing;
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
// Must match the writer's `EVENT_BUCKET_SIZE` in sui-rpc-store's
// `schema::event_bitmap` and sui-kvstore's `event_bitmap_index::BUCKET_SIZE`
// (2^28).
pub(super) const EVENT_BITMAP_BUCKET_SIZE: u64 = 268_435_456;
// Provisional heuristic: favor a native local seek after a short drain probe.
// This only affects performance; correctness does not depend on the threshold.
const BITMAP_DRAIN_PROBE_ROWS: u32 = 2;

/// The fullnode reports a chunk's scan progress only when the chunk ends at its
/// scan budget, and then reports the `ScanStop::ScanLimit` frontier; every
/// other evaluator watermark is dropped (see
/// [`DrainedBitmapHits::scan_limit_frontier`]). So it asks the evaluator for as
/// few watermarks as possible.
const FULLNODE_WATERMARK_PACING: WatermarkPacing = WatermarkPacing {
    min_buckets: u64::MAX,
    emit_held_at_end: false,
};

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
    /// `Some(frontier)` exactly when the evaluator exhausted the scan budget
    /// supplied for this chunk (`ScanStop::ScanLimit`), carrying that
    /// terminal's frontier in absolute member-id space (the slowest source's
    /// frontier). Ascending frontiers are the first unscanned position
    /// (inclusive); descending frontiers are an exclusive upper bound. The
    /// caller decides whether the request budget is also exhausted or another
    /// chunk should continue scanning, and resolves the frontier to a
    /// checkpoint for the request's scan-limit terminal or, if the chunk
    /// produced no items, for a scan watermark.
    pub(super) scan_limit_frontier: Option<u64>,
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
    cancel: CancellationToken,
}

impl<'a> RpcIndexesBitmapSource<'a> {
    fn new(
        service: &'a RpcService,
        kind: LedgerBitmapKind,
        bucket_size: u64,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            service,
            kind,
            bucket_size,
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
            scan_limit_frontier: None,
        });
    }

    // The iterator evaluator in `sui-inverted-index` is synchronous, so the
    // borrowed RocksDB iterators stay on this blocking task for the whole chunk.
    // That avoids a runtime dependency in this path while keeping BigTable on
    // the async stream evaluator. The evaluator charges `scan_budget` and
    // reports the charged bucket rows through `on_metrics` once the iterator
    // finishes or is dropped, which happens inside `drain_watermarked_buckets`.
    // The count stays 0 when the chunk only drains a pending bucket and never
    // opens the iterator.
    let buckets_evaluated = Rc::new(Cell::new(0u64));
    let record_buckets_evaluated = buckets_evaluated.clone();
    let source = RpcIndexesBitmapSource::new(&service, kind, bucket_size, cancel.clone());
    let state = drain_watermarked_buckets(
        move |scan_range| {
            eval_bitmap_query_bucket_iter(
                source,
                query,
                scan_range,
                bucket_size,
                direction,
                scan_budget as u64,
                SkipPolicy {
                    drain_probe_rows: std::num::NonZeroU32::new(BITMAP_DRAIN_PROBE_ROWS),
                },
                FULLNODE_WATERMARK_PACING,
                move |metrics| record_buckets_evaluated.set(metrics.buckets_evaluated),
            )
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
        buckets_scanned: buckets_evaluated.get() as usize,
        scan_limit_frontier: state.scan_limit_frontier,
    })
}

/// Item type of the iterative bitmap evaluator: a matching bucket, a progress
/// watermark, or a terminal [`ScanStop`].
type WatermarkedBucketItem = Result<Watermarked<(u64, RoaringBitmap)>, ScanStop>;

/// Loop state from draining a watermarked-bucket iterator, independent of budget
/// accounting (the caller adds `buckets_scanned` from the evaluator's
/// [`sui_inverted_index::BitmapScanMetrics`]).
struct DrainLoopState {
    items: Vec<u64>,
    pending_bucket: Option<PendingBitmapBucket>,
    next_range: Option<Range<u64>>,
    scan_limit_frontier: Option<u64>,
}

/// Drain matching member ids until `hit_limit` items, the iterator ends, or it
/// signals [`ScanStop::ScanLimit`]. `open_iter` builds the iterator over a scan
/// range and is invoked at most once, lazily — only when a fresh scan beyond
/// `pending_bucket` is needed — so resuming a dense pending bucket never opens
/// a RocksDB iterator. Ascending evaluator frontiers are the first unscanned
/// position and remain inclusive; descending frontiers retain their existing
/// exclusive upper-bound semantics. The caller decides whether a scan-limit
/// stop is the per-request cap or a per-chunk cap.
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
    let mut scan_limit_frontier: Option<u64> = None;
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
            // The fullnode only reports the scan-limit frontier.
            Some(Ok(Watermarked::Watermark(_))) => {}
            Some(Err(ScanStop::ScanLimit { scan_frontier })) => {
                // Budget stop: the terminal carries the merged resume frontier
                // (the exact value the stopping round's beacon would have held).
                // In ascending scans this is the first unscanned position, so
                // resume inclusively without item-consumption's `+ 1`.
                // Descending continuation remains strictly below the frontier.
                scan_limit_frontier = Some(scan_frontier);
                next_range = if direction.is_ascending() {
                    let remaining = scan_frontier.max(iter_range.start)..iter_range.end;
                    (!remaining.is_empty()).then_some(remaining)
                } else {
                    remaining_range_after(iter_range.clone(), scan_frontier, false)
                };
                break;
            }
            // Cancelled stream → gRPC Cancelled status.
            Some(Err(ScanStop::Cancelled)) => {
                return Err(RpcError::new(
                    tonic::Code::Cancelled,
                    ScanStop::Cancelled.to_string(),
                ));
            }
            // Genuine fault → gRPC Internal, carrying the error unchanged.
            Some(Err(ScanStop::Fault(inner))) => {
                return Err(RpcError::new(tonic::Code::Internal, inner.to_string()));
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
        scan_limit_frontier,
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
    Universe(DenseUniverseBuckets),
}

/// Leaf bucket iterator for one index dimension. It yields buckets as read; the
/// evaluator charges each pulled bucket against the scan budget, including the
/// reserved first bucket that lets every leaf report its first watermark.
struct RpcIndexesBitmapIterator<'a> {
    cancel: CancellationToken,
    iter: Option<BitmapBucketIter<'a>>,
    finished: bool,
    initial_error: Option<LeafStop>,
}

impl<'a> RpcIndexesBitmapIterator<'a> {
    fn new(
        service: &'a RpcService,
        kind: LedgerBitmapKind,
        bucket_size: u64,
        cancel: CancellationToken,
        dimension_key: Vec<u8>,
        range: Range<u64>,
        direction: ScanDirection,
    ) -> Self {
        if range.is_empty() {
            return Self {
                cancel,
                iter: None,
                finished: true,
                initial_error: None,
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
                cancel,
                finished: false,
                iter: Some(BitmapBucketIter::Universe(dense_universe_buckets(
                    range,
                    bucket_size,
                    direction,
                ))),
                initial_error: None,
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
            Err(e) => (None, Some(e.into())),
        };
        Self {
            cancel,
            finished: false,
            iter,
            initial_error,
        }
    }

    fn read_next_bucket(&mut self) -> Option<Result<(u64, RoaringBitmap), LeafStop>> {
        if self.cancel.is_cancelled() {
            self.finished = true;
            return Some(Err(LeafStop::Cancelled));
        }
        let Some(iter) = self.iter.as_mut() else {
            self.finished = true;
            return None;
        };

        let next = match iter {
            BitmapBucketIter::Stored(iter) => match iter.next() {
                Some(Ok(bucket)) => Some(Ok((bucket.bucket_id, bucket.bitmap))),
                Some(Err(e)) => Some(Err(LeafStop::Fault(anyhow::anyhow!(e.to_string())))),
                None => None,
            },
            BitmapBucketIter::Universe(iter) => iter.next().map(Ok),
        };
        if !matches!(next, Some(Ok(_))) {
            self.finished = true;
        }
        next
    }
}

impl Iterator for RpcIndexesBitmapIterator<'_> {
    type Item = Result<(u64, RoaringBitmap), LeafStop>;

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
impl SeekableBucketIterator for RpcIndexesBitmapIterator<'_> {
    fn seek_bucket(&mut self, bucket: u64) {
        match self.iter.as_mut() {
            Some(BitmapBucketIter::Stored(iter)) => iter.seek_bucket(bucket),
            Some(BitmapBucketIter::Universe(iter)) => iter.seek_bucket(bucket),
            None => {}
        }
    }
}

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

    fn wm(pos: u64) -> WatermarkedBucketItem {
        Ok(Watermarked::Watermark(pos))
    }

    fn hit(bucket_id: u64, bits: &[u32]) -> WatermarkedBucketItem {
        Ok(Watermarked::Item((bucket_id, bitmap(bits))))
    }

    /// A budget-stop terminal as the merged evaluator emits it: the merged
    /// terminal always carries its frontier.
    fn budget_exceeded(frontier: u64) -> WatermarkedBucketItem {
        Err(ScanStop::ScanLimit {
            scan_frontier: frontier,
        })
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

    fn drain_result(
        events: Vec<WatermarkedBucketItem>,
        range: Option<Range<u64>>,
        bucket_size: u64,
        direction: ScanDirection,
        hit_limit: usize,
    ) -> Result<DrainLoopState, RpcError> {
        let cancel = CancellationToken::new();
        drain_watermarked_buckets(
            move |_range| events.into_iter(),
            None,
            range,
            bucket_size,
            direction,
            hit_limit,
            &cancel,
        )
    }

    /// A `ScanLimit` terminal is a graceful chunk stop: it reports its bundled
    /// frontier and the continuation range is anchored there.
    #[test]
    fn scan_limit_terminal_sets_continuation_range() {
        let state = drain(
            vec![budget_exceeded(25)],
            None,
            Some(0..100),
            1,
            ScanDirection::Ascending,
            10,
        );
        assert_eq!(state.scan_limit_frontier, Some(25));
        assert_eq!(state.next_range, Some(25..100));
    }

    /// A `Fault` terminal (what a collapsed budget+fault aggregate becomes)
    /// must surface as `Internal`, never masked as a clean `ScanLimit` end.
    #[test]
    fn fault_terminal_is_internal() {
        let err = drain_result(
            vec![
                wm(10),
                Err(ScanStop::Fault(anyhow::anyhow!("storage boom"))),
            ],
            Some(0..100),
            1,
            ScanDirection::Ascending,
            10,
        )
        .err()
        .expect("fault must error");
        assert_eq!(tonic::Status::from(err).code(), tonic::Code::Internal);
    }

    /// A `Cancelled` terminal must surface as gRPC `Cancelled`, not a clean
    /// `ScanLimit` end.
    #[test]
    fn cancelled_terminal_is_cancelled() {
        let err = drain_result(
            vec![wm(10), Err(ScanStop::Cancelled)],
            Some(0..100),
            1,
            ScanDirection::Ascending,
            10,
        )
        .err()
        .expect("cancelled must error");
        assert_eq!(tonic::Status::from(err).code(), tonic::Code::Cancelled);
    }

    #[test]
    fn budget_exceeded_resumes_at_frontier_ascending() {
        let state = drain(
            vec![wm(10), wm(25), budget_exceeded(25)],
            None,
            Some(0..100),
            1,
            ScanDirection::Ascending,
            10,
        );
        assert!(state.items.is_empty());
        assert_eq!(state.scan_limit_frontier, Some(25));
        // The evaluator frontier is the first unscanned position.
        assert_eq!(state.next_range, Some(25..100));
        assert!(state.pending_bucket.is_none());
    }

    #[test]
    fn budget_exceeded_anchors_resume_past_frontier_descending() {
        let state = drain(
            vec![wm(80), wm(40), budget_exceeded(40)],
            None,
            Some(0..100),
            1,
            ScanDirection::Descending,
            10,
        );
        assert_eq!(state.scan_limit_frontier, Some(40));
        // Descending resume is the range below the frontier (exclusive).
        assert_eq!(state.next_range, Some(0..40));
    }

    #[test]
    fn ascending_scan_limit_continuation_emits_frontier_bucket_edge_once() {
        let stopped = drain(
            vec![budget_exceeded(100)],
            None,
            Some(0..300),
            100,
            ScanDirection::Ascending,
            10,
        );
        assert_eq!(stopped.next_range, Some(100..300));

        let resumed = drain(
            vec![hit(1, &[0])],
            stopped.pending_bucket,
            stopped.next_range,
            100,
            ScanDirection::Ascending,
            10,
        );
        assert_eq!(resumed.items, vec![100]);
        assert_eq!(resumed.items.iter().filter(|&&item| item == 100).count(), 1);
    }

    #[test]
    fn ascending_scan_limit_frontier_at_numeric_end_is_safe() {
        let state = drain(
            vec![budget_exceeded(u64::MAX)],
            None,
            Some((u64::MAX - 1)..u64::MAX),
            1,
            ScanDirection::Ascending,
            10,
        );
        assert_eq!(state.scan_limit_frontier, Some(u64::MAX));
        assert_eq!(state.next_range, None);
    }

    /// Only a `ScanLimit` terminal reports a frontier: progress watermarks
    /// before it, or before a natural end, do not.
    #[test]
    fn scan_limit_frontier_comes_only_from_scan_limit_terminal() {
        let stopped = drain(
            vec![wm(10), budget_exceeded(40)],
            None,
            Some(0..100),
            1,
            ScanDirection::Ascending,
            10,
        );
        assert_eq!(stopped.scan_limit_frontier, Some(40));
        assert_eq!(stopped.next_range, Some(40..100));

        let ended = drain(
            vec![wm(10), wm(40)],
            None,
            Some(0..100),
            1,
            ScanDirection::Ascending,
            10,
        );
        assert_eq!(ended.scan_limit_frontier, None);
        assert_eq!(ended.next_range, None);
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
        assert_eq!(state.scan_limit_frontier, None);
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
        assert_eq!(state.scan_limit_frontier, None);
        assert_eq!(state.items, vec![1, 3]);
        assert!(state.pending_bucket.is_some());
    }
}
