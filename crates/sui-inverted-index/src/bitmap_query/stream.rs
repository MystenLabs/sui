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

use futures::Stream;
use futures::StreamExt;
use futures::stream::BoxStream;
use futures::stream::Peekable;
use mysten_common::zip_debug_eq::ZipDebugEqIteratorExt;
use roaring::RoaringBitmap;

use super::BitmapBucketSource;
use super::BitmapQuery;
use super::BucketItem;
use super::BucketStream;
use super::DedupedQuery;
use super::LeafHead;
use super::LeafStop;
use super::ScanDirection;
use super::ScanStop;
use super::SkipPolicy;
use super::Watermarked;
use super::WatermarkedBucketStream;
use super::advance_in_direction;
use super::bound_in_direction;
use super::bucket_edges;
use super::build_term_specs;
use super::collapse;
use super::count_on_floor_refs;
use super::eval_term_at_bucket;
use super::frontier_advanced;
use super::leaf_skip_targets;
use super::recompute_unreferenced;
use super::strictly_before;
use super::take_snapshot_bitmap;

/// Per-request bucket-scan accounting, delivered via the `on_metrics`
/// callback passed to `eval_bitmap_query_stream`. Fires once when the
/// eval pipeline is dropped (natural end, error, or consumer cancel).
/// The sole exception is the budget-misconfig early-out, which errors
/// before any scan is set up and emits nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BitmapScanMetrics {
    /// Bucket rows charged against the budget, including dead rows drained
    /// during gap catch-up.
    pub buckets_evaluated: u64,
    /// Charged dead bucket rows discarded while catching up lagging leaves.
    pub buckets_discarded: u64,
    /// Physical leaf scans abandoned and reopened at a later bucket.
    pub leaf_seeks: u64,
}

/// Per-request evaluated-bucket budget shared across all dimension
/// streams of one eval. Charges are post-poll — see
/// `budgeted_bucket_stream`.
#[derive(Clone)]
pub(crate) struct BitmapScanBudget {
    initial: u64,
    remaining: Arc<AtomicU64>,
    discarded: Arc<AtomicU64>,
    seeks: Arc<AtomicU64>,
}

impl BitmapScanBudget {
    pub(crate) fn new(initial: u64) -> Self {
        Self {
            initial,
            remaining: Arc::new(AtomicU64::new(initial)),
            discarded: Arc::new(AtomicU64::new(0)),
            seeks: Arc::new(AtomicU64::new(0)),
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
    fn note_discarded(&self) {
        self.discarded.fetch_add(1, Ordering::Relaxed);
    }

    fn note_seek(&self) {
        self.seeks.fetch_add(1, Ordering::Relaxed);
    }

    fn discarded(&self) -> u64 {
        self.discarded.load(Ordering::Relaxed)
    }

    fn seeks(&self) -> u64 {
        self.seeks.load(Ordering::Relaxed)
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
                buckets_discarded: self.budget.discarded(),
                leaf_seeks: self.budget.seeks(),
            });
        }
    }
}

/// Evaluate a DNF `BitmapQuery` against a backend-provided bitmap source.
///
/// `budget` caps evaluated buckets across all dimension scans (see scan-limit
/// handling and [`BitmapScanMetrics`]). `on_metrics`
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
    policy: SkipPolicy,
    on_metrics: F,
) -> BoxStream<'static, Result<Watermarked<u64>, ScanStop>>
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
            // Misconfiguration is a genuine fault, not a `ScanLimit` stop.
            yield Err(ScanStop::Fault(anyhow::anyhow!(
                "bitmap scan budget {budget} is insufficient for {leaves} leaf streams; \
                 server is misconfigured"
            )));
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
        policy,
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
    policy: SkipPolicy,
) -> WatermarkedBucketStream
where
    S: BitmapBucketSource,
{
    let DedupedQuery {
        keys: unique_keys,
        mut terms,
    } = build_term_specs(query.terms);
    let leaf_count = unique_keys.len();
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
        let mut leaves: Vec<Peekable<BucketStream>> = Vec::with_capacity(leaf_count);
        for key in &unique_keys {
            let raw = source.scan_bucket_stream(key.clone(), range.clone(), direction);
            leaves.push(
                budgeted_bucket_stream(raw, budget.clone(), true)
                    .boxed()
                    .peekable(),
            );
        }
        let mut unreferenced = vec![false; leaf_count];
        let mut front = vec![request_floor; leaf_count];
        let mut drained = vec![0u64; leaf_count];
        let mut progress_frontier = Some(request_floor);

        loop {
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
                if let Some(position) = scanned_to {
                    front[i] = advance_in_direction(front[i], position, direction);
                }
                class[i] = Some(head);
            }

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
            recompute_unreferenced(&terms, &class, &mut unreferenced);

            let targets = leaf_skip_targets(&terms, &class, &unreferenced, direction);
            for (i, target) in targets.iter().enumerate() {
                if let Some(target) = target {
                    let (pre, _) = bucket_edges(*target, bucket_size, &range, direction);
                    front[i] = advance_in_direction(front[i], pre, direction);
                }
            }

            let mut errors: Vec<LeafStop> = Vec::new();
            for i in 0..leaf_count {
                if !unreferenced[i] && matches!(class[i], Some(LeafHead::Error)) {
                    match Pin::new(&mut leaves[i]).next().await {
                        Some(Err(error)) => errors.push(error),
                        _ => unreachable!("peek classified Error"),
                    }
                }
            }

            let active: Vec<usize> = (0..leaf_count).filter(|&i| !unreferenced[i]).collect();
            if active.is_empty() {
                return;
            }

            let floor_pos = active
                .iter()
                .map(|&i| front[i])
                .reduce(|a, b| bound_in_direction(a, b, direction))
                .expect("active non-empty");
            let collapsed = (!errors.is_empty()).then(|| collapse(errors, floor_pos));
            let scan_limited = matches!(collapsed, Some(ScanStop::ScanLimit { .. }));
            if !scan_limited && frontier_advanced(progress_frontier, floor_pos, direction) {
                yield Watermarked::Watermark(floor_pos);
                progress_frontier = Some(floor_pos);
            }
            if let Some(stop) = collapsed {
                Err(stop)?;
            }

            // A target marks a leaf whose logical frontier is ahead of its
            // physical stream. Its rows before the target are proven dead, but
            // the stream still needs to drain or seek across them.
            let lagging: Vec<usize> = active
                .iter()
                .copied()
                .filter(|&i| targets[i].is_some())
                .collect();
            // A leaf gets a fresh drain probe after it catches up. Keeping the
            // count while it remains lagging prevents a moving target from
            // repeatedly restarting the probe.
            for i in 0..leaf_count {
                if targets[i].is_none() {
                    drained[i] = 0;
                }
            }

            // Lagging heads are intentionally excluded here. Find the next
            // physical bucket among leaves that are ready to participate in
            // evaluation.
            let eval_bucket = active
                .iter()
                .filter(|&&i| targets[i].is_none())
                .filter_map(|&i| match class[i] {
                    Some(LeafHead::Bucket(bucket)) => Some(bucket),
                    _ => None,
                })
                .reduce(|a, b| bound_in_direction(a, b, direction))
                .expect("at least one active leaf has a ready bucket");
            // Ready leaves may be evaluated before the nearest lagging target:
            // any term that needs a lagging leaf is known to produce nothing
            // there. Equality must wait so a leaf needed at the target is not
            // omitted from that bucket's snapshot.
            let lagging_target = lagging
                .iter()
                .filter_map(|&i| targets[i])
                .reduce(|a, b| bound_in_direction(a, b, direction));
            let evaluate = lagging_target
                .is_none_or(|target| strictly_before(eval_bucket, target, direction));

            if evaluate {
                let (_, post) = bucket_edges(eval_bucket, bucket_size, &range, direction);
                // Consume only ready leaves positioned at this bucket. Lagging
                // leaves stay out of the snapshot until catch-up is observed in
                // a later outer round.
                let mut snapshot: Vec<Option<RoaringBitmap>> =
                    (0..leaf_count).map(|_| None).collect();
                let mut on_floor = vec![false; leaf_count];
                for i in 0..leaf_count {
                    if !unreferenced[i]
                        && targets[i].is_none()
                        && matches!(class[i], Some(LeafHead::Bucket(b)) if b == eval_bucket)
                    {
                        on_floor[i] = true;
                        front[i] = advance_in_direction(front[i], post, direction);
                        snapshot[i] = match Pin::new(&mut leaves[i]).next().await {
                            Some(Ok((_, bitmap))) => Some(bitmap),
                            _ => None,
                        };
                    }
                }
                // Evaluate each conjunction from the shared leaf snapshot, then
                // union the non-empty bitmaps to implement the top-level OR.
                let mut remaining_refs = count_on_floor_refs(&terms, &on_floor);
                let mut result: Option<RoaringBitmap> = None;
                for term in &terms {
                    if term.unsatisfiable {
                        continue;
                    }
                    let includes = term
                        .includes
                        .iter()
                        .map(|&i| {
                            take_snapshot_bitmap(
                                &mut snapshot,
                                &mut remaining_refs,
                                &on_floor,
                                i,
                            )
                        })
                        .collect();
                    let excludes = term
                        .excludes
                        .iter()
                        .map(|&i| {
                            take_snapshot_bitmap(
                                &mut snapshot,
                                &mut remaining_refs,
                                &on_floor,
                                i,
                            )
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
                    yield Watermarked::Item((eval_bucket, bitmap));
                }
                if frontier_advanced(progress_frontier, post, direction) {
                    yield Watermarked::Watermark(post);
                    progress_frontier = Some(post);
                }
            }

            // Physically catch up all lagging leaves concurrently. The
            // evaluator waits for every catch-up future before starting the
            // next classification round.
            let catch_up = leaves.iter_mut().zip_debug_eq(drained.iter_mut())
                .enumerate()
                .filter_map(|(i, (leaf, drained))| {
                    let target = targets[i]?;
                    let source = source.clone();
                    let key = unique_keys[i].clone();
                    let range = range.clone();
                    let budget = budget.clone();
                    Some(async move {
                        // Stop on the landing row, an error, or EOF. Leaving it
                        // buffered lets the next outer round classify it using
                        // fresh term and target state.
                        loop {
                            let dead_row = matches!(
                                Pin::new(&mut *leaf).peek().await,
                                Some(Ok((bucket, _)))
                                    if strictly_before(*bucket, target, direction)
                            );
                            if !dead_row {
                                break;
                            }
                            // Drain short gaps from the open scan. Once the
                            // probe is exhausted, abandon the scan and reopen at
                            // the target instead.
                            if policy
                                .drain_probe_rows
                                .is_some_and(|probe| *drained >= u64::from(probe.get()))
                            {
                                // The dead row in the peek slot was already
                                // pulled through the budgeted stream. Replacing
                                // the stream discards it without calling next().
                                budget.note_discarded();
                                // Narrow the replacement range so it includes
                                // the target bucket and excludes the dead gap.
                                let narrowed = match direction {
                                    ScanDirection::Ascending => {
                                        target.saturating_mul(bucket_size).max(range.start)
                                            ..range.end
                                    }
                                    ScanDirection::Descending => {
                                        range.start
                                            ..target
                                                .saturating_add(1)
                                                .saturating_mul(bucket_size)
                                                .min(range.end)
                                    }
                                };
                                let raw = source.scan_bucket_stream(key, narrowed, direction);
                                // This is still the same logical leaf, so the
                                // reopened stream gets no second mandatory
                                // first-row reservation.
                                *leaf = budgeted_bucket_stream(raw, budget.clone(), false)
                                    .boxed()
                                    .peekable();
                                budget.note_seek();
                                break;
                            }
                            // Consuming a dead row keeps using the existing scan
                            // and counts both the budgeted read and the discard.
                            let discarded = Pin::new(&mut *leaf).next().await;
                            debug_assert!(matches!(discarded, Some(Ok(_))));
                            *drained += 1;
                            budget.note_discarded();
                        }
                    })
                });
            futures::future::join_all(catch_up).await;
        }
    }
    .boxed()
}

/// Wrap a raw per-dimension bucket stream with the shared scan budget: charge
/// one bucket per pull (the first via `take_first`, the rest via `try_take`),
/// yielding [`LeafStop::BudgetExhausted`] when the pool is empty — never a
/// silent EOF.
fn budgeted_bucket_stream<S>(
    inner: S,
    budget: BitmapScanBudget,
    reserve_first: bool,
) -> impl Stream<Item = BucketItem> + Send + 'static
where
    S: Stream<Item = BucketItem> + Send + 'static,
{
    async_stream::try_stream! {
        futures::pin_mut!(inner);
        let mut first = reserve_first;
        while let Some(item) = inner.next().await {
            let item = item?;
            if first {
                budget.take_first();
                first = false;
            } else if !budget.try_take() {
                Err(LeafStop::BudgetExhausted)?;
            }
            yield item;
        }
    }
}

/// Flatten marked bucket bitmaps into absolute member ids with
/// edge-bucket trimming against `range`. Watermarks pass through
/// unchanged.
pub fn flatten_watermarked_buckets<S, E>(
    stream: S,
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
) -> impl Stream<Item = Result<Watermarked<u64>, E>> + Send + 'static
where
    S: Stream<Item = Result<Watermarked<(u64, RoaringBitmap)>, E>> + Send + 'static,
    E: Send + 'static,
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
            range: Range<u64>,
            direction: ScanDirection,
        ) -> BucketStream {
            let mut buckets = self
                .buckets
                .get(&dimension_key)
                .cloned()
                .unwrap_or_default();
            if range.is_empty() {
                buckets.clear();
            } else {
                let first_bucket = range.start / BUCKET_SIZE;
                let last_bucket = (range.end - 1) / BUCKET_SIZE;
                buckets.retain(|(bucket, _)| first_bucket <= *bucket && *bucket <= last_bucket);
            }
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
        stream: BoxStream<'static, Result<Watermarked<u64>, ScanStop>>,
    ) -> Result<(Vec<u64>, Vec<u64>), ScanStop> {
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

    /// Tightest-budget starvation. `term1 = (a AND b)` cannot match before
    /// bucket 50, while independent `term2 = c` matches at bucket 40. The
    /// logical jump lets bucket 40 evaluate before catch-up exhausts the shared
    /// budget; the stopping round carries bucket 50's leading edge only in the
    /// terminal frontier.
    #[tokio::test]
    async fn nested_term_starvation_bundles_frontier_in_scan_limit() {
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
            SkipPolicy::DRAIN_ONLY,
            |_| {},
        );
        let all: Vec<Result<Watermarked<u64>, ScanStop>> = stream.collect().await;

        let items: Vec<u64> = all
            .iter()
            .filter_map(|result| match result {
                Ok(Watermarked::Item(item)) => Some(*item),
                _ => None,
            })
            .collect();
        assert_eq!(items, vec![40 * BUCKET_SIZE + 7]);

        let watermarks: Vec<u64> = all
            .iter()
            .filter_map(|r| match r {
                Ok(Watermarked::Watermark(position)) => Some(*position),
                _ => None,
            })
            .collect();
        assert_eq!(watermarks, vec![40 * BUCKET_SIZE, 41 * BUCKET_SIZE]);
        assert_eq!(
            all.len(),
            items.len() + watermarks.len() + 1,
            "the stopping round must add only its terminal, not another beacon"
        );
        let err = all
            .last()
            .expect("non-empty")
            .as_ref()
            .expect_err("scan must terminate with an error");
        match err {
            ScanStop::ScanLimit { scan_frontier } => assert_eq!(
                *scan_frontier,
                50 * BUCKET_SIZE,
                "terminal must carry the logically advanced merged floor"
            ),
            other => panic!("expected ScanLimit, got {other:?}"),
        }
    }

    /// Flush-on-error: a budget error truncates the scan at the floor, but
    /// matches at or below the floor were already emitted in earlier rounds.
    /// Here `c` matches in the FIRST bucket — below where `(a AND b)` exhausts
    /// the budget — so its item must be DELIVERED, and the resume cursor must
    /// advance to that death floor rather than be pinned at the request floor
    /// (the livelock this guards against). The delivered item stays below the
    /// final watermark, so resuming from that cursor will not re-emit it.
    ///
    /// The logical frontier for `a` advances to bucket 50 before catch-up. The
    /// independent bucket-0 match remains below that valid resume edge.
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
            SkipPolicy::DRAIN_ONLY,
            |_| {},
        );
        let all: Vec<Result<Watermarked<u64>, ScanStop>> = stream.collect().await;

        // c's bucket-0 match (member id 7) is delivered despite term1 dying.
        let items: Vec<u64> = all
            .iter()
            .filter_map(|r| match r {
                Ok(Watermarked::Item(v)) => Some(*v),
                _ => None,
            })
            .collect();
        assert_eq!(items, vec![7], "c's below-floor match must be delivered");

        let watermarks: Vec<u64> = all
            .iter()
            .filter_map(|r| match r {
                Ok(Watermarked::Watermark(position)) => Some(*position),
                _ => None,
            })
            .collect();
        assert_eq!(watermarks, vec![BUCKET_SIZE]);
        assert_eq!(
            all.len(),
            items.len() + watermarks.len() + 1,
            "the stopping round must add only its terminal, not another beacon"
        );

        let err = all
            .last()
            .expect("non-empty")
            .as_ref()
            .expect_err("scan must terminate with an error");
        let scan_frontier = match err {
            ScanStop::ScanLimit { scan_frontier } => *scan_frontier,
            other => panic!("expected ScanLimit, got {other:?}"),
        };
        assert_eq!(
            scan_frontier,
            50 * BUCKET_SIZE,
            "terminal frontier must include the logical gap jump"
        );
        assert!(
            items.iter().all(|&i| i < scan_frontier),
            "delivered items must be below the resume frontier"
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
            SkipPolicy::DRAIN_ONLY,
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
            SkipPolicy::DRAIN_ONLY,
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
            SkipPolicy::DRAIN_ONLY,
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
            SkipPolicy::DRAIN_ONLY,
            |_| {},
        );
        let (items, _watermarks) = drain_marked(stream).await.unwrap();

        assert_eq!(items, vec![BUCKET_SIZE + 5, 2]);
    }

    /// Flattening trims edge buckets while preserving already-clamped
    /// evaluator watermarks and their ordering among items.
    #[tokio::test]
    async fn flatten_watermarked_buckets_ascending() {
        let range = 50u64..(2 * BUCKET_SIZE + 50_001);
        let marked_buckets = stream::iter(vec![
            Ok::<_, LeafStop>(Watermarked::Item((
                0,
                make_bitmap(&[10, 50, (BUCKET_SIZE - 1) as u32]),
            ))),
            Ok(Watermarked::Watermark(BUCKET_SIZE)),
            Ok(Watermarked::Item((
                1,
                make_bitmap(&[0, (BUCKET_SIZE - 1) as u32]),
            ))),
            Ok(Watermarked::Watermark(2 * BUCKET_SIZE)),
            Ok(Watermarked::Item((2, make_bitmap(&[0, 50_000, 50_001])))),
            Ok(Watermarked::Watermark(2 * BUCKET_SIZE + 50_001)),
        ]);
        let out: Vec<Watermarked<u64>> = flatten_watermarked_buckets(
            marked_buckets,
            range,
            BUCKET_SIZE,
            ScanDirection::Ascending,
        )
        .try_collect()
        .await
        .unwrap();

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
                Watermarked::Watermark(2 * BUCKET_SIZE + 50_001),
            ],
        );
    }

    #[tokio::test]
    async fn scan_budget_below_unique_leaf_count_yields_misconfig_error() {
        // Defensive runtime guard: a per-request budget smaller than the
        // query's leaf count would produce a cursorless SCAN_LIMIT
        // (merged watermarks stay None until every child reports). The
        // eval surfaces this as a plain anyhow error — distinct from a
        // scan-limit stop — so the handler propagates it as Internal rather
        // than SCAN_LIMIT.
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
            SkipPolicy::DRAIN_ONLY,
            move |m| *metrics_sink.lock().unwrap() = Some(m),
        );
        let err = drain_marked(stream).await.unwrap_err();

        assert!(
            matches!(err, ScanStop::Fault(_)),
            "must be a Fault (cursorless), never a clean ScanLimit end"
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
        // should be consumed across all per-dimension fetches before a
        // scan-limit stop surfaces from the merged eval stream.
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
            SkipPolicy::DRAIN_ONLY,
            move |m| *metrics_sink.lock().unwrap() = Some(m),
        );
        let err = drain_marked(stream).await.unwrap_err();

        assert!(
            matches!(err, ScanStop::ScanLimit { .. }),
            "expected ScanLimit, got {err:?}"
        );
        // All four buckets were evaluated through budgeted_bucket_stream
        // before the fifth try_take() failed and surfaced a scan-limit stop.
        assert_eq!(metrics.lock().unwrap().unwrap().buckets_evaluated, 4);
    }

    /// Budget exhausting on an exclude leaf must NOT be mistaken for the
    /// exclude reaching its range terminus. With silent EOF semantics, includes
    /// past the exclude cutoff would leak unfiltered. With scan-limit errors,
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
        // the budget exhausts mid-scan, the scan-limit stop propagates
        // without the driver mistaking the exclude leaf's error for a
        // natural EOF.
        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..300_000,
            BUCKET_SIZE,
            ScanDirection::Ascending,
            2,
            SkipPolicy::DRAIN_ONLY,
            |_| {},
        );
        let result = drain_marked(stream).await;

        // Must error, not return Ok with leaked include rows.
        let err = result.expect_err("must surface scan-limit, not silently emit includes");
        assert!(
            matches!(err, ScanStop::ScanLimit { .. }),
            "expected ScanLimit, got {err:?}"
        );
    }

    /// Disjoint intersect with a large gap. The logical frontier jumps to the
    /// leading edge of bucket 100 before physical catch-up drains the dense
    /// sibling and exhausts the budget. The stopping round must not duplicate
    /// that frontier as an in-band beacon.
    #[tokio::test]
    async fn sparse_intersect_bundles_frontier_in_scan_limit() {
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
            SkipPolicy::DRAIN_ONLY,
            |_| {},
        );
        let all: Vec<Result<Watermarked<u64>, ScanStop>> = stream.collect().await;

        assert!(
            all.iter().all(|r| !matches!(r, Ok(Watermarked::Item(_)))),
            "disjoint intersect must not emit items"
        );
        let err = all
            .last()
            .expect("non-empty")
            .as_ref()
            .expect_err("scan must terminate with an error");
        let scan_frontier = match err {
            ScanStop::ScanLimit { scan_frontier } => *scan_frontier,
            other => panic!("expected ScanLimit, got {other:?}"),
        };
        assert_eq!(
            scan_frontier,
            100 * BUCKET_SIZE,
            "terminal must carry the logically advanced merged floor"
        );
        let watermarks: Vec<u64> = all
            .iter()
            .filter_map(|r| match r {
                Ok(Watermarked::Watermark(position)) => Some(*position),
                _ => None,
            })
            .collect();
        assert_eq!(
            watermarks,
            vec![100 * BUCKET_SIZE],
            "the stopping round must not append another frontier beacon"
        );
        assert_eq!(all.len(), watermarks.len() + 1);
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
            SkipPolicy::DRAIN_ONLY,
            |_| {},
        );
        let (items, watermarks) = drain_marked(stream).await.unwrap();

        // Items at the bits within each of the three populated buckets.
        assert_eq!(items, vec![1, 3 * BUCKET_SIZE + 2, 7 * BUCKET_SIZE + 3]);
        // The request floor is suppressed; sparse leading edges and every
        // post-bucket edge are emitted eagerly. The last bucket itself earns
        // the range terminus.
        assert_eq!(
            watermarks,
            vec![
                BUCKET_SIZE,
                3 * BUCKET_SIZE,
                4 * BUCKET_SIZE,
                7 * BUCKET_SIZE,
                8 * BUCKET_SIZE,
            ]
        );
    }

    #[tokio::test]
    async fn descending_exclusive_upper_bound_is_not_progress() {
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
            SkipPolicy::DRAIN_ONLY,
            |_| {},
        );
        let (items, watermarks) = drain_marked(stream).await.unwrap();

        assert_eq!(items, vec![7 * BUCKET_SIZE + 3, 3 * BUCKET_SIZE + 2, 1]);
        // The exclusive upper request position is not earned progress: the
        // first beacon follows the highest bucket's items at its low edge.
        assert_eq!(
            watermarks,
            vec![
                7 * BUCKET_SIZE,
                4 * BUCKET_SIZE,
                3 * BUCKET_SIZE,
                BUCKET_SIZE,
                0,
            ]
        );
    }

    #[tokio::test]
    async fn natural_completion_omits_terminus_but_retains_earned_progress() {
        // The include dimensions never align, so the evaluator emits no items.
        // Retiring the term is a natural terminal boundary, not additional scan
        // progress; the last eager beacon is the final position both live leaves
        // actually established before one reached EOF.
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
            SkipPolicy::DRAIN_ONLY,
            |_| {},
        );
        let (items, watermarks) = drain_marked(stream).await.unwrap();

        assert!(items.is_empty(), "disjoint intersect must not emit items");
        assert_eq!(
            watermarks,
            vec![
                BUCKET_SIZE,
                2 * BUCKET_SIZE,
                3 * BUCKET_SIZE,
                4 * BUCKET_SIZE,
                5 * BUCKET_SIZE,
            ],
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
            SkipPolicy::DRAIN_ONLY,
            |_| {},
        );
        let all: Vec<Watermarked<u64>> = stream.try_collect().await.unwrap();

        // The request floor is not scan progress. Each bucket's Items precede
        // its post-bucket watermark, and the shared edge between adjacent
        // buckets is emitted only once.
        assert_eq!(
            all,
            vec![
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
    async fn flatten_watermarked_buckets_descending() {
        let range = 50u64..(2 * BUCKET_SIZE + 50_001);
        let marked_buckets = stream::iter(vec![
            Ok::<_, LeafStop>(Watermarked::Item((2, make_bitmap(&[0, 50_000, 50_001])))),
            Ok(Watermarked::Watermark(2 * BUCKET_SIZE)),
            Ok(Watermarked::Item((
                1,
                make_bitmap(&[0, (BUCKET_SIZE - 1) as u32]),
            ))),
            Ok(Watermarked::Watermark(BUCKET_SIZE)),
            Ok(Watermarked::Item((
                0,
                make_bitmap(&[10, 50, (BUCKET_SIZE - 1) as u32]),
            ))),
            Ok(Watermarked::Watermark(50)),
        ]);
        let out: Vec<Watermarked<u64>> = flatten_watermarked_buckets(
            marked_buckets,
            range,
            BUCKET_SIZE,
            ScanDirection::Descending,
        )
        .try_collect()
        .await
        .unwrap();

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
                Watermarked::Watermark(50),
            ],
        );
    }

    /// A lone leaf budget stop becomes a merged scan limit carrying the exact
    /// floor supplied by the evaluator.
    #[test]
    fn collapse_single_budget_stop_binds_frontier() {
        assert!(matches!(
            collapse(vec![LeafStop::BudgetExhausted], 17 * BUCKET_SIZE),
            ScanStop::ScanLimit { scan_frontier } if scan_frontier == 17 * BUCKET_SIZE
        ));
    }

    /// Several leaf budget stops collapse to one scan limit without losing the
    /// evaluator's merged floor.
    #[test]
    fn collapse_all_budget_stops_bind_frontier() {
        assert!(matches!(
            collapse(
                vec![LeafStop::BudgetExhausted, LeafStop::BudgetExhausted],
                23 * BUCKET_SIZE
            ),
            ScanStop::ScanLimit { scan_frontier } if scan_frontier == 23 * BUCKET_SIZE
        ));
    }

    fn gap_probe_policy() -> SkipPolicy {
        SkipPolicy {
            drain_probe_rows: std::num::NonZeroU32::new(2),
        }
    }

    fn skewed_and_source() -> TestBucketSource {
        TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (test_key(b"a"), vec![(0, vec![1]), (50, vec![1])]),
                (
                    test_key(b"b"),
                    (0..=50).map(|bucket| (bucket, vec![1])).collect(),
                ),
            ])),
        }
    }

    #[tokio::test]
    async fn skewed_and_seeks_past_dead_gap() {
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b")]).unwrap(),
        ])
        .unwrap();
        let (metrics_tx, metrics_rx) = std::sync::mpsc::channel();
        let stream = eval_bitmap_query_stream(
            skewed_and_source(),
            query,
            0..(51 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            1_000,
            gap_probe_policy(),
            move |observed| metrics_tx.send(observed).unwrap(),
        );
        let (items, watermarks) = drain_marked(stream).await.unwrap();

        assert_eq!(items, vec![1, 50 * BUCKET_SIZE + 1]);
        assert_eq!(
            watermarks,
            vec![BUCKET_SIZE, 50 * BUCKET_SIZE, 51 * BUCKET_SIZE]
        );
        assert_eq!(
            metrics_rx.recv().unwrap(),
            BitmapScanMetrics {
                buckets_evaluated: 7,
                buckets_discarded: 3,
                leaf_seeks: 1,
            }
        );
    }

    #[tokio::test]
    async fn skewed_and_seeks_past_dead_gap_descending() {
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b")]).unwrap(),
        ])
        .unwrap();
        let (metrics_tx, metrics_rx) = std::sync::mpsc::channel();
        let stream = eval_bitmap_query_stream(
            skewed_and_source(),
            query,
            0..(51 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Descending,
            1_000,
            gap_probe_policy(),
            move |observed| metrics_tx.send(observed).unwrap(),
        );
        let (items, watermarks) = drain_marked(stream).await.unwrap();

        assert_eq!(items, vec![50 * BUCKET_SIZE + 1, 1]);
        assert_eq!(watermarks, vec![50 * BUCKET_SIZE, BUCKET_SIZE, 0]);
        assert_eq!(
            metrics_rx.recv().unwrap(),
            BitmapScanMetrics {
                buckets_evaluated: 7,
                buckets_discarded: 3,
                leaf_seeks: 1,
            }
        );
    }

    #[tokio::test]
    async fn budget_death_mid_gap_carries_jumped_frontier() {
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b")]).unwrap(),
        ])
        .unwrap();
        let first: Vec<_> = eval_bitmap_query_stream(
            skewed_and_source(),
            query.clone(),
            0..(51 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            5,
            gap_probe_policy(),
            |_| {},
        )
        .collect()
        .await;
        let first_items: Vec<_> = first
            .iter()
            .filter_map(|result| match result {
                Ok(Watermarked::Item(item)) => Some(*item),
                _ => None,
            })
            .collect();
        assert_eq!(first_items, vec![1]);
        assert!(matches!(
            first.last(),
            Some(Err(ScanStop::ScanLimit { scan_frontier }))
                if *scan_frontier == 50 * BUCKET_SIZE
        ));

        let resumed = eval_bitmap_query_stream(
            skewed_and_source(),
            query,
            (50 * BUCKET_SIZE)..(51 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            100,
            gap_probe_policy(),
            |_| {},
        );
        let (resumed_items, _) = drain_marked(resumed).await.unwrap();
        assert_eq!(resumed_items, vec![50 * BUCKET_SIZE + 1]);
    }

    #[tokio::test]
    async fn shared_leaf_never_skips_past_its_own_terms_candidate() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (test_key(b"a"), vec![(0, vec![1]), (50, vec![1])]),
                (test_key(b"e"), vec![(10, vec![3])]),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), exclude(b"e")]).unwrap(),
            BitmapTerm::new(vec![include(b"e")]).unwrap(),
        ])
        .unwrap();
        let (metrics_tx, metrics_rx) = std::sync::mpsc::channel();
        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..(51 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            100,
            gap_probe_policy(),
            move |observed| metrics_tx.send(observed).unwrap(),
        );
        let (items, _) = drain_marked(stream).await.unwrap();

        assert_eq!(items, vec![1, 10 * BUCKET_SIZE + 3, 50 * BUCKET_SIZE + 1]);
        let metrics = metrics_rx.recv().expect("metrics callback ran");
        assert_eq!(metrics.buckets_discarded, 0);
        assert_eq!(metrics.leaf_seeks, 0);
    }

    #[tokio::test]
    async fn exclude_leaf_drains_dead_rows_when_dragged() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (test_key(b"a"), vec![(0, vec![1]), (50, vec![1])]),
                (test_key(b"e"), vec![(10, vec![3])]),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), exclude(b"e")]).unwrap(),
        ])
        .unwrap();
        let (metrics_tx, metrics_rx) = std::sync::mpsc::channel();
        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..(51 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            100,
            gap_probe_policy(),
            move |observed| metrics_tx.send(observed).unwrap(),
        );
        let (items, _) = drain_marked(stream).await.unwrap();

        assert_eq!(items, vec![1, 50 * BUCKET_SIZE + 1]);
        let metrics = metrics_rx.recv().expect("metrics callback ran");
        assert_eq!(metrics.buckets_discarded, 1);
        assert_eq!(metrics.leaf_seeks, 0);
    }

    /// A storage fault co-occurring with budget exhaustion must win: masking
    /// the fault as a graceful scan limit would silently corrupt results.
    #[test]
    fn collapse_fault_outranks_budget_stop() {
        let collapsed = collapse(
            vec![
                LeafStop::BudgetExhausted,
                LeafStop::Fault(anyhow::anyhow!("storage boom")),
            ],
            7,
        );
        match collapsed {
            ScanStop::Fault(e) => assert!(e.to_string().contains("storage boom")),
            other => panic!("expected Fault to win, got {other:?}"),
        }
    }

    /// A storage fault outranks both budget exhaustion and cancellation.
    #[test]
    fn collapse_fault_outranks_budget_stop_and_cancelled() {
        let collapsed = collapse(
            vec![
                LeafStop::BudgetExhausted,
                LeafStop::Cancelled,
                LeafStop::Fault(anyhow::anyhow!("storage boom")),
            ],
            11,
        );
        match collapsed {
            ScanStop::Fault(e) => assert!(e.to_string().contains("storage boom")),
            other => panic!("expected Fault to win, got {other:?}"),
        }
    }

    /// Budget exhaustion outranks cancellation because it preserves a usable
    /// merged resume frontier.
    #[test]
    fn collapse_budget_stop_outranks_cancelled() {
        assert!(matches!(
            collapse(vec![LeafStop::Cancelled, LeafStop::BudgetExhausted], 29),
            ScanStop::ScanLimit { scan_frontier: 29 }
        ));
    }

    /// Cancellation remains cancellation when no higher-precedence leaf stop
    /// occurred in the evaluator round.
    #[test]
    fn collapse_all_cancelled() {
        assert!(matches!(
            collapse(vec![LeafStop::Cancelled, LeafStop::Cancelled], 31),
            ScanStop::Cancelled
        ));
    }

    /// Several concurrent faults combine into one terminal fault that retains
    /// every leaf's message rather than dropping all but one.
    #[test]
    fn collapse_combines_concurrent_faults() {
        let collapsed = collapse(
            vec![
                LeafStop::Fault(anyhow::anyhow!("boom one")),
                LeafStop::Fault(anyhow::anyhow!("boom two")),
            ],
            37,
        );
        match collapsed {
            ScanStop::Fault(e) => {
                let s = e.to_string();
                assert!(s.contains("boom one"), "missing first fault: {s}");
                assert!(s.contains("boom two"), "missing second fault: {s}");
            }
            other => panic!("expected combined Fault, got {other:?}"),
        }
    }

    /// `From<anyhow::Error>` on the leaf channel preserves backend failures as
    /// leaf faults rather than manufacturing a terminal disposition.
    #[test]
    fn from_anyhow_funnels_to_leaf_fault() {
        match LeafStop::from(anyhow::anyhow!("leaf storage boom")) {
            LeafStop::Fault(e) => assert!(e.to_string().contains("leaf storage boom")),
            other => panic!("expected leaf Fault, got {other:?}"),
        }
    }

    /// `From<anyhow::Error>` on the merged channel preserves backend failures
    /// as terminal faults rather than manufacturing a scan limit or cancel.
    #[test]
    fn from_anyhow_funnels_to_scan_fault() {
        match ScanStop::from(anyhow::anyhow!("merged storage boom")) {
            ScanStop::Fault(e) => assert!(e.to_string().contains("merged storage boom")),
            other => panic!("expected scan Fault, got {other:?}"),
        }
    }

    fn stream_items(all: &[Result<Watermarked<u64>, ScanStop>]) -> Vec<u64> {
        all.iter()
            .filter_map(|r| match r {
                Ok(Watermarked::Item(v)) => Some(*v),
                _ => None,
            })
            .collect()
    }

    /// Absent-dimension semantics (mirrors the iter-side tests): an include whose key has no rows
    /// at all annihilates its conjunction (`∩ ∅ = ∅`). Pinned explicitly because this shape only
    /// arises when a queried key was never written (e.g. a sender with no transactions), which
    /// live-cluster tests never exercise.
    #[tokio::test]
    async fn absent_include_annihilates_term() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([(test_key(b"a"), vec![(0, vec![1, 2])])])),
        };
        let query =
            BitmapQuery::new(vec![BitmapTerm::new(vec![include(b"ghost")]).unwrap()]).unwrap();

        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..(2 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            3,
            SkipPolicy::DRAIN_ONLY,
            |_| {},
        );
        let all: Vec<Result<Watermarked<u64>, ScanStop>> = stream.collect().await;
        let items = stream_items(&all);

        assert!(
            items.is_empty(),
            "absent include must annihilate: {items:?}"
        );
    }

    /// A present include cannot rescue a conjunction whose other include is absent — the
    /// intersection is still empty.
    #[tokio::test]
    async fn absent_include_annihilates_term_despite_present_include() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([(test_key(b"a"), vec![(0, vec![1, 2])])])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"ghost")]).unwrap(),
        ])
        .unwrap();

        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..(2 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            3,
            SkipPolicy::DRAIN_ONLY,
            |_| {},
        );
        let all: Vec<Result<Watermarked<u64>, ScanStop>> = stream.collect().await;
        let items = stream_items(&all);

        assert!(
            items.is_empty(),
            "absent include must annihilate: {items:?}"
        );
    }

    /// An exclude whose key has no rows subtracts nothing (`∖ ∅`): the present include's matches
    /// pass through untouched.
    #[tokio::test]
    async fn absent_exclude_is_noop() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([(test_key(b"a"), vec![(0, vec![1, 2])])])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), exclude(b"ghost")]).unwrap(),
        ])
        .unwrap();

        let stream = eval_bitmap_query_stream(
            source,
            query,
            0..(2 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            3,
            SkipPolicy::DRAIN_ONLY,
            |_| {},
        );
        let all: Vec<Result<Watermarked<u64>, ScanStop>> = stream.collect().await;

        assert_eq!(stream_items(&all), vec![1, 2]);
    }
}
