// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Async stream evaluator for DNF bitmap queries.
//!
//! A single flat driver merge-joins every leaf scan against one shared *floor*
//! (the slowest leaf's position). At the floor bucket it evaluates the query —
//! intersect each term's included dimensions, subtract its excluded ones, then
//! union across terms — and emits a watermark at the floor. Leaves only ever
//! advance at the floor (peeked one bucket ahead, polled concurrently), so no
//! branch can run ahead of the others: the resume cursor stays within one sparse
//! read of every leaf, and there is no windowing/parking to get wrong. Consumed
//! by streaming backends such as BigTable; the synchronous
//! [`super::eval_bitmap_query_bucket_iter`] mirrors it and shares the per-bucket
//! evaluation ([`eval_term_at_bucket`]).

use std::ops::Range;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use anyhow::Result;
use futures::Stream;
use futures::StreamExt;
use futures::stream::BoxStream;
use futures::stream::Peekable;
use roaring::RoaringBitmap;

use super::BitmapBucketSource;
use super::BitmapQuery;
use super::BitmapScanLimitExceeded;
use super::BucketItem;
use super::BucketStream;
use super::DedupedQuery;
use super::LeafHead;
use super::MultiError;
use super::ScanDirection;
use super::Watermarked;
use super::WatermarkedBucketStream;
use super::bound_in_direction;
use super::bucket_edges;
use super::build_term_specs;
use super::count_on_floor_refs;
use super::eval_term_at_bucket;
use super::frontier_advanced;
use super::recompute_unreferenced;
use super::take_snapshot_bitmap;

/// Per-request bucket-scan accounting, delivered via the `on_metrics`
/// callback passed to `eval_bitmap_query_stream`. Fires once when the
/// eval pipeline is dropped (natural end, error, or consumer cancel).
/// The sole exception is the budget-misconfig early-out, which errors
/// before any scan is set up and emits nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BitmapScanMetrics {
    /// Buckets actually evaluated (charged against the per-request
    /// budget). At exhaustion each leaf may have fetched one extra
    /// bucket that was discarded rather than evaluated, so observed
    /// backend reads can exceed this by up to `BitmapQuery::unique_leaf_count()`.
    pub buckets_evaluated: u64,
}

/// Per-request evaluated-bucket budget shared across all dimension
/// streams of one eval. Charges are post-poll — see
/// `budgeted_bucket_stream`.
#[derive(Clone)]
pub(crate) struct BitmapScanBudget {
    initial: u64,
    remaining: Arc<AtomicU64>,
}

impl BitmapScanBudget {
    pub(crate) fn new(initial: u64) -> Self {
        Self {
            initial,
            remaining: Arc::new(AtomicU64::new(initial)),
        }
    }

    /// Charge one bucket. Returns false on underflow.
    fn try_take(&self) -> bool {
        self.remaining
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |b| {
                if b == 0 { None } else { Some(b - 1) }
            })
            .is_ok()
    }

    /// Charge a leaf's mandatory first bucket: decrements the shared pool
    /// when it can, but ALWAYS succeeds. The runtime guards `budget >=
    /// unique_leaf_count`, but a *shared* atomic with concurrent leaves
    /// gives no ordering guarantee — a sparse term can drain the pool
    /// before a slower sibling leaf charges its first bucket, leaving that
    /// leaf unable to report its first position (a cursorless `SCAN_LIMIT`).
    /// Reserving the first bucket per leaf makes the `unique_leaf_count`
    /// floor's promise — "every leaf reaches its first bucket" — hold.
    /// Charging-when-possible keeps `buckets_evaluated` accurate in the
    /// common `budget >> leaves` case; it only undercounts a first bucket
    /// taken after the pool was already exhausted by other leaves.
    fn take_first(&self) {
        let _ = self
            .remaining
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |b| {
                Some(b.saturating_sub(1))
            });
    }

    fn buckets_evaluated(&self) -> u64 {
        self.initial
            .saturating_sub(self.remaining.load(Ordering::SeqCst))
    }
}

/// RAII guard: fires `on_metrics` exactly once on drop with the final
/// `BitmapScanMetrics`. Held inside the boxed eval stream so the callback
/// fires on natural end, error, or consumer cancel.
struct ObserveOnDrop<F: FnOnce(BitmapScanMetrics) + Send + 'static> {
    budget: BitmapScanBudget,
    callback: Option<F>,
}

impl<F: FnOnce(BitmapScanMetrics) + Send + 'static> Drop for ObserveOnDrop<F> {
    fn drop(&mut self) {
        if let Some(cb) = self.callback.take() {
            cb(BitmapScanMetrics {
                buckets_evaluated: self.budget.buckets_evaluated(),
            });
        }
    }
}

/// Evaluate a DNF `BitmapQuery` against a backend-provided bitmap source.
///
/// `budget` caps evaluated buckets across all dimension scans (see
/// [`BitmapScanLimitExceeded`] and [`BitmapScanMetrics`]). `on_metrics`
/// fires exactly once when the eval stream is dropped.
///
/// Output emits `Watermarked::Item(absolute_member_id)` interleaved with
/// `Watermarked::Watermark(p)` derived from the slowest leaf — sparse scans
/// that match nothing still report progress at the rate sources advance.
pub fn eval_bitmap_query_stream<S, F>(
    source: S,
    query: BitmapQuery,
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
    budget: u64,
    on_metrics: F,
) -> BoxStream<'static, Result<Watermarked<u64>>>
where
    S: BitmapBucketSource,
    F: FnOnce(BitmapScanMetrics) + Send + 'static,
{
    let leaves = query.unique_leaf_count();
    if (budget as usize) < leaves {
        // Misconfig guard: short-circuit before any scan setup. No
        // `on_metrics` here — there's no scan to account for, and the
        // error surfaces on its own. `on_metrics` is dropped uncalled.
        return async_stream::stream! {
            yield Err(anyhow::anyhow!(
                "bitmap scan budget {budget} is insufficient for {leaves} leaf streams; \
                 server is misconfigured"
            ));
        }
        .boxed();
    }
    let budget = BitmapScanBudget::new(budget);
    let bucket_stream = eval_bitmap_query_bucket_stream(
        source,
        query,
        range.clone(),
        bucket_size,
        direction,
        budget.clone(),
    );
    let inner = flatten_watermarked_buckets(bucket_stream, range.clone(), bucket_size, direction);
    // Wrapping the guard inside `async_stream::stream!` keeps it alive
    // for the stream's full lifetime; the callback fires when the
    // consumer drops the returned `BoxStream`.
    let guard = ObserveOnDrop {
        budget,
        callback: Some(on_metrics),
    };
    async_stream::stream! {
        let _guard = guard;
        futures::pin_mut!(inner);
        while let Some(item) = inner.next().await {
            yield item;
        }
    }
    .boxed()
}

/// Non-consuming peek of one leaf, paired with its index and the position it has
/// now scanned to (`None` for an error head, which leaves the prior position).
async fn peek_leaf<S>(
    mut leaf: Pin<&mut Peekable<S>>,
    idx: usize,
    bucket_size: u64,
    range: &Range<u64>,
    direction: ScanDirection,
    terminus: u64,
) -> (usize, LeafHead, Option<u64>)
where
    S: Stream<Item = BucketItem>,
{
    match leaf.as_mut().peek().await {
        Some(Ok((bucket, _))) => {
            let (pre, _post) = bucket_edges(*bucket, bucket_size, range, direction);
            (idx, LeafHead::Bucket(*bucket), Some(pre))
        }
        None => (idx, LeafHead::Eof, Some(terminus)),
        Some(Err(_)) => (idx, LeafHead::Error, None),
    }
}

/// Evaluate a DNF `BitmapQuery` as an ordered `WatermarkedBucketStream`.
///
/// The flat driver: each round peeks every active leaf concurrently, takes the
/// slowest leaf's position as the floor (the merged watermark), evaluates the
/// whole DNF at the floor bucket, and advances only the leaves sitting there.
/// No leaf runs more than one peeked bucket ahead of the floor.
pub(crate) fn eval_bitmap_query_bucket_stream<S>(
    source: S,
    query: BitmapQuery,
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
    budget: BitmapScanBudget,
) -> WatermarkedBucketStream
where
    S: BitmapBucketSource,
{
    // One peekable leaf per *unique* dimension key — terms reference them by
    // index. Identical keys across literals share a single backend scan, so
    // `(sender=A AND module=X) OR (sender=A AND type=Y)` reads `sender=A`
    // once. Budgeted bucket streams are `'static`, so `source` is only
    // borrowed while building.
    let DedupedQuery {
        keys: unique_keys,
        mut terms,
    } = build_term_specs(query.terms);
    let mut leaves: Vec<Peekable<BucketStream>> = Vec::with_capacity(unique_keys.len());
    for key in unique_keys {
        let raw = source.scan_bucket_stream(key, range.clone(), direction);
        leaves.push(
            budgeted_bucket_stream(raw, budget.clone())
                .boxed()
                .peekable(),
        );
    }

    let leaf_count = leaves.len();
    let terminus = if direction.is_ascending() {
        range.end
    } else {
        range.start
    };
    let request_floor = if direction.is_ascending() {
        range.start
    } else {
        range.end
    };

    async_stream::try_stream! {
        // `unreferenced[i]`: leaf is retired — either no satisfiable term still
        // points at it, or its bucket stream is at EOF (a spent exclude).
        let mut unreferenced = vec![false; leaf_count];
        // `front[i]`: clamped position each leaf has provably scanned to. Bounds
        // the resume cursor when a leaf errors before it can advance.
        let mut front = vec![request_floor; leaf_count];
        let mut last_emitted: Option<u64> = None;

        loop {
            // Peek every active leaf concurrently (preserves cross-scan
            // parallelism), recording each head and the position it scanned to.
            let mut peeks = Vec::new();
            for (i, leaf) in leaves.iter_mut().enumerate() {
                if !unreferenced[i] {
                    peeks.push(peek_leaf(
                        Pin::new(leaf),
                        i,
                        bucket_size,
                        &range,
                        direction,
                        terminus,
                    ));
                }
            }
            let results = futures::future::join_all(peeks).await;
            let mut class: Vec<Option<LeafHead>> = (0..leaf_count).map(|_| None).collect();
            for (i, head, scanned_to) in results {
                if let Some(p) = scanned_to {
                    front[i] = p;
                }
                class[i] = Some(head);
            }

            // An include at EOF makes its term unsatisfiable (the intersection
            // is permanently empty). With dedup, an EOF'd leaf may be an
            // include for several terms; all of them become unsatisfiable.
            for term in terms.iter_mut() {
                if !term.unsatisfiable
                    && term
                        .includes
                        .iter()
                        .any(|&i| matches!(class[i], Some(LeafHead::Eof)))
                {
                    term.unsatisfiable = true;
                }
            }
            // Recompute leaf liveness from current term state. A leaf may be
            // shared across terms (include for one, exclude for another), so it
            // can only be retired when no satisfiable term still references
            // it — or when its head is at EOF.
            recompute_unreferenced(&terms, &class, &mut unreferenced);

            // Consume any budget-error frame so the error surfaces (after the
            // floor watermark below).
            let mut errors: Vec<anyhow::Error> = Vec::new();
            for i in 0..leaf_count {
                if !unreferenced[i] && matches!(class[i], Some(LeafHead::Error)) {
                    match Pin::new(&mut leaves[i]).next().await {
                        Some(Err(e)) => errors.push(e),
                        _ => unreachable!("peek classified Error"),
                    }
                }
            }

            let active: Vec<usize> = (0..leaf_count).filter(|&i| !unreferenced[i]).collect();
            if active.is_empty() {
                // Every term retired naturally: cap at the range terminus so the
                // client learns the scan covered the whole range.
                if frontier_advanced(last_emitted, terminus, direction) {
                    yield Watermarked::Watermark(terminus);
                }
                return;
            }

            // The floor is the slowest active leaf's scanned-to position: the
            // merged "every source has scanned past here" watermark.
            let floor_pos = active
                .iter()
                .map(|&i| front[i])
                .reduce(|a, b| bound_in_direction(a, b, direction))
                .expect("active non-empty");
            if frontier_advanced(last_emitted, floor_pos, direction) {
                yield Watermarked::Watermark(floor_pos);
                last_emitted = Some(floor_pos);
            }

            // Budget exhausted: the floor watermark above is the resume cursor;
            // everything below it was fully evaluated in prior rounds.
            if !errors.is_empty() {
                Err(MultiError::collapse(errors))?;
            }

            // Evaluate the DNF at the nearest bucket any active leaf sits on.
            let floor_bucket = active
                .iter()
                .filter_map(|&i| match class[i] {
                    Some(LeafHead::Bucket(b)) => Some(b),
                    _ => None,
                })
                .reduce(|a, b| match direction {
                    ScanDirection::Ascending => a.min(b),
                    ScanDirection::Descending => a.max(b),
                })
                .expect("active leaves carry buckets when there is no error");
            let (_pre, post) = bucket_edges(floor_bucket, bucket_size, &range, direction);

            // Snapshot the bitmaps of leaves sitting on `floor_bucket` —
            // each unique leaf consumed exactly once, regardless of how many
            // terms reference it — then distribute. Without dedup, a leaf
            // shared across multiple terms would otherwise be polled once per
            // term, each call advancing past the bucket so siblings see
            // nothing. The single-consume + distribute keeps storage reads
            // proportional to unique keys, not literal occurrences.
            let mut snapshot: Vec<Option<RoaringBitmap>> =
                (0..leaf_count).map(|_| None).collect();
            let mut on_floor = vec![false; leaf_count];
            for i in 0..leaf_count {
                if !unreferenced[i]
                    && matches!(class[i], Some(LeafHead::Bucket(b)) if b == floor_bucket)
                {
                    on_floor[i] = true;
                    front[i] = post;
                    snapshot[i] = match Pin::new(&mut leaves[i]).next().await {
                        Some(Ok((_, bitmap))) => Some(bitmap),
                        _ => None,
                    };
                }
            }
            let mut remaining_refs = count_on_floor_refs(&terms, &on_floor);

            let mut result: Option<RoaringBitmap> = None;
            for term in &terms {
                if term.unsatisfiable {
                    continue;
                }
                let includes: Vec<Option<RoaringBitmap>> = term
                    .includes
                    .iter()
                    .map(|&i| {
                        take_snapshot_bitmap(&mut snapshot, &mut remaining_refs, &on_floor, i)
                    })
                    .collect();
                let excludes: Vec<Option<RoaringBitmap>> = term
                    .excludes
                    .iter()
                    .map(|&i| {
                        take_snapshot_bitmap(&mut snapshot, &mut remaining_refs, &on_floor, i)
                    })
                    .collect();
                if let Some(bitmap) = eval_term_at_bucket(includes, excludes) {
                    result = Some(match result {
                        None => bitmap,
                        Some(acc) => acc | bitmap,
                    });
                }
            }

            if let Some(bitmap) = result {
                yield Watermarked::Item((floor_bucket, bitmap));
            }
            if frontier_advanced(last_emitted, post, direction) {
                yield Watermarked::Watermark(post);
                last_emitted = Some(post);
            }
        }
    }
    .boxed()
}

/// Wrap a raw per-dimension bucket stream with the shared scan budget: charge
/// one bucket per pull (the first via `take_first`, the rest via `try_take`),
/// yielding [`BitmapScanLimitExceeded`] when the pool is empty. Never a silent
/// EOF — the driver must see the error to truncate at the floor.
fn budgeted_bucket_stream<S>(
    inner: S,
    budget: BitmapScanBudget,
) -> impl Stream<Item = BucketItem> + Send + 'static
where
    S: Stream<Item = BucketItem> + Send + 'static,
{
    async_stream::try_stream! {
        futures::pin_mut!(inner);
        let mut first = true;
        while let Some(item) = inner.next().await {
            let item = item?;
            if first {
                budget.take_first();
                first = false;
            } else if !budget.try_take() {
                Err(anyhow::Error::new(BitmapScanLimitExceeded))?;
            }
            yield item;
        }
    }
}

/// Convenience adapter: wrap a single raw `BucketStream` into a
/// `WatermarkedBucketStream` with one `Watermark(post_bucket)` after each
/// bucket plus one final at the range terminus on EOF.
///
/// The DNF eval pipeline budgets and merges leaves itself; this helper is for
/// backend-side consumers (e.g. RocksDB single-dimension scans) that want
/// bucket-level output without the full eval machinery.
pub fn buckets_with_watermarks<S>(
    stream: S,
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
) -> impl Stream<Item = Result<Watermarked<(u64, RoaringBitmap)>>> + Send + 'static
where
    S: Stream<Item = BucketItem> + Send + 'static,
{
    async_stream::try_stream! {
        if range.is_empty() {
            return;
        }
        futures::pin_mut!(stream);
        let mut last_emitted: Option<u64> = None;
        while let Some(item) = stream.next().await {
            let (bucket_id, bitmap) = item?;
            yield Watermarked::Item((bucket_id, bitmap));
            // Ascending = just past this bucket. Descending = this
            // bucket's low edge. Clamp to the request bounds — cursors
            // round-trip into subsequent requests with different ranges.
            let bucket_start = bucket_id.saturating_mul(bucket_size);
            let watermark = if direction.is_ascending() {
                bucket_start.saturating_add(bucket_size).min(range.end)
            } else {
                bucket_start.max(range.start)
            };
            last_emitted = Some(watermark);
            yield Watermarked::Watermark(watermark);
        }
        // Natural EOF: cap with a watermark at the range boundary so
        // handlers get an explicit "scan covered the range" signal.
        // Skip if a per-bucket watermark already exceeded it.
        let range_end = if direction.is_ascending() {
            range.end
        } else {
            range.start
        };
        let should_emit = match last_emitted {
            None => true,
            Some(prev) => {
                if direction.is_ascending() {
                    range_end > prev
                } else {
                    range_end < prev
                }
            }
        };
        if should_emit {
            yield Watermarked::Watermark(range_end);
        }
    }
}

/// Flatten marked bucket bitmaps into absolute member ids with
/// edge-bucket trimming against `range`. Watermarks pass through
/// unchanged.
pub fn flatten_watermarked_buckets<S>(
    stream: S,
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
) -> impl Stream<Item = Result<Watermarked<u64>>> + Send + 'static
where
    S: Stream<Item = Result<Watermarked<(u64, RoaringBitmap)>>> + Send + 'static,
{
    async_stream::try_stream! {
        if range.is_empty() {
            return;
        }
        let start_bucket = range.start / bucket_size;
        let end_bucket = (range.end - 1) / bucket_size;
        futures::pin_mut!(stream);
        while let Some(item) = stream.next().await {
            match item? {
                Watermarked::Watermark(p) => yield Watermarked::Watermark(p),
                Watermarked::Item((bucket_id, bitmap)) => {
                    let bucket_start = bucket_id * bucket_size;
                    let is_first = bucket_id == start_bucket;
                    let is_last = bucket_id == end_bucket;
                    let lo = if is_first {
                        (range.start - bucket_start) as u32
                    } else {
                        0
                    };
                    let hi = if is_last {
                        ((range.end - bucket_start).min(bucket_size)) as u32
                    } else {
                        bucket_size as u32
                    };

                    if direction.is_ascending() {
                        for bit in bitmap.iter() {
                            if bit >= lo && bit < hi {
                                yield Watermarked::Item(bucket_start + bit as u64);
                            }
                        }
                    } else {
                        for bit in bitmap.iter().rev() {
                            if bit >= lo && bit < hi {
                                yield Watermarked::Item(bucket_start + bit as u64);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use futures::TryStreamExt;
    use futures::stream;

    use super::*;
    use crate::bitmap_query::BitmapLiteral;
    use crate::bitmap_query::BitmapTerm;
    use crate::bitmap_query::BucketStream;
    use crate::bitmap_query::error_contains;

    const BUCKET_SIZE: u64 = 100_000;
    type TestBuckets = BTreeMap<Vec<u8>, Vec<(u64, Vec<u32>)>>;

    #[derive(Clone)]
    struct TestBucketSource {
        buckets: Arc<TestBuckets>,
    }

    impl BitmapBucketSource for TestBucketSource {
        fn scan_bucket_stream(
            &self,
            dimension_key: Vec<u8>,
            _range: Range<u64>,
            direction: ScanDirection,
        ) -> BucketStream {
            let mut buckets = self
                .buckets
                .get(&dimension_key)
                .cloned()
                .unwrap_or_default();
            if matches!(direction, ScanDirection::Descending) {
                buckets.reverse();
            }
            make_bucket_stream(
                buckets
                    .iter()
                    .map(|(bucket_id, bits)| (*bucket_id, bits.as_slice()))
                    .collect(),
            )
        }
    }

    fn make_bitmap(bits: &[u32]) -> RoaringBitmap {
        let mut bm = RoaringBitmap::new();
        for &b in bits {
            bm.insert(b);
        }
        bm
    }

    fn make_bucket_stream(items: Vec<(u64, &[u32])>) -> BucketStream {
        let items: Vec<BucketItem> = items
            .into_iter()
            .map(|(bid, bits)| Ok((bid, make_bitmap(bits))))
            .collect();
        stream::iter(items).boxed()
    }

    /// Drain into (items, watermarks) parallel vecs. Order between the
    /// two is lost; for ordering checks collect `Watermarked` directly.
    async fn drain_marked(
        stream: BoxStream<'static, Result<Watermarked<u64>>>,
    ) -> Result<(Vec<u64>, Vec<u64>)> {
        let all: Vec<Watermarked<u64>> = stream.try_collect().await?;
        let mut items = Vec::new();
        let mut watermarks = Vec::new();
        for m in all {
            match m {
                Watermarked::Item(v) => items.push(v),
                Watermarked::Watermark(f) => watermarks.push(f),
            }
        }
        Ok((items, watermarks))
    }

    fn test_key(value: &[u8]) -> Vec<u8> {
        crate::dimensions::encode_dimension_key(crate::dimensions::IndexDimension::Sender, value)
    }

    fn include(value: &[u8]) -> BitmapLiteral {
        BitmapLiteral::include(test_key(value)).unwrap()
    }

    fn exclude(value: &[u8]) -> BitmapLiteral {
        BitmapLiteral::exclude(test_key(value)).unwrap()
    }

    /// Tightest-budget starvation. `term1 = (a AND b)` is disjoint (matches
    /// nothing) and `term2 = c`'s only data sits far ahead of the request floor.
    /// With `budget == unique_leaf_count`, round 1 still fetches every leaf's first
    /// bucket, so the driver derives a real floor watermark before the budget
    /// exhausts advancing `a`. The scan therefore ends with that forward-progress
    /// watermark ahead of `SCAN_LIMIT` — never a cursorless error that would
    /// livelock the client on retry.
    #[tokio::test]
    async fn nested_term_starvation_emits_frontier_before_scan_limit() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (
                    test_key(b"a"),
                    vec![
                        (0, vec![1]),
                        (1, vec![1]),
                        (2, vec![1]),
                        (3, vec![1]),
                        (4, vec![1]),
                    ],
                ),
                (test_key(b"b"), vec![(50, vec![1])]),
                (test_key(b"c"), vec![(40, vec![7])]),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b")]).unwrap(),
            BitmapTerm::new(vec![include(b"c")]).unwrap(),
        ])
        .unwrap();

        // Budget == unique_leaf_count: the runtime floor, the tightest starvation.
        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..(60 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            3,
            |_| {},
        );
        // Collect rather than try_collect: short-circuiting on Err would
        // drop the pre-error watermark under test.
        let all: Vec<Result<Watermarked<u64>>> = stream.collect().await;

        let last_ok =
            all.iter().rev().find_map(|r| r.as_ref().ok()).expect(
                "a frontier watermark must surface before the (otherwise cursorless) error",
            );
        match last_ok {
            Watermarked::Watermark(p) => assert!(
                *p > 0,
                "frontier must reflect real progress past the request floor (got {p})"
            ),
            Watermarked::Item(_) => panic!("disjoint intersect must not emit items here"),
        }
        let err = all
            .last()
            .expect("non-empty")
            .as_ref()
            .expect_err("scan must terminate with an error");
        assert!(
            error_contains::<BitmapScanLimitExceeded>(err).is_some(),
            "expected BitmapScanLimitExceeded, got {err:?}"
        );
    }

    /// Flush-on-error: a budget error truncates the scan at the floor, but
    /// matches at or below the floor were already emitted in earlier rounds.
    /// Here `c` matches in the FIRST bucket — below where `(a AND b)` exhausts
    /// the budget — so its item must be DELIVERED, and the resume cursor must
    /// advance to that death floor rather than be pinned at the request floor
    /// (the livelock this guards against). The delivered item stays below the
    /// final watermark, so resuming from that cursor will not re-emit it.
    ///
    /// The death floor is `post` of bucket 0 (one `BUCKET_SIZE`): every leaf's
    /// first bucket is reserved (`take_first`), so the shared budget is spent
    /// reaching bucket 0 across the leaves and `(a AND b)` errors before it can
    /// advance to bucket 1.
    #[tokio::test]
    async fn flush_on_error_delivers_below_floor_sibling_result() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (
                    test_key(b"a"),
                    vec![
                        (0, vec![1]),
                        (1, vec![1]),
                        (2, vec![1]),
                        (3, vec![1]),
                        (4, vec![1]),
                    ],
                ),
                (test_key(b"b"), vec![(50, vec![1])]),
                (test_key(b"c"), vec![(0, vec![7])]),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b")]).unwrap(),
            BitmapTerm::new(vec![include(b"c")]).unwrap(),
        ])
        .unwrap();

        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..(60 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            3,
            |_| {},
        );
        let all: Vec<Result<Watermarked<u64>>> = stream.collect().await;

        // c's bucket-0 match (member id 7) is delivered despite term1 dying.
        let items: Vec<u64> = all
            .iter()
            .filter_map(|r| match r {
                Ok(Watermarked::Item(v)) => Some(*v),
                _ => None,
            })
            .collect();
        assert_eq!(items, vec![7], "c's below-floor match must be delivered");

        // Resume frontier advances to term1's death floor (post of bucket 0),
        // not stuck at the request floor 0.
        let last_wm = all
            .iter()
            .rev()
            .find_map(|r| match r {
                Ok(Watermarked::Watermark(p)) => Some(*p),
                _ => None,
            })
            .expect("a frontier watermark must surface");
        assert_eq!(last_wm, BUCKET_SIZE, "frontier must advance past floor");
        // The delivered item is below the resume cursor, so a resume won't
        // re-emit it.
        assert!(
            items.iter().all(|&i| i < last_wm),
            "items must be below the resume cursor"
        );

        let err = all
            .last()
            .expect("non-empty")
            .as_ref()
            .expect_err("scan must terminate with an error");
        assert!(
            error_contains::<BitmapScanLimitExceeded>(err).is_some(),
            "expected BitmapScanLimitExceeded, got {err:?}"
        );
    }

    #[test]
    fn bitmap_query_validation_rejects_empty_shapes() {
        assert!(BitmapQuery::new(Vec::new()).is_err());
        assert!(BitmapLiteral::include(Vec::new()).is_err());
        assert!(
            BitmapLiteral::include(vec![crate::dimensions::IndexDimension::Sender.tag_byte()])
                .is_err()
        );
        assert!(BitmapLiteral::include(vec![0xff, 0x00]).is_err());
        assert!(BitmapTerm::new(vec![exclude(b"neg")]).is_err());
    }

    /// Two terms share the same include literal `a`. Dedup must collapse them
    /// to a single backend scan of `a` and distribute its per-bucket bitmap to
    /// both terms — otherwise term 2 would see `a` already consumed by term 1
    /// at the floor bucket and silently drop matches.
    #[tokio::test]
    async fn shared_include_across_terms_scans_dimension_once() {
        use crate::bitmap_query::test_utils::CountingBucketSource;

        let source = CountingBucketSource::new(BTreeMap::from([
            (test_key(b"a"), vec![(0, vec![1, 2, 3])]),
            (test_key(b"b"), vec![(0, vec![1])]),
            (test_key(b"c"), vec![(0, vec![2])]),
        ]));
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b")]).unwrap(),
            BitmapTerm::new(vec![include(b"a"), include(b"c")]).unwrap(),
        ])
        .unwrap();

        let stream = eval_bitmap_query_stream(
            source.clone(),
            query,
            0..200_000,
            BUCKET_SIZE,
            ScanDirection::Ascending,
            u64::MAX,
            |_| {},
        );
        let (items, _watermarks) = drain_marked(stream).await.unwrap();

        // Term 1: a ∩ b = {1}; term 2: a ∩ c = {2}; OR = {1, 2}. If `a` were
        // not distributed to term 2, term 2 would be empty and items = [1].
        assert_eq!(items, vec![1, 2]);
        // The dedup property: `a` was scanned exactly once.
        assert_eq!(source.scan_count(&test_key(b"a")), 1);
        assert_eq!(source.scan_count(&test_key(b"b")), 1);
        assert_eq!(source.scan_count(&test_key(b"c")), 1);
    }

    /// Same key appearing as include in one term and exclude in another:
    /// dedup still collapses to one leaf, and snapshot-distribute clones so
    /// both polarities see the bitmap.
    #[tokio::test]
    async fn shared_key_across_include_and_exclude_terms_scans_once() {
        use crate::bitmap_query::test_utils::CountingBucketSource;

        let source = CountingBucketSource::new(BTreeMap::from([
            (test_key(b"a"), vec![(0, vec![1, 2])]),
            (test_key(b"b"), vec![(0, vec![1, 2, 3])]),
        ]));
        // term 1: b AND NOT a -> {3}
        // term 2: b AND a     -> {1, 2}
        // OR -> {1, 2, 3}
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"b"), exclude(b"a")]).unwrap(),
            BitmapTerm::new(vec![include(b"b"), include(b"a")]).unwrap(),
        ])
        .unwrap();

        let stream = eval_bitmap_query_stream(
            source.clone(),
            query,
            0..100_000,
            BUCKET_SIZE,
            ScanDirection::Ascending,
            u64::MAX,
            |_| {},
        );
        let (items, _watermarks) = drain_marked(stream).await.unwrap();

        assert_eq!(items, vec![1, 2, 3]);
        assert_eq!(source.scan_count(&test_key(b"a")), 1);
        assert_eq!(source.scan_count(&test_key(b"b")), 1);
    }

    #[tokio::test]
    async fn eval_bitmap_query_stream_uses_backend_source() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (test_key(b"a"), vec![(0, vec![1, 2, 3]), (1, vec![5])]),
                (test_key(b"b"), vec![(0, vec![2, 3]), (1, vec![5])]),
                (test_key(b"c"), vec![(0, vec![3])]),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b"), exclude(b"c")]).unwrap(),
        ])
        .unwrap();

        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..200_000,
            BUCKET_SIZE,
            ScanDirection::Ascending,
            u64::MAX,
            |_| {},
        );
        let (items, _watermarks) = drain_marked(stream).await.unwrap();

        assert_eq!(items, vec![2, BUCKET_SIZE + 5]);
    }

    #[tokio::test]
    async fn eval_bitmap_query_stream_descending() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (test_key(b"a"), vec![(0, vec![1, 2, 3]), (1, vec![5])]),
                (test_key(b"b"), vec![(0, vec![2, 3]), (1, vec![5])]),
                (test_key(b"c"), vec![(0, vec![3])]),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b"), exclude(b"c")]).unwrap(),
        ])
        .unwrap();

        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..200_000,
            BUCKET_SIZE,
            ScanDirection::Descending,
            u64::MAX,
            |_| {},
        );
        let (items, _watermarks) = drain_marked(stream).await.unwrap();

        assert_eq!(items, vec![BUCKET_SIZE + 5, 2]);
    }

    /// End-to-end: `buckets_with_watermarks` injects watermarks, then
    /// `flatten_watermarked_buckets` flattens items and passes watermarks through.
    /// Verifies edge trimming + marker interleaving in one composed test.
    #[tokio::test]
    async fn buckets_with_watermarks_then_flatten_watermarked_buckets_ascending() {
        let range = 50u64..(2 * BUCKET_SIZE + 50_001);
        let items = stream::iter(vec![
            // bucket 0: bit 10 trimmed (< 50); 50 and bucket_size-1 kept.
            Ok((0u64, {
                let mut bm = RoaringBitmap::new();
                bm.insert(10);
                bm.insert(50);
                bm.insert((BUCKET_SIZE - 1) as u32);
                bm
            })),
            // bucket 1: middle, full pass-through.
            Ok((1u64, {
                let mut bm = RoaringBitmap::new();
                bm.insert(0);
                bm.insert((BUCKET_SIZE - 1) as u32);
                bm
            })),
            // bucket 2: bit 50_001 trimmed (>= hi=50_001 relative).
            Ok((2u64, {
                let mut bm = RoaringBitmap::new();
                bm.insert(0);
                bm.insert(50_000);
                bm.insert(50_001);
                bm
            })),
        ]);
        let marked_buckets =
            buckets_with_watermarks(items, range.clone(), BUCKET_SIZE, ScanDirection::Ascending);
        let out: Vec<Watermarked<u64>> = flatten_watermarked_buckets(
            marked_buckets,
            range,
            BUCKET_SIZE,
            ScanDirection::Ascending,
        )
        .try_collect()
        .await
        .unwrap();
        // Items are interleaved with watermarks at each bucket boundary.
        // Watermark(p) is emitted AFTER the bucket's items so its arrival proves
        // those items also passed.
        assert_eq!(
            out,
            vec![
                Watermarked::Item(50),
                Watermarked::Item(BUCKET_SIZE - 1),
                Watermarked::Watermark(BUCKET_SIZE),
                Watermarked::Item(BUCKET_SIZE),
                Watermarked::Item(2 * BUCKET_SIZE - 1),
                Watermarked::Watermark(2 * BUCKET_SIZE),
                Watermarked::Item(2 * BUCKET_SIZE),
                Watermarked::Item(2 * BUCKET_SIZE + 50_000),
                // Edge bucket watermark is clamped to range.end so cursors
                // don't claim progress past the requested upper bound.
                Watermarked::Watermark(2 * BUCKET_SIZE + 50_001),
            ],
        );
    }

    /// `buckets_with_watermarks` standalone: verify each bucket gets its own
    /// `Watermarked::Watermark` immediately after, with no flattening / trimming.
    /// This is the variant the rocksdb branch consumes directly.
    #[tokio::test]
    async fn buckets_with_watermarks_one_per_bucket_no_flatten() {
        let items = stream::iter(vec![
            Ok((0u64, {
                let mut bm = RoaringBitmap::new();
                bm.insert(1);
                bm.insert(2);
                bm
            })),
            Ok((3u64, {
                let mut bm = RoaringBitmap::new();
                bm.insert(5);
                bm
            })),
        ]);
        // Range extends past the last populated bucket (bucket 3 = positions
        // [3*BUCKET_SIZE, 4*BUCKET_SIZE)); the final natural-EOF watermark
        // caps at the request range boundary so resume cursors don't leave
        // the empty tail (4*BUCKET_SIZE..5*BUCKET_SIZE) un-acknowledged.
        let out: Vec<Watermarked<(u64, Vec<u32>)>> = buckets_with_watermarks(
            items,
            0..(5 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
        )
        .map_ok(|m| m.map_item(|(bid, bm)| (bid, bm.iter().collect::<Vec<_>>())))
        .try_collect()
        .await
        .unwrap();
        assert_eq!(
            out,
            vec![
                Watermarked::Item((0, vec![1, 2])),
                Watermarked::Watermark(BUCKET_SIZE),
                Watermarked::Item((3, vec![5])),
                Watermarked::Watermark(4 * BUCKET_SIZE),
                Watermarked::Watermark(5 * BUCKET_SIZE),
            ],
        );
    }

    #[tokio::test]
    async fn scan_budget_below_unique_leaf_count_yields_misconfig_error() {
        // Defensive runtime guard: a per-request budget smaller than the
        // query's leaf count would produce a cursorless SCAN_LIMIT
        // (merged watermarks stay None until every child reports). The
        // eval surfaces this as a plain anyhow error — distinct from
        // BitmapScanLimitExceeded — so the handler propagates it as
        // Internal rather than SCAN_LIMIT.
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([(
                test_key(b"a"),
                vec![(0, vec![1, 2]), (1, vec![3])],
            )])),
        };
        let query = BitmapQuery::new(vec![BitmapTerm::new(vec![include(b"a")]).unwrap()]).unwrap();

        let metrics = std::sync::Arc::new(std::sync::Mutex::new(None));
        let metrics_sink = metrics.clone();
        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..200_000,
            BUCKET_SIZE,
            ScanDirection::Ascending,
            0,
            move |m| *metrics_sink.lock().unwrap() = Some(m),
        );
        let err = drain_marked(stream).await.unwrap_err();

        assert!(
            error_contains::<BitmapScanLimitExceeded>(&err).is_none(),
            "must NOT surface as BitmapScanLimitExceeded; would be cursorless SCAN_LIMIT"
        );
        assert!(
            err.to_string().contains("insufficient for"),
            "expected misconfig error, got {err:?}"
        );
        // The misconfig guard short-circuits before any scan setup, so
        // no scan metrics are emitted (the callback is dropped uncalled).
        assert!(
            metrics.lock().unwrap().is_none(),
            "misconfig early-out should not emit scan metrics"
        );
    }

    #[tokio::test]
    async fn scan_budget_shared_across_dimensions() {
        // Three include dimensions with several buckets each. Budget = 4
        // should be consumed across all per-dimension fetches before
        // ScanLimitExceeded surfaces from the merged eval stream.
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (
                    test_key(b"a"),
                    vec![(0, vec![1]), (1, vec![2]), (2, vec![3])],
                ),
                (
                    test_key(b"b"),
                    vec![(0, vec![1]), (1, vec![2]), (2, vec![3])],
                ),
                (
                    test_key(b"c"),
                    vec![(0, vec![1]), (1, vec![2]), (2, vec![3])],
                ),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b"), include(b"c")]).unwrap(),
        ])
        .unwrap();

        let metrics = std::sync::Arc::new(std::sync::Mutex::new(None));
        let metrics_sink = metrics.clone();
        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..300_000,
            BUCKET_SIZE,
            ScanDirection::Ascending,
            4,
            move |m| *metrics_sink.lock().unwrap() = Some(m),
        );
        let err = drain_marked(stream).await.unwrap_err();

        assert!(
            error_contains::<BitmapScanLimitExceeded>(&err).is_some(),
            "expected BitmapScanLimitExceeded, got {err:?}"
        );
        // All four buckets were evaluated through budgeted_bucket_stream
        // before the fifth try_take() failed and surfaced BitmapScanLimitExceeded.
        assert_eq!(metrics.lock().unwrap().unwrap().buckets_evaluated, 4);
    }

    /// Budget exhausting on an exclude leaf must NOT be mistaken for the
    /// exclude reaching its range terminus. With silent EOF semantics, includes
    /// past the exclude cutoff would leak unfiltered. With `ScanLimitExceeded`,
    /// the error propagates and the eval pipeline short-circuits cleanly.
    #[tokio::test]
    async fn scan_budget_exclude_side_exhaustion_does_not_leak_includes() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (
                    test_key(b"inc"),
                    vec![(0, vec![1]), (1, vec![2]), (2, vec![3])],
                ),
                (
                    test_key(b"exc"),
                    vec![(0, vec![1]), (1, vec![2]), (2, vec![3])],
                ),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"inc"), exclude(b"exc")]).unwrap(),
        ])
        .unwrap();

        // Budget = leaf count gives every leaf one bucket fetch (the
        // minimum the runtime guard allows; see
        // `scan_budget_below_unique_leaf_count_yields_misconfig_error`). Once
        // the budget exhausts mid-scan, ScanLimitExceeded propagates
        // without the driver mistaking the exclude leaf's error for a
        // natural EOF.
        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..300_000,
            BUCKET_SIZE,
            ScanDirection::Ascending,
            2,
            |_| {},
        );
        let result = drain_marked(stream).await;

        // Must error, not return Ok with leaked include rows.
        let err = result.expect_err("must surface scan-limit, not silently emit includes");
        assert!(
            error_contains::<BitmapScanLimitExceeded>(&err).is_some(),
            "expected BitmapScanLimitExceeded, got {err:?}"
        );
    }

    /// Disjoint-intersect: the leaves advance but the term matches nothing, so
    /// no item is ever emitted. The driver's floor watermark must still surface
    /// real progress before the budget error; otherwise handlers fall back to
    /// the request lower bound and the client livelocks on retry.
    #[tokio::test]
    async fn sparse_intersect_emits_frontier_watermark_before_scan_limit() {
        // include "a" at buckets [0, 1, 2, ...], include "b" at bucket 100 —
        // disjoint, so the driver advances the floor through a's buckets one by
        // one and emits zero output. Budget=4 forces error mid-scan.
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (
                    test_key(b"a"),
                    vec![
                        (0, vec![1]),
                        (1, vec![1]),
                        (2, vec![1]),
                        (3, vec![1]),
                        (4, vec![1]),
                    ],
                ),
                (test_key(b"b"), vec![(100, vec![1])]),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b")]).unwrap(),
        ])
        .unwrap();

        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..(110 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            4,
            |_| {},
        );
        // Don't try_collect — short-circuiting on Err would drop the
        // pre-error watermark we're verifying.
        let all: Vec<Result<Watermarked<u64>>> = stream.collect().await;

        let last_ok = all
            .iter()
            .rev()
            .find_map(|r| r.as_ref().ok())
            .expect("expected a watermark item before the error");
        match last_ok {
            Watermarked::Watermark(p) => {
                assert!(
                    *p > 0,
                    "frontier watermark must reflect real progress (got {p})"
                );
            }
            Watermarked::Item(_) => panic!("disjoint intersect must not emit items"),
        }
        let err = all
            .last()
            .expect("non-empty")
            .as_ref()
            .expect_err("scan must terminate with an error");
        assert!(
            error_contains::<BitmapScanLimitExceeded>(err).is_some(),
            "expected BitmapScanLimitExceeded, got {err:?}"
        );
    }

    #[tokio::test]
    async fn eval_emits_watermarks_at_bucket_boundaries_ascending() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([(
                test_key(b"a"),
                vec![(0, vec![1]), (3, vec![2]), (7, vec![3])],
            )])),
        };
        let query = BitmapQuery::new(vec![BitmapTerm::new(vec![include(b"a")]).unwrap()]).unwrap();

        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..(8 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            u64::MAX,
            |_| {},
        );
        let (items, watermarks) = drain_marked(stream).await.unwrap();

        // Items at the bits within each of the three populated buckets.
        assert_eq!(items, vec![1, 3 * BUCKET_SIZE + 2, 7 * BUCKET_SIZE + 3]);
        // The flat driver emits the floor bucket's leading edge (pre) and
        // trailing edge (post) each round, so each populated bucket [0, 3, 7]
        // contributes both: pre/post = (0, bs), (3bs, 4bs), (7bs, 8bs). The
        // final post(7)=8bs is the range terminus.
        assert_eq!(
            watermarks,
            vec![
                0,
                BUCKET_SIZE,
                3 * BUCKET_SIZE,
                4 * BUCKET_SIZE,
                7 * BUCKET_SIZE,
                8 * BUCKET_SIZE,
            ]
        );
    }

    #[tokio::test]
    async fn eval_emits_watermarks_at_bucket_boundaries_descending() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([(
                test_key(b"a"),
                vec![(0, vec![1]), (3, vec![2]), (7, vec![3])],
            )])),
        };
        let query = BitmapQuery::new(vec![BitmapTerm::new(vec![include(b"a")]).unwrap()]).unwrap();

        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..(8 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Descending,
            u64::MAX,
            |_| {},
        );
        let (items, watermarks) = drain_marked(stream).await.unwrap();

        assert_eq!(items, vec![7 * BUCKET_SIZE + 3, 3 * BUCKET_SIZE + 2, 1]);
        // Descending pre/post per matched bucket [7, 3, 0]: pre is the high
        // edge, post the low edge — (8bs, 7bs), (4bs, 3bs), (1bs, 0). pre(7)=8bs
        // is range.end; post(0)=0 is the range terminus.
        assert_eq!(
            watermarks,
            vec![
                8 * BUCKET_SIZE,
                7 * BUCKET_SIZE,
                4 * BUCKET_SIZE,
                3 * BUCKET_SIZE,
                BUCKET_SIZE,
                0,
            ]
        );
    }

    #[tokio::test]
    async fn eval_emits_per_source_watermarks_and_final_eof_when_no_bucket_yielded() {
        // Two include dimensions whose buckets never align -> the term
        // yields no Items. The floor watermark (pre+post per bucket) still
        // propagates the actual scan progress, and the driver caps the stream
        // with a final range_end watermark on natural EOF so clients see "scan
        // covered the range with no matches."
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (
                    test_key(b"a"),
                    vec![(0, vec![1]), (2, vec![3]), (4, vec![5])],
                ),
                (
                    test_key(b"b"),
                    vec![(1, vec![1]), (3, vec![3]), (5, vec![5])],
                ),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b")]).unwrap(),
        ])
        .unwrap();

        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..(6 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            u64::MAX,
            |_| {},
        );
        let (items, watermarks) = drain_marked(stream).await.unwrap();

        assert!(items.is_empty(), "disjoint intersect must not emit items");
        // Watermarks reflect real per-source progress as intersect drops
        // misaligned buckets, then the eval root adds the final range_end.
        assert!(
            !watermarks.is_empty(),
            "expected per-source watermarks to propagate, got none"
        );
        let mut prev = 0u64;
        for w in &watermarks {
            assert!(
                *w >= prev,
                "ascending watermarks must be monotonic, got {watermarks:?}"
            );
            assert!(
                *w <= 6 * BUCKET_SIZE,
                "watermark exceeds range.end ({watermarks:?})"
            );
            prev = *w;
        }
        assert_eq!(
            *watermarks.last().unwrap(),
            6 * BUCKET_SIZE,
            "final watermark must be range.end on natural EOF"
        );
    }

    #[tokio::test]
    async fn eval_watermark_ordering_invariant_item_then_watermark() {
        // Critical invariant: for each bucket, all Items come BEFORE the
        // post-bucket watermark. This is what makes the watermark safe as
        // a resume cursor — its arrival downstream proves the dominated
        // items also arrived in the same stream order.
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([(
                test_key(b"a"),
                vec![(0, vec![10, 20, 30]), (1, vec![40, 50])],
            )])),
        };
        let query = BitmapQuery::new(vec![BitmapTerm::new(vec![include(b"a")]).unwrap()]).unwrap();

        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..(2 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            u64::MAX,
            |_| {},
        );
        let all: Vec<Watermarked<u64>> = stream.try_collect().await.unwrap();

        // Per-source pre+post watermarks: each bucket emits Watermark(pre),
        // Item(s)…, Watermark(post). pre(0)=0, post(0)=BUCKET_SIZE,
        // pre(1)=BUCKET_SIZE (dup of post(0), filtered), post(1)=2*BUCKET_SIZE,
        // EOF=2*BUCKET_SIZE (dup, filtered).
        assert_eq!(
            all,
            vec![
                Watermarked::Watermark(0),
                Watermarked::Item(10),
                Watermarked::Item(20),
                Watermarked::Item(30),
                Watermarked::Watermark(BUCKET_SIZE),
                Watermarked::Item(BUCKET_SIZE + 40),
                Watermarked::Item(BUCKET_SIZE + 50),
                Watermarked::Watermark(2 * BUCKET_SIZE),
            ],
        );
    }

    #[tokio::test]
    async fn buckets_with_watermarks_then_flatten_watermarked_buckets_descending() {
        let range = 50u64..(2 * BUCKET_SIZE + 50_001);
        let items = stream::iter(vec![
            Ok((2u64, {
                let mut bm = RoaringBitmap::new();
                bm.insert(0);
                bm.insert(50_000);
                bm.insert(50_001);
                bm
            })),
            Ok((1u64, {
                let mut bm = RoaringBitmap::new();
                bm.insert(0);
                bm.insert((BUCKET_SIZE - 1) as u32);
                bm
            })),
            Ok((0u64, {
                let mut bm = RoaringBitmap::new();
                bm.insert(10);
                bm.insert(50);
                bm.insert((BUCKET_SIZE - 1) as u32);
                bm
            })),
        ]);
        let marked_buckets =
            buckets_with_watermarks(items, range.clone(), BUCKET_SIZE, ScanDirection::Descending);
        let out: Vec<Watermarked<u64>> = flatten_watermarked_buckets(
            marked_buckets,
            range,
            BUCKET_SIZE,
            ScanDirection::Descending,
        )
        .try_collect()
        .await
        .unwrap();
        // Descending: watermark(p) = "all items >= p have been emitted." After
        // bucket 2 yields, frontier = 2 * BUCKET_SIZE (bucket 2's low edge).
        assert_eq!(
            out,
            vec![
                Watermarked::Item(2 * BUCKET_SIZE + 50_000),
                Watermarked::Item(2 * BUCKET_SIZE),
                Watermarked::Watermark(2 * BUCKET_SIZE),
                Watermarked::Item(2 * BUCKET_SIZE - 1),
                Watermarked::Item(BUCKET_SIZE),
                Watermarked::Watermark(BUCKET_SIZE),
                Watermarked::Item(BUCKET_SIZE - 1),
                Watermarked::Item(50),
                // Edge bucket watermark is clamped to range.start so cursors
                // don't claim progress past the requested lower bound (in
                // descending: lower position is "further past").
                Watermarked::Watermark(50),
            ],
        );
    }

    /// Single-error `collapse` returns the inner error directly so the
    /// common case preserves `downcast_ref` on the concrete error type.
    #[test]
    fn multi_error_collapses_single() {
        let err = MultiError::collapse(vec![anyhow::Error::new(BitmapScanLimitExceeded)]);
        assert!(err.downcast_ref::<MultiError>().is_none());
        assert!(err.downcast_ref::<BitmapScanLimitExceeded>().is_some());
        assert!(error_contains::<BitmapScanLimitExceeded>(&err).is_some());
    }

    /// Multi-error `collapse` wraps; `error_contains` looks through the
    /// aggregate to find the requested error type from any sibling.
    #[test]
    fn multi_error_error_contains_finds_through_aggregate() {
        let err = MultiError::collapse(vec![
            anyhow::anyhow!("first transport"),
            anyhow::Error::new(BitmapScanLimitExceeded),
        ]);
        assert!(err.downcast_ref::<MultiError>().is_some());
        // Without `error_contains` you'd miss it — top-level isn't BSLE.
        assert!(err.downcast_ref::<BitmapScanLimitExceeded>().is_none());
        assert!(error_contains::<BitmapScanLimitExceeded>(&err).is_some());
    }

    /// Display includes each inner error so logs are useful when the
    /// aggregate hits the top level.
    #[test]
    fn multi_error_display_lists_inner() {
        let err = MultiError::collapse(vec![anyhow::anyhow!("alpha"), anyhow::anyhow!("beta")]);
        let s = err.to_string();
        assert!(s.contains("alpha"));
        assert!(s.contains("beta"));
    }
}
