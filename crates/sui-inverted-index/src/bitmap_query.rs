// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! DNF bitmap index queries over ordered bucket streams.
//!
//! Callers build a `BitmapQuery` as an OR of terms. Each term is an AND of
//! signed dimension-key literals. Evaluation yields matching bitmap members as
//! they are produced. Back-pressure from downstream consumers, e.g. a
//! `.take(page_size)`, propagates back to the backend-provided bucket streams
//! and avoids materializing matches we won't use.
//!
//! Queries are intentionally restricted to anchored DNF: every term must contain
//! at least one positive literal. Positive literals give the evaluator concrete
//! bitmap streams to scan and intersect; negative literals only shrink those
//! candidate streams. Supporting negative-only terms such as `NOT sender = A`
//! would require scanning an external universe for the requested range and
//! subtracting from it, which defeats the index's selective streaming behavior
//! and makes pagination depend on a full-range scan. Requiring DNF at the API
//! boundary also keeps this evaluator as a set of ordered stream merge-joins
//! instead of a recursive expression engine or a query normalizer.
//!
//! Backends provide one ordered `(bucket_id, RoaringBitmap)` stream per
//! dimension key. The merge-join machinery here is storage-agnostic: BigTable,
//! RocksDB, or any other backend can reuse it as long as its bucket stream is
//! sparse, ordered by the requested scan direction, and stores bitmap positions
//! relative to that bucket.

use std::ops::Range;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use anyhow::Result;
use anyhow::bail;
use futures::Stream;
use futures::StreamExt;
use futures::stream::BoxStream;
use futures::stream::Peekable;
use itertools::Itertools;
use roaring::RoaringBitmap;

use crate::dimensions::IndexDimension;

/// Terminal signal: the per-request bucket-fetch budget is exhausted.
/// Surfaced as `anyhow::Error` so it short-circuits `try_stream!`
/// pipelines through the existing error path. A silent EOF would let
/// `subtract_two` emit unfiltered include rows past the exclude side's
/// last fetched bucket.
#[derive(Debug, thiserror::Error)]
#[error("bitmap scan budget exhausted")]
pub struct BitmapScanLimitExceeded;

/// Aggregate of multiple terminal errors raised during the same poll
/// cycle of a multi-way combinator (e.g. two children of `intersect_n`
/// both error before the next loop iteration). The single-error
/// shortcut returns the inner error directly so existing downcasts on
/// the wire keep working — `MultiError` only appears when 2+ errors
/// coincide, in which case downstream consumers should use
/// [`error_contains`] to interrogate the aggregate.
#[derive(Debug)]
pub struct MultiError(Vec<anyhow::Error>);

impl MultiError {
    /// Wrap a non-empty error list into an `anyhow::Error`. With exactly
    /// one error, returns it unwrapped so the common case preserves
    /// `downcast_ref` behavior on the original concrete type.
    pub fn collapse(mut errs: Vec<anyhow::Error>) -> anyhow::Error {
        assert!(!errs.is_empty(), "MultiError::collapse on empty Vec");
        if errs.len() == 1 {
            return errs.pop().expect("len == 1");
        }
        anyhow::Error::new(MultiError(errs))
    }

    pub fn iter(&self) -> impl Iterator<Item = &anyhow::Error> {
        self.0.iter()
    }
}

impl std::fmt::Display for MultiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} concurrent errors", self.0.len())?;
        for (i, e) in self.0.iter().enumerate() {
            write!(f, "\n  [{i}] {e}")?;
        }
        Ok(())
    }
}

impl std::error::Error for MultiError {}

/// Downcast probe that looks through a `MultiError` aggregate. Returns
/// the first inner `T` whether the top-level error is `T` directly or a
/// `MultiError` containing one. Use this instead of `downcast_ref::<T>`
/// at sites that may receive aggregated errors from the bitmap
/// combinators.
pub fn error_contains<T: std::error::Error + Send + Sync + 'static>(
    err: &anyhow::Error,
) -> Option<&T> {
    if let Some(t) = err.downcast_ref::<T>() {
        return Some(t);
    }
    if let Some(multi) = err.downcast_ref::<MultiError>() {
        return multi.iter().find_map(|e| e.downcast_ref::<T>());
    }
    None
}

/// Item or progress watermark flowing through a bitmap eval pipeline.
/// `Watermark(p)` means every Item with position strictly before `p`
/// in scan direction has been emitted upstream. Downstream stages must
/// preserve watermark/item ordering — that's what makes the watermark a
/// safe resume cursor on timeout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Watermarked<T> {
    Item(T),
    Watermark(u64),
}

impl<T> Watermarked<T> {
    pub fn map_item<U>(self, f: impl FnOnce(T) -> U) -> Watermarked<U> {
        match self {
            Watermarked::Item(t) => Watermarked::Item(f(t)),
            Watermarked::Watermark(p) => Watermarked::Watermark(p),
        }
    }
}

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
    /// backend reads can exceed this by up to `BitmapQuery::leaf_count()`.
    pub buckets_evaluated: u64,
}

/// Per-request evaluated-bucket budget shared across all dimension
/// streams of one eval. Charges are post-poll — see
/// `budget_limited_bucket_stream`.
#[derive(Clone)]
struct BitmapScanBudget {
    initial: u64,
    remaining: Arc<AtomicU64>,
}

impl BitmapScanBudget {
    fn new(initial: u64) -> Self {
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

    fn buckets_evaluated(&self) -> u64 {
        self.initial
            .saturating_sub(self.remaining.load(Ordering::SeqCst))
    }
}

/// Merge per-child watermark slots into the combinator's output.
/// `None` if any child has yet to emit a first watermark — combinator
/// must not claim progress past an unknown floor. Ascending min,
/// descending max (the slowest source is the floor).
fn merge_watermarks(slots: &[Option<u64>], direction: ScanDirection) -> Option<u64> {
    // Short-circuits to None if any slot is None.
    let all: Vec<u64> = slots.iter().copied().collect::<Option<Vec<_>>>()?;
    if all.is_empty() {
        return None;
    }
    Some(match direction {
        ScanDirection::Ascending => *all.iter().min().expect("non-empty"),
        ScanDirection::Descending => *all.iter().max().expect("non-empty"),
    })
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

/// A stream of `(bucket_id, RoaringBitmap)` in the requested bucket order.
/// Bitmap positions are **relative** to the bucket (u32 offsets `[0, BUCKET_SIZE)`)
/// - edge trimming against the requested range happens at the flatten step.
type BucketItem = Result<(u64, RoaringBitmap)>;
pub type BucketStream = BoxStream<'static, BucketItem>;

/// A bucket stream that interleaves data buckets with per-source progress
/// watermarks. Combinators (`intersect_n`, `union_n`, `subtract_two`)
/// merge child watermarks structurally so the output always reflects
/// "every source has scanned past P."
type WatermarkedBucket = Result<Watermarked<(u64, RoaringBitmap)>>;
pub type WatermarkedBucketStream = BoxStream<'static, WatermarkedBucket>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanDirection {
    Ascending,
    Descending,
}

impl ScanDirection {
    pub fn is_ascending(self) -> bool {
        matches!(self, Self::Ascending)
    }
}

/// Storage backend that can scan one bitmap dimension key over a member range.
///
/// The returned stream must be sparse and ordered by the requested direction.
/// Missing bucket rows are interpreted as all-zero bitmaps by the merge-join
/// operators.
pub trait BitmapBucketSource: Clone + Send + 'static {
    fn scan_bucket_stream(
        &self,
        dimension_key: Vec<u8>,
        range: Range<u64>,
        direction: ScanDirection,
    ) -> BucketStream;
}

/// A DNF query over bitmap dimension scans.
///
/// A query is a disjunction of terms. It must contain at least one term, and
/// every term must be anchored by at least one included dimension key.
#[derive(Clone, Debug)]
pub struct BitmapQuery {
    terms: Vec<BitmapTerm>,
}

/// One conjunction in a DNF bitmap query.
///
/// A term is a conjunction of signed literals. It must include at least one
/// positive literal so the evaluator has a finite candidate stream to refine.
#[derive(Clone, Debug)]
pub struct BitmapTerm {
    literals: Vec<BitmapLiteral>,
}

/// Validated `[dimension_tag][dimension_value]` lookup key.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct BitmapKey(Vec<u8>);

/// One signed dimension-key literal in a bitmap term.
#[derive(Clone, Debug)]
pub enum BitmapLiteral {
    Include(BitmapKey),
    Exclude(BitmapKey),
}

impl BitmapKey {
    pub fn new(bytes: Vec<u8>) -> Result<Self> {
        if bytes.is_empty() {
            bail!("bitmap dimension key must not be empty");
        }
        if bytes.len() == 1 {
            bail!("bitmap dimension value must not be empty");
        }
        if IndexDimension::from_tag_byte(bytes[0]).is_none() {
            bail!("unknown bitmap dimension tag {}", bytes[0]);
        }
        Ok(Self(bytes))
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<Vec<u8>> for BitmapKey {
    type Error = anyhow::Error;

    fn try_from(value: Vec<u8>) -> Result<Self> {
        Self::new(value)
    }
}

impl BitmapLiteral {
    pub fn include(dimension_key: Vec<u8>) -> Result<Self> {
        Ok(Self::Include(BitmapKey::new(dimension_key)?))
    }

    pub fn exclude(dimension_key: Vec<u8>) -> Result<Self> {
        Ok(Self::Exclude(BitmapKey::new(dimension_key)?))
    }
}

impl BitmapQuery {
    pub fn new(terms: Vec<BitmapTerm>) -> Result<Self> {
        if terms.is_empty() {
            bail!("bitmap query must contain at least one term");
        }
        Ok(Self { terms })
    }

    pub fn scan(dimension_key: Vec<u8>) -> Result<Self> {
        Ok(Self {
            terms: vec![BitmapTerm::new(vec![BitmapLiteral::include(
                dimension_key,
            )?])?],
        })
    }

    /// Total leaf literal count across every term. The per-request
    /// budget must be at least this many so every leaf can emit its
    /// first watermark.
    pub fn leaf_count(&self) -> usize {
        self.terms.iter().map(|t| t.literals.len()).sum()
    }
}

impl BitmapTerm {
    pub fn new(literals: Vec<BitmapLiteral>) -> Result<Self> {
        if !literals
            .iter()
            .any(|literal| matches!(literal, BitmapLiteral::Include(_)))
        {
            bail!("bitmap query term must contain at least one include literal");
        }
        Ok(Self { literals })
    }
}

/// Evaluate a DNF `BitmapQuery` against a backend-provided bitmap source.
///
/// `budget` caps evaluated buckets across all dimension scans (see
/// [`BitmapScanLimitExceeded`] and [`BitmapScanMetrics`]). `on_metrics`
/// fires exactly once when the eval stream is dropped.
///
/// Output emits `Watermarked::Item(absolute_member_id)` interleaved with
/// `Watermarked::Watermark(p)` merged from the leaf streams — sparse
/// intersects that drop every bucket still emit watermarks at the rate
/// sources advance.
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
    let leaves = query.leaf_count();
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
    let range_terminus = if direction.is_ascending() {
        range.end
    } else {
        range.start
    };
    async_stream::stream! {
        let _guard = guard;
        futures::pin_mut!(inner);
        let mut last_emitted: Option<u64> = None;
        while let Some(item) = inner.next().await {
            let is_err = item.is_err();
            if let Ok(Watermarked::Watermark(p)) = &item {
                last_emitted = Some(*p);
            }
            yield item;
            if is_err {
                // Don't synthesize a final range-terminus watermark on
                // error — the scan did NOT reach it.
                return;
            }
        }
        // Natural EOF: guarantee a final range-terminus watermark so
        // clients learn "scan covered the entire range." Combinators that
        // short-circuit early (e.g. intersect when one side EOFs) leave
        // `last_emitted` short of the terminus; this fills the gap.
        if frontier_advanced(last_emitted, range_terminus, direction) {
            yield Ok(Watermarked::Watermark(range_terminus));
        }
    }
    .boxed()
}

fn frontier_advanced(prev: Option<u64>, next: u64, direction: ScanDirection) -> bool {
    match prev {
        None => true,
        Some(prev) => match direction {
            ScanDirection::Ascending => next > prev,
            ScanDirection::Descending => next < prev,
        },
    }
}

/// Evaluate a DNF `BitmapQuery` as an ordered `WatermarkedBucketStream`.
fn eval_bitmap_query_bucket_stream<S>(
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
    let streams: Vec<WatermarkedBucketStream> = query
        .terms
        .into_iter()
        .map(|term| {
            term_bucket_stream(
                source.clone(),
                term,
                range.clone(),
                bucket_size,
                direction,
                budget.clone(),
            )
            .boxed()
        })
        .collect();
    union_n(streams, direction).boxed()
}

/// Evaluate one DNF term: intersect all includes, then subtract excludes.
fn term_bucket_stream<S>(
    source: S,
    term: BitmapTerm,
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
    budget: BitmapScanBudget,
) -> WatermarkedBucketStream
where
    S: BitmapBucketSource,
{
    let mut include_streams: Vec<WatermarkedBucketStream> = Vec::new();
    let mut exclude_streams: Vec<WatermarkedBucketStream> = Vec::new();

    for literal in term.literals {
        let (key, is_include) = match literal {
            BitmapLiteral::Include(key) => (key.into_inner(), true),
            BitmapLiteral::Exclude(key) => (key.into_inner(), false),
        };
        let stream = budget_limited_bucket_stream(
            source.scan_bucket_stream(key, range.clone(), direction),
            budget.clone(),
            range.clone(),
            bucket_size,
            direction,
        )
        .boxed();
        if is_include {
            include_streams.push(stream);
        } else {
            exclude_streams.push(stream);
        }
    }

    let include_stream = intersect_n(include_streams, direction).boxed();
    // Skip subtract when nothing to exclude. `union_n` of zero streams
    // emits nothing, which would leave `subtract_two`'s `b_watermark`
    // slot `None` forever and suppress every merged watermark.
    if exclude_streams.is_empty() {
        return include_stream;
    }
    let exclude_stream = union_n(exclude_streams, direction);

    // `subtract_two` polls both sides with `try_join!`, so include and
    // exclude scans are opened/read concurrently.
    subtract_two(include_stream, exclude_stream, direction).boxed()
}

/// Wrap a per-dimension raw bucket stream into a `WatermarkedBucketStream`,
/// emitting `Watermark(pre), Item, Watermark(post)` per bucket and a
/// final `Watermark(range_terminus)` on natural EOF.
///
/// On budget exhaustion yields `Err(BitmapScanLimitExceeded)` — never
/// silent EOF, which `subtract_two` would mistake for "no more excludes."
/// Charge is post-poll so `budget == leaves` doesn't spuriously trip at
/// natural EOF.
fn budget_limited_bucket_stream<S>(
    inner: S,
    budget: BitmapScanBudget,
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
) -> impl Stream<Item = WatermarkedBucket> + Send + 'static
where
    S: Stream<Item = BucketItem> + Send + 'static,
{
    let range_terminus = if direction.is_ascending() {
        range.end
    } else {
        range.start
    };
    async_stream::try_stream! {
        futures::pin_mut!(inner);
        while let Some(item) = inner.next().await {
            let item = item?;
            if !budget.try_take() {
                Err(anyhow::Error::new(BitmapScanLimitExceeded))?;
            }
            let (bucket_id, _) = &item;
            let bucket_start = bucket_id.saturating_mul(bucket_size);
            let bucket_end_exclusive = bucket_start.saturating_add(bucket_size);
            // Clamp to the request range: cursors round-trip into
            // subsequent requests with different ranges, so an
            // out-of-bound cursor would be a foot-gun.
            let (pre, post) = if direction.is_ascending() {
                (
                    bucket_start.max(range.start),
                    bucket_end_exclusive.min(range.end),
                )
            } else {
                (
                    bucket_end_exclusive.min(range.end),
                    bucket_start.max(range.start),
                )
            };
            yield Watermarked::Watermark(pre);
            yield Watermarked::Item(item);
            yield Watermarked::Watermark(post);
        }
        // Natural EOF: emit a final watermark at the range terminus so
        // combinators learn this source covered the whole range.
        yield Watermarked::Watermark(range_terminus);
    }
}

/// Multi-way merge intersection. Emits bucket_ids present in every
/// child, with the bitwise AND of their bitmaps; drops empty results.
/// Per-child watermarks drain eagerly each iteration; the min/max
/// merged across children emits when it advances.
pub fn intersect_n<S>(
    streams: Vec<S>,
    direction: ScanDirection,
) -> impl Stream<Item = WatermarkedBucket> + Send + 'static
where
    S: Stream<Item = WatermarkedBucket> + Send + Unpin + 'static,
{
    async_stream::try_stream! {
        if streams.is_empty() {
            return;
        }
        let mut children: Vec<Peekable<S>> =
            streams.into_iter().map(|s| s.peekable()).collect();
        let mut child_watermarks: Vec<Option<u64>> = vec![None; children.len()];
        let mut last_emitted: Option<u64> = None;

        loop {
            // Drain pending watermarks concurrently so we don't serialize
            // on the slowest child. Defer errors until AFTER emitting the
            // merged watermark — progress up to the error point should
            // still reach the client.
            let outcomes = futures::future::join_all(
                children
                    .iter_mut()
                    .map(|child| drain_pending_watermarks(Pin::new(child))),
            )
            .await;
            let mut deferred_errors: Vec<anyhow::Error> = Vec::new();
            for (i, outcome) in outcomes.into_iter().enumerate() {
                if let Some(p) = outcome.last_watermark {
                    child_watermarks[i] = Some(p);
                }
                if let Some(e) = outcome.error {
                    deferred_errors.push(e);
                }
            }
            if let Some(merged) = merge_watermarks(&child_watermarks, direction)
                && frontier_advanced(last_emitted, merged, direction)
            {
                yield Watermarked::Watermark(merged);
                last_emitted = Some(merged);
            }
            if !deferred_errors.is_empty() {
                Err(MultiError::collapse(deferred_errors))?;
            }

            // Poll children together so independent backend scans run
            // concurrently.
            let Some((peeks, max_bucket)) = complete_peeks(peek_buckets(&mut children).await?)
            else {
                break;
            };
            let target_bucket = match direction {
                ScanDirection::Ascending => max_bucket,
                ScanDirection::Descending => peeks.iter().copied().min().expect("non-empty peeks"),
            };

            if peeks.iter().all(|&b| b == target_bucket) {
                // All children at the same bucket: intersect and emit.
                let mut acc: Option<RoaringBitmap> = None;
                for child in children.iter_mut() {
                    let (bid, bitmap) = take_bucket_item(Pin::new(child)).await?;
                    debug_assert_eq!(bid, target_bucket);
                    acc = Some(match acc {
                        None => bitmap,
                        Some(a) => a & bitmap,
                    });
                }
                let bitmap = acc.expect("children non-empty");
                if !bitmap.is_empty() {
                    yield Watermarked::Item((target_bucket, bitmap));
                }
            } else {
                // Sparse bitmap rows encode only non-empty buckets. A
                // child lagging the alignment target intersects with an
                // implicit all-zero bitmap at that bucket → empty result.
                // Consume the lagging bucket and re-peek.
                for (i, child) in children.iter_mut().enumerate() {
                    let drop_bucket = match direction {
                        ScanDirection::Ascending => peeks[i] < target_bucket,
                        ScanDirection::Descending => peeks[i] > target_bucket,
                    };
                    if drop_bucket {
                        let _ = take_bucket_item(Pin::new(child)).await?;
                    }
                }
            }
        }
    }
}

/// Multi-way merge union. Emits every bucket_id produced by any child,
/// with the bitwise OR of bitmaps at that bucket. Per-child watermarks
/// drain eagerly; min/max merged across surviving children emits when
/// it advances.
pub fn union_n<S>(
    streams: Vec<S>,
    direction: ScanDirection,
) -> impl Stream<Item = WatermarkedBucket> + Send + 'static
where
    S: Stream<Item = WatermarkedBucket> + Send + Unpin + 'static,
{
    async_stream::try_stream! {
        if streams.is_empty() {
            return;
        }
        let mut children: Vec<Peekable<S>> =
            streams.into_iter().map(|s| s.peekable()).collect();
        let mut child_watermarks: Vec<Option<u64>> = vec![None; children.len()];
        let mut last_emitted: Option<u64> = None;

        loop {
            // Drain pending watermarks concurrently to avoid serializing
            // on the slowest child. Defer errors until AFTER emitting the
            // merged watermark.
            let outcomes = futures::future::join_all(
                children
                    .iter_mut()
                    .map(|child| drain_pending_watermarks(Pin::new(child))),
            )
            .await;
            let mut deferred_errors: Vec<anyhow::Error> = Vec::new();
            for (i, outcome) in outcomes.into_iter().enumerate() {
                if let Some(p) = outcome.last_watermark {
                    child_watermarks[i] = Some(p);
                }
                if let Some(e) = outcome.error {
                    deferred_errors.push(e);
                }
            }
            if let Some(merged) = merge_watermarks(&child_watermarks, direction)
                && frontier_advanced(last_emitted, merged, direction)
            {
                yield Watermarked::Watermark(merged);
                last_emitted = Some(merged);
            }
            if !deferred_errors.is_empty() {
                Err(MultiError::collapse(deferred_errors))?;
            }

            let peeks = peek_buckets(&mut children).await?;

            // Evict exhausted children so their resources (e.g. semaphore
            // permits) release promptly. The evicted child's last
            // watermark already drained into `last_emitted`, so dropping
            // its slot is safe.
            let mut surviving_children = Vec::with_capacity(children.len());
            let mut surviving_peeks = Vec::with_capacity(peeks.len());
            let mut surviving_watermarks = Vec::with_capacity(child_watermarks.len());
            for ((child, peek), wm) in children
                .into_iter()
                .zip_eq(peeks)
                .zip_eq(child_watermarks.into_iter())
            {
                if let Some(b) = peek {
                    surviving_children.push(child);
                    surviving_peeks.push(b);
                    surviving_watermarks.push(wm);
                }
            }
            children = surviving_children;
            child_watermarks = surviving_watermarks;

            if surviving_peeks.is_empty() {
                return;
            }
            let next_bucket = match direction {
                ScanDirection::Ascending => *surviving_peeks
                    .iter()
                    .min()
                    .expect("non-empty after evicting None peeks"),
                ScanDirection::Descending => *surviving_peeks
                    .iter()
                    .max()
                    .expect("non-empty after evicting None peeks"),
            };

            let mut acc: Option<RoaringBitmap> = None;
            for (i, child) in children.iter_mut().enumerate() {
                if surviving_peeks[i] == next_bucket {
                    let (_, bitmap) = take_bucket_item(Pin::new(child)).await?;
                    acc = Some(match acc {
                        None => bitmap,
                        Some(a) => a | bitmap,
                    });
                }
            }
            if let Some(bitmap) = acc
                && !bitmap.is_empty()
            {
                yield Watermarked::Item((next_bucket, bitmap));
            }
        }
    }
}

/// Merge-join subtraction for an anchored negative literal.
///
/// A negative literal in a valid term is evaluated as `a AND NOT b`, where `a`
/// is the already-anchored positive candidate stream. Bitmap subtraction
/// (`a_bm - b_bm`) has the same truth table without materializing `NOT b` over
/// the whole range:
///
/// ```text
/// a b | a AND NOT b
/// 1 0 | 1
/// 0 1 | 0
/// 0 0 | 0
/// 1 1 | 0
/// ```
///
/// For each bucket in `a`, emits `a_bm - b_bm` if `b` has the same bucket,
/// else emits `a_bm` unchanged. Drops empty results.
pub fn subtract_two<A, B>(
    a: A,
    b: B,
    direction: ScanDirection,
) -> impl Stream<Item = WatermarkedBucket> + Send + 'static
where
    A: Stream<Item = WatermarkedBucket> + Send + 'static,
    B: Stream<Item = WatermarkedBucket> + Send + 'static,
{
    async_stream::try_stream! {
        let a = a.peekable();
        let b = b.peekable();
        futures::pin_mut!(a);
        futures::pin_mut!(b);
        let mut a_watermark: Option<u64> = None;
        let mut b_watermark: Option<u64> = None;
        let mut last_emitted: Option<u64> = None;

        loop {
            // Drain both sides concurrently, emit merged "both past P"
            // if it advanced. Defer errors until after the emit so
            // mid-drain progress reaches the client.
            let (a_outcome, b_outcome) = futures::future::join(
                drain_pending_watermarks(a.as_mut()),
                drain_pending_watermarks(b.as_mut()),
            )
            .await;
            if let Some(p) = a_outcome.last_watermark {
                a_watermark = Some(p);
            }
            if let Some(p) = b_outcome.last_watermark {
                b_watermark = Some(p);
            }
            let mut deferred_errors: Vec<anyhow::Error> = Vec::new();
            if let Some(e) = a_outcome.error {
                deferred_errors.push(e);
            }
            if let Some(e) = b_outcome.error {
                deferred_errors.push(e);
            }
            if let Some(merged) = merge_watermarks(&[a_watermark, b_watermark], direction)
                && frontier_advanced(last_emitted, merged, direction)
            {
                yield Watermarked::Watermark(merged);
                last_emitted = Some(merged);
            }
            if !deferred_errors.is_empty() {
                Err(MultiError::collapse(deferred_errors))?;
            }

            // Concurrent peek buffers both head rows; the later `next()`
            // calls consume them.
            let (a_peek, b_peek) =
                futures::try_join!(peek_bucket(a.as_mut()), peek_bucket(b.as_mut()))?;
            let Some(a_bucket) = a_peek else {
                return;
            };

            match b_peek {
                None => {
                    // No more negatives: flush a.
                    let (bid, bitmap) = take_bucket_item(a.as_mut()).await?;
                    if !bitmap.is_empty() {
                        yield Watermarked::Item((bid, bitmap));
                    }
                }
                Some(bb)
                    if (direction.is_ascending() && bb > a_bucket)
                        || (!direction.is_ascending() && bb < a_bucket) =>
                {
                    // b is ahead, emit a unchanged.
                    let (bid, bitmap) = take_bucket_item(a.as_mut()).await?;
                    if !bitmap.is_empty() {
                        yield Watermarked::Item((bid, bitmap));
                    }
                }
                Some(bb)
                    if (direction.is_ascending() && bb < a_bucket)
                        || (!direction.is_ascending() && bb > a_bucket) =>
                {
                    // b is behind; skip it.
                    let _ = take_bucket_item(b.as_mut()).await?;
                }
                Some(_) => {
                    // Same bucket: subtract.
                    let (bid, a_bm) = take_bucket_item(a.as_mut()).await?;
                    let (_, b_bm) = take_bucket_item(b.as_mut()).await?;
                    let diff = a_bm - b_bm;
                    if !diff.is_empty() {
                        yield Watermarked::Item((bid, diff));
                    }
                }
            }
        }
    }
}

/// Convenience adapter: wrap a single raw `BucketStream` into a
/// `WatermarkedBucketStream` with one `Watermark(post_bucket)` after each
/// bucket plus one final at the range terminus on EOF.
///
/// The DNF eval pipeline uses `budget_limited_bucket_stream` instead;
/// this helper is for backend-side consumers (e.g. RocksDB
/// single-dimension scans) that want bucket-level output without the
/// full eval machinery.
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

/// Outcome of `drain_pending_watermarks`: latest watermark consumed (if
/// any) AND any terminal error at the same head. Combinators apply the
/// watermark before propagating the error so mid-drain progress still
/// surfaces.
struct DrainOutcome {
    last_watermark: Option<u64>,
    error: Option<anyhow::Error>,
}

/// Drain consecutive `Watermark` frames from the head of a
/// `Peekable<WatermarkedBucketStream>`. Combinators call this at the top of
/// each loop iteration so subsequent peeks see Items only.
async fn drain_pending_watermarks<S>(mut s: Pin<&mut Peekable<S>>) -> DrainOutcome
where
    S: Stream<Item = WatermarkedBucket>,
{
    let mut last: Option<u64> = None;
    loop {
        let action = match s.as_mut().peek().await {
            None => {
                return DrainOutcome {
                    last_watermark: last,
                    error: None,
                };
            }
            Some(Ok(Watermarked::Item(_))) => {
                return DrainOutcome {
                    last_watermark: last,
                    error: None,
                };
            }
            Some(Ok(Watermarked::Watermark(_))) => PeekAction::ConsumeWatermark,
            Some(Err(_)) => PeekAction::ConsumeError,
        };
        match action {
            PeekAction::ConsumeWatermark => match s.as_mut().next().await {
                Some(Ok(Watermarked::Watermark(p))) => last = Some(p),
                _ => unreachable!("peek confirmed Watermark"),
            },
            PeekAction::ConsumeError => match s.as_mut().next().await {
                Some(Err(e)) => {
                    return DrainOutcome {
                        last_watermark: last,
                        error: Some(e),
                    };
                }
                _ => unreachable!("peek confirmed Err"),
            },
        }
    }
}

enum PeekAction {
    ConsumeWatermark,
    ConsumeError,
}

/// Consume the next bucket Item. Caller must have confirmed via
/// `peek_bucket` that the head is an Item — watermark or EOF here is a
/// logic error.
async fn take_bucket_item<S>(mut s: Pin<&mut Peekable<S>>) -> Result<(u64, RoaringBitmap)>
where
    S: Stream<Item = WatermarkedBucket>,
{
    match s.as_mut().next().await {
        Some(Ok(Watermarked::Item(it))) => Ok(it),
        Some(Ok(Watermarked::Watermark(_))) => {
            unreachable!("take_bucket_item called on Watermark — drain should have run first")
        }
        Some(Err(e)) => Err(e),
        None => unreachable!("take_bucket_item called on EOF — peek should have caught it"),
    }
}

/// Peek the next bucket_id. Caller MUST have run `drain_pending_watermarks`
/// in this loop iteration — a Watermark at the head here means the
/// drain step was skipped, which is a refactor bug rather than a
/// runtime condition. Returns `None` on EOF; surfaces errors via `?`.
async fn peek_bucket<S>(mut s: Pin<&mut Peekable<S>>) -> Result<Option<u64>>
where
    S: Stream<Item = WatermarkedBucket>,
{
    match s.as_mut().peek().await {
        None => Ok(None),
        Some(Ok(Watermarked::Item((b, _)))) => Ok(Some(*b)),
        Some(Ok(Watermarked::Watermark(_))) => {
            // Surface rather than silently consume: a stray WM here means
            // the per-iteration `drain_pending_watermarks` contract was
            // violated by a future refactor. Eating the WM would turn
            // that bug into "watermarks just stop appearing," which is
            // exactly the silent-progress-loss class this whole pipeline
            // is designed to avoid.
            bail!(
                "peek_bucket observed a stray Watermark — drain_pending_watermarks \
                 must run first in each combinator loop iteration"
            );
        }
        Some(Err(_)) => match s.as_mut().next().await {
            Some(Err(e)) => Err(e),
            _ => unreachable!("peek confirmed Err"),
        },
    }
}

async fn peek_buckets<S>(streams: &mut [Peekable<S>]) -> Result<Vec<Option<u64>>>
where
    S: Stream<Item = WatermarkedBucket> + Unpin,
{
    futures::future::try_join_all(
        streams
            .iter_mut()
            .map(|stream| peek_bucket(Pin::new(stream))),
    )
    .await
}

fn complete_peeks(peeks: Vec<Option<u64>>) -> Option<(Vec<u64>, u64)> {
    let mut buckets = Vec::with_capacity(peeks.len());
    let mut max_bucket = None;

    for peek in peeks {
        let bucket = peek?;
        max_bucket = Some(max_bucket.map_or(bucket, |max: u64| max.max(bucket)));
        buckets.push(bucket);
    }

    Some((buckets, max_bucket?))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use futures::TryStreamExt;
    use futures::stream;

    use super::*;

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

    /// Ascending `Watermark(pre)`, `Item`, `Watermark(post)` per bucket.
    /// Feeds combinator tests without the real budget machinery.
    fn make_marked_stream_asc(items: Vec<(u64, &[u32])>) -> WatermarkedBucketStream {
        let frames: Vec<WatermarkedBucket> = items
            .into_iter()
            .flat_map(|(bid, bits)| {
                let bm = make_bitmap(bits);
                let pre = bid * BUCKET_SIZE;
                let post = (bid + 1) * BUCKET_SIZE;
                [
                    Ok::<_, anyhow::Error>(Watermarked::Watermark(pre)),
                    Ok(Watermarked::Item((bid, bm))),
                    Ok(Watermarked::Watermark(post)),
                ]
            })
            .collect();
        stream::iter(frames).boxed()
    }

    /// Descending variant: pre = bucket high edge, post = bucket low edge.
    fn make_marked_stream_desc(items: Vec<(u64, &[u32])>) -> WatermarkedBucketStream {
        let frames: Vec<WatermarkedBucket> = items
            .into_iter()
            .flat_map(|(bid, bits)| {
                let bm = make_bitmap(bits);
                let pre = (bid + 1) * BUCKET_SIZE;
                let post = bid * BUCKET_SIZE;
                [
                    Ok::<_, anyhow::Error>(Watermarked::Watermark(pre)),
                    Ok(Watermarked::Item((bid, bm))),
                    Ok(Watermarked::Watermark(post)),
                ]
            })
            .collect();
        stream::iter(frames).boxed()
    }

    fn collect_bitmap_items(items: Vec<WatermarkedBucket>) -> Vec<(u64, Vec<u32>)> {
        items
            .into_iter()
            .filter_map(|r| match r.unwrap() {
                Watermarked::Item((b, bm)) => Some((b, bm.iter().collect())),
                Watermarked::Watermark(_) => None,
            })
            .collect()
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

    #[tokio::test]
    async fn intersect_n_basic() {
        let a = make_marked_stream_asc(vec![(0, &[1, 2, 3]), (1, &[4, 5]), (2, &[6])]);
        let b = make_marked_stream_asc(vec![(0, &[2, 3]), (2, &[6, 7])]);
        let c = make_marked_stream_asc(vec![(0, &[3, 4]), (2, &[6])]);
        let out: Vec<_> = intersect_n(vec![a, b, c], ScanDirection::Ascending)
            .collect()
            .await;
        let out = collect_bitmap_items(out);
        // bucket 0: {1,2,3} intersect {2,3} intersect {3,4} = {3}
        // bucket 1: only in a, dropped by AND
        // bucket 2: {6} intersect {6,7} intersect {6} = {6}
        assert_eq!(out, vec![(0, vec![3]), (2, vec![6])]);
    }

    #[tokio::test]
    async fn intersect_n_descending() {
        let a = make_marked_stream_desc(vec![(2, &[6]), (1, &[4, 5]), (0, &[1, 2, 3])]);
        let b = make_marked_stream_desc(vec![(2, &[6, 7]), (0, &[2, 3])]);
        let c = make_marked_stream_desc(vec![(2, &[6]), (0, &[3, 4])]);
        let out: Vec<_> = intersect_n(vec![a, b, c], ScanDirection::Descending)
            .collect()
            .await;
        let out = collect_bitmap_items(out);
        assert_eq!(out, vec![(2, vec![6]), (0, vec![3])]);
    }

    #[tokio::test]
    async fn peek_bucket_propagates_errors_without_panicking() {
        let stream: WatermarkedBucketStream =
            stream::iter(vec![Err(anyhow::anyhow!("boom"))]).boxed();
        let mut stream = stream.peekable();

        let err = peek_bucket(Pin::new(&mut stream)).await.unwrap_err();

        assert!(err.to_string().contains("boom"));
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn intersect_n_disjoint_dropped() {
        let a = make_marked_stream_asc(vec![(0, &[1])]);
        let b = make_marked_stream_asc(vec![(0, &[2])]);
        let out: Vec<_> = intersect_n(vec![a, b], ScanDirection::Ascending)
            .collect()
            .await;
        let out = collect_bitmap_items(out);
        // intersection is empty, bucket dropped
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn intersect_n_one_empty() {
        let a = make_marked_stream_asc(vec![(0, &[1]), (1, &[2])]);
        let b = make_marked_stream_asc(vec![]);
        let out: Vec<_> = intersect_n(vec![a, b], ScanDirection::Ascending)
            .collect()
            .await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn union_n_basic() {
        let a = make_marked_stream_asc(vec![(0, &[1, 2]), (2, &[6])]);
        let b = make_marked_stream_asc(vec![(0, &[2, 3]), (1, &[4])]);
        let out: Vec<_> = union_n(vec![a, b], ScanDirection::Ascending)
            .collect()
            .await;
        let out = collect_bitmap_items(out);
        assert_eq!(out, vec![(0, vec![1, 2, 3]), (1, vec![4]), (2, vec![6])]);
    }

    #[tokio::test]
    async fn union_n_descending() {
        let a = make_marked_stream_desc(vec![(2, &[6]), (0, &[1, 2])]);
        let b = make_marked_stream_desc(vec![(1, &[4]), (0, &[2, 3])]);
        let out: Vec<_> = union_n(vec![a, b], ScanDirection::Descending)
            .collect()
            .await;
        let out = collect_bitmap_items(out);
        assert_eq!(out, vec![(2, vec![6]), (1, vec![4]), (0, vec![1, 2, 3])]);
    }

    #[tokio::test]
    async fn union_n_drops_exhausted_child() {
        use std::sync::atomic::AtomicBool;
        use std::sync::atomic::Ordering;
        use std::task::Context;
        use std::task::Poll;

        struct ObserveDrop<S> {
            inner: S,
            dropped: Arc<AtomicBool>,
        }

        impl<S: Stream<Item = WatermarkedBucket> + Unpin> Stream for ObserveDrop<S> {
            type Item = WatermarkedBucket;
            fn poll_next(
                mut self: Pin<&mut Self>,
                cx: &mut Context<'_>,
            ) -> Poll<Option<Self::Item>> {
                Pin::new(&mut self.inner).poll_next(cx)
            }
        }

        impl<S> Drop for ObserveDrop<S> {
            fn drop(&mut self) {
                self.dropped.store(true, Ordering::SeqCst);
            }
        }

        let dropped = Arc::new(AtomicBool::new(false));
        let short: WatermarkedBucketStream = ObserveDrop {
            inner: stream::iter(vec![
                Ok::<_, anyhow::Error>(Watermarked::Item((0u64, make_bitmap(&[1])))),
                Ok(Watermarked::Watermark(BUCKET_SIZE)),
            ]),
            dropped: dropped.clone(),
        }
        .boxed();
        let long = make_marked_stream_asc(vec![(0, &[2]), (1, &[3]), (2, &[4])]);

        // Filter merged Watermark frames out — the test only checks the
        // bucket-Item order and the drop timing relative to bucket pulls.
        let mut merged = union_n(vec![short, long], ScanDirection::Ascending)
            .try_filter_map(|m| async move {
                Ok(match m {
                    Watermarked::Item(it) => Some(it),
                    Watermarked::Watermark(_) => None,
                })
            })
            .boxed();

        // Bucket 0 merges both children; the short stream still has its
        // post-bucket Watermark pending. Eviction happens on the next
        // iteration after the watermark drain returns None for short.
        let (b, _) = merged.try_next().await.unwrap().unwrap();
        assert_eq!(b, 0);
        assert!(!dropped.load(Ordering::SeqCst));

        // Pulling bucket 1 first drains short's pending Watermark (updating
        // its slot), tries to peek a bucket and sees None for short, evicts
        // it (dropping the Peekable and its inner ObserveDrop), then aligns
        // and emits bucket 1 from the surviving long stream. The long stream
        // still has bucket 2 pending — proving the drop happened mid-merge,
        // not at completion.
        let (b, _) = merged.try_next().await.unwrap().unwrap();
        assert_eq!(b, 1);
        assert!(dropped.load(Ordering::SeqCst));

        let (b, _) = merged.try_next().await.unwrap().unwrap();
        assert_eq!(b, 2);
        assert!(merged.try_next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn subtract_two_basic() {
        let a = make_marked_stream_asc(vec![(0, &[1, 2, 3]), (1, &[4, 5]), (2, &[6, 7])]);
        let b = make_marked_stream_asc(vec![(0, &[2]), (2, &[7]), (3, &[100])]);
        let out: Vec<_> = subtract_two(a, b, ScanDirection::Ascending).collect().await;
        let out = collect_bitmap_items(out);
        assert_eq!(
            out,
            vec![
                (0, vec![1, 3]), // {1,2,3} - {2}
                (1, vec![4, 5]), // unchanged
                (2, vec![6]),    // {6,7} - {7}
            ],
        );
    }

    #[tokio::test]
    async fn subtract_two_descending() {
        let a = make_marked_stream_desc(vec![(2, &[6, 7]), (1, &[4, 5]), (0, &[1, 2, 3])]);
        let b = make_marked_stream_desc(vec![(3, &[100]), (2, &[7]), (0, &[2])]);
        let out: Vec<_> = subtract_two(a, b, ScanDirection::Descending)
            .collect()
            .await;
        let out = collect_bitmap_items(out);
        assert_eq!(out, vec![(2, vec![6]), (1, vec![4, 5]), (0, vec![1, 3]),],);
    }

    #[tokio::test]
    async fn subtract_two_drops_fully_erased_buckets() {
        let a = make_marked_stream_asc(vec![(0, &[1])]);
        let b = make_marked_stream_asc(vec![(0, &[1])]);
        let out: Vec<_> = subtract_two(a, b, ScanDirection::Ascending).collect().await;
        let out = collect_bitmap_items(out);
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn subtract_two_empty_right() {
        let a = make_marked_stream_asc(vec![(0, &[1, 2]), (5, &[3])]);
        let b = make_marked_stream_asc(vec![]);
        let out: Vec<_> = subtract_two(a, b, ScanDirection::Ascending).collect().await;
        let out = collect_bitmap_items(out);
        assert_eq!(out, vec![(0, vec![1, 2]), (5, vec![3])]);
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
    async fn scan_budget_below_leaf_count_yields_misconfig_error() {
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
        // All four buckets were evaluated through budget_limited_bucket_stream
        // before the fifth try_take() failed and surfaced BitmapScanLimitExceeded.
        assert_eq!(metrics.lock().unwrap().unwrap().buckets_evaluated, 4);
    }

    /// Budget exhausting on the exclude side must NOT be interpreted as
    /// "no more excludes" by `subtract_two`. With silent EOF semantics,
    /// includes past the exclude cutoff would leak unfiltered. With
    /// `ScanLimitExceeded`, the error propagates and the eval pipeline
    /// short-circuits cleanly.
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
        // `scan_budget_below_leaf_count_yields_misconfig_error`). Once
        // the budget exhausts mid-scan, ScanLimitExceeded propagates
        // without subtract_two interpreting exclude-side EOF as "no
        // more excludes."
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

    /// Disjoint-intersect: source streams advance but the combinator yields
    /// no output bucket, so per-bucket watermarks (which fire only on output)
    /// never advance. The eval root's periodic + on-error frontier injection
    /// must surface real progress; otherwise handlers fall back to the
    /// request lower bound and the client livelocks on retry.
    #[tokio::test]
    async fn sparse_intersect_emits_frontier_watermark_before_scan_limit() {
        // include "a" at buckets [0, 1, 2, ...], include "b" at bucket 100 —
        // disjoint, so intersect_n drops a's lagging buckets one by one and
        // emits zero output. Budget=4 forces error mid-scan.
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
        // Per-source watermarks: each drain pass collapses consecutive
        // watermarks at the source into the latest, so post(b) and the
        // immediately-following pre(b+gap) merge — only one watermark per
        // gap surfaces. For buckets [0, 3, 7]:
        // - Watermark(0): pre(0) before bucket 0.
        // - Watermark(3*bs): collapsed post(0)=bs + pre(3)=3bs.
        // - Watermark(7*bs): collapsed post(3)=4bs + pre(7)=7bs.
        // - Watermark(8*bs): collapsed post(7)=8bs (=range.end) + EOF.
        assert_eq!(
            watermarks,
            vec![0, 3 * BUCKET_SIZE, 7 * BUCKET_SIZE, 8 * BUCKET_SIZE,]
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
        // Descending: drain collapses consecutive watermarks into the
        // latest. For buckets [7, 3, 0]: pre(7)=8bs (=range.end),
        // post(7)=7bs then drain collapses with pre(3)=4bs → 4bs is
        // the merged value (smaller wins in descending). Etc.
        assert_eq!(
            watermarks,
            vec![8 * BUCKET_SIZE, 4 * BUCKET_SIZE, BUCKET_SIZE, 0,]
        );
    }

    #[tokio::test]
    async fn eval_emits_per_source_watermarks_and_final_eof_when_no_bucket_yielded() {
        // Two include dimensions whose buckets never align -> intersect
        // yields no Items. Per-source watermarks (pre+post per bucket)
        // still propagate the actual scan progress through the
        // combinators' min-merge, and the eval root caps the stream with a
        // final range_end watermark on natural EOF so clients see "scan
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

    /// `peek_bucket` must surface a stray `Watermark` as an error rather
    /// than silently consume it. Drain is the caller's contract; eating
    /// the WM here would turn a refactor bug into "watermarks just stop
    /// appearing," exactly the silent-progress-loss class this pipeline
    /// is built to avoid.
    #[tokio::test]
    async fn peek_bucket_errors_on_stray_watermark() {
        let stream: WatermarkedBucketStream =
            stream::iter(vec![Ok(Watermarked::<(u64, RoaringBitmap)>::Watermark(42))]).boxed();
        let mut stream = stream.peekable();
        let err = peek_bucket(Pin::new(&mut stream)).await.unwrap_err();
        assert!(
            err.to_string().contains("stray Watermark"),
            "expected stray-watermark error, got: {err}"
        );
    }
}
