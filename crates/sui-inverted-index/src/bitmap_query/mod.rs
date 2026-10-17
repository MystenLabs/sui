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
//! candidate streams. Negative-only terms such as `NOT sender = A` are
//! supported by anchoring them upstream on a universe include — a stored
//! existence marker in event-space (`EventExtant`), a scan-time-synthesized
//! dense leaf in tx-space (`TxUniverse`, see [`dense_universe_buckets`]) — so
//! the evaluator itself stays a set of ordered stream merge-joins with no
//! complement-specific code path. The full-range scan such a term implies is
//! inherent to negation and is bounded by the per-request bucket budget.
//!
//! Backends provide one ordered `(bucket_id, RoaringBitmap)` stream or iterator
//! per dimension key. The merge-join machinery here is storage-agnostic:
//! BigTable, RocksDB, or any other backend can reuse it as long as its bucket
//! source is sparse, ordered by the requested scan direction, and stores bitmap
//! positions relative to that bucket.

use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use anyhow::Result;
use anyhow::bail;
use futures::stream::BoxStream;
use roaring::RoaringBitmap;

use crate::dimensions::IndexDimension;

mod iter;
mod stream;

pub use iter::eval_bitmap_query_bucket_iter;
pub use stream::eval_bitmap_query_stream;
pub use stream::flatten_watermarked_buckets;

// Cross-checked against the iterative evaluator in iter.rs tests.
#[cfg(test)]
pub(crate) use stream::eval_bitmap_query_bucket_stream;

/// Terminal raised by a single leaf/backend bucket stream. Positionless by
/// construction — one leaf cannot know the merged multi-term floor. Leaf stops
/// never reach a List driver: the DNF evaluators consume them and re-raise a
/// [`ScanStop`] carrying the merged frontier.
#[derive(Debug, thiserror::Error)]
pub enum LeafStop {
    /// This leaf's share of the bucket-scan budget is exhausted.
    #[error("bitmap scan limit reached")]
    BudgetExhausted,
    /// The request's cancellation token fired.
    #[error("bitmap scan cancelled")]
    Cancelled,
    /// A backend/storage fault.
    #[error(transparent)]
    Fault(anyhow::Error),
}

/// Terminal signal of a merged bitmap eval stream, raised on the error channel
/// so the `try_stream!` pipeline short-circuits (a clean end-of-stream means
/// "scanned the whole range"). The List handlers map each variant to a wire
/// outcome with one exhaustive match.
///
/// Mental model: leaves raise positionless [`LeafStop`]s; the evaluator turns
/// them into a `ScanStop` that ALWAYS carries the resume frontier on
/// `ScanLimit`; in-band `Watermarked::Watermark`s are only mid-scan progress
/// beacons, never the resume channel.
#[derive(Debug, thiserror::Error)]
pub enum ScanStop {
    /// Budget exhausted: a graceful early stop. The handler ends the stream
    /// with `QUERY_END_REASON_SCAN_LIMIT`; the frontier is the resume cursor.
    #[error("bitmap scan limit reached")]
    ScanLimit {
        /// Merged floor position every term provably scanned to when the budget
        /// died — the exact value the stopping round's progress beacon would
        /// have carried. It lives in the scanned index's member-id domain
        /// (tx-seq for transaction/checkpoint indexes; encoded event-seq — see
        /// `event_seq` — for the event index). Consumers apply their existing
        /// domain conversion and resume arithmetic.
        scan_frontier: u64,
    },
    /// Cancelled → gRPC `Cancelled` status.
    #[error("bitmap scan cancelled")]
    Cancelled,
    /// Backend/storage fault → gRPC `Internal`, error carried unchanged.
    #[error(transparent)]
    Fault(anyhow::Error),
}

impl From<anyhow::Error> for LeafStop {
    fn from(err: anyhow::Error) -> Self {
        LeafStop::Fault(err)
    }
}

impl From<anyhow::Error> for ScanStop {
    fn from(err: anyhow::Error) -> Self {
        ScanStop::Fault(err)
    }
}

/// Reduce the leaf stops raised in one evaluator round to the driver-facing
/// terminal. Precedence: Fault > BudgetExhausted > Cancelled (a real fault must
/// surface as Internal; a budget stop with its frontier beats a bare Cancel —
/// the resume point costs nothing to deliver). `frontier` is the merged floor
/// the caller computed for this round; it is bound into `ScanLimit` only.
/// Panics on empty input — evaluators only collapse when a leaf actually erred.
pub(crate) fn collapse(stops: Vec<LeafStop>, scan_frontier: u64) -> ScanStop {
    assert!(!stops.is_empty(), "collapse on empty Vec");
    let mut cancelled = false;
    let mut budget = false;
    let mut faults: Vec<anyhow::Error> = Vec::new();
    for s in stops {
        match s {
            LeafStop::Cancelled => cancelled = true,
            LeafStop::BudgetExhausted => budget = true,
            LeafStop::Fault(err) => faults.push(err),
        }
    }
    match faults.len() {
        0 => {
            if budget {
                ScanStop::ScanLimit { scan_frontier }
            } else {
                debug_assert!(cancelled, "collapse saw only non-erroring leaves");
                ScanStop::Cancelled
            }
        }
        1 => ScanStop::Fault(faults.pop().expect("len == 1")),
        n => {
            let combined = faults
                .iter()
                .enumerate()
                .map(|(i, err)| format!("  [{i}] {err}"))
                .collect::<Vec<_>>()
                .join("\n");
            ScanStop::Fault(anyhow::anyhow!(
                "{n} concurrent bitmap scan faults:\n{combined}"
            ))
        }
    }
}

/// Item or progress watermark flowing through a bitmap eval pipeline.
/// `Watermark(p)` means every Item with position strictly before `p`
/// in scan direction has been emitted upstream. Downstream stages must
/// preserve watermark/item ordering — that's what makes the watermark a
/// safe resume cursor on timeout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Watermarked<T, P = u64> {
    Item(T),
    Watermark(P),
}

impl<T, P> Watermarked<T, P> {
    pub fn map_item<U>(self, f: impl FnOnce(T) -> U) -> Watermarked<U, P> {
        match self {
            Watermarked::Item(t) => Watermarked::Item(f(t)),
            Watermarked::Watermark(p) => Watermarked::Watermark(p),
        }
    }

    pub fn map_watermark<Q>(self, f: impl FnOnce(P) -> Q) -> Watermarked<T, Q> {
        match self {
            Watermarked::Item(t) => Watermarked::Item(t),
            Watermarked::Watermark(p) => Watermarked::Watermark(f(p)),
        }
    }
}

/// A stream of `(bucket_id, RoaringBitmap)` in the requested bucket order.
/// Bitmap positions are **relative** to the bucket (u32 offsets `[0, BUCKET_SIZE)`)
/// - edge trimming against the requested range happens at the flatten step.
pub type BucketItem = Result<(u64, RoaringBitmap), LeafStop>;
pub type BucketStream = BoxStream<'static, BucketItem>;

/// A bucket stream that interleaves data buckets with progress watermarks.
/// The flat DNF driver derives each watermark from the slowest leaf's
/// position, so the output always reflects "every source has scanned past P."
pub(crate) type WatermarkedBucket = Result<Watermarked<(u64, RoaringBitmap)>, ScanStop>;
pub type WatermarkedBucketStream = BoxStream<'static, WatermarkedBucket>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanDirection {
    Ascending,
    Descending,
}
/// Physical gap-crossing policy. Logical frontier jumps are unconditional;
/// this only bounds how many charged dead rows a lagging leaf drains from its
/// open scan per lag episode before seeking past the gap.
#[derive(Clone, Copy, Debug)]
pub struct SkipPolicy {
    /// `None` never seeks and drains dead rows without bound.
    pub drain_probe_rows: Option<std::num::NonZeroU32>,
}

impl SkipPolicy {
    /// Drain dead rows without seeking; logical frontier jumps remain enabled.
    pub const DRAIN_ONLY: Self = Self {
        drain_probe_rows: None,
    };

    /// What a gap catch-up step that reopens a leaf scan costs, in bucket
    /// rows: the dead rows this policy drains before it judges a reopen
    /// cheaper. 0 when it never reopens.
    pub(crate) fn reopen_cost_rows(self) -> u64 {
        self.drain_probe_rows
            .map_or(0, |probe| u64::from(probe.get()))
    }
}

impl ScanDirection {
    pub fn is_ascending(self) -> bool {
        matches!(self, Self::Ascending)
    }
}

/// How often an evaluator emits the progress watermarks it computes.
///
/// A watermark is emitted once `min_buckets` of scan work have been done since
/// the last emission; until then it is held, replacing any older held one.
/// The scan's first output is never held. Items and stops pass through
/// unchanged.
///
/// Scan work is counted in bucket rows: the rows charged to the budget
/// ([`BitmapScanMetrics::buckets_evaluated`]), plus, for every gap catch-up
/// step that reopened at least one leaf scan, the evaluation's
/// [`SkipPolicy::drain_probe_rows`]. A reopen is a new backend read, and the
/// skip policy reopens exactly when it judges that cheaper than draining that
/// many dead rows, so it is priced at them. Leaves reopened in the same step
/// count once, because the stream evaluator reopens them concurrently.
///
/// An emission is an emitted watermark or an item with a match inside the
/// scan range; such an item also discards the held watermark. Items are whole
/// buckets, so when the range starts or ends mid-bucket (e.g. a page resuming
/// from a cursor) an edge item can hold only matches outside it. That item
/// sends the client nothing and is not an emission.
///
/// The held watermark is discarded at every stop: a [`ScanStop::ScanLimit`]
/// carries a frontier further along, and after a [`ScanStop::Fault`] or
/// [`ScanStop::Cancelled`] the consumer is left with the last emitted
/// watermark. At a clean end it is emitted only with `emit_held_at_end`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WatermarkPacing {
    /// Scan work, in bucket rows, between emissions. With 0 nothing is held.
    pub min_buckets: u64,
    /// Emit the held watermark at a clean end instead of discarding it.
    pub emit_held_at_end: bool,
}

/// Bucket-scan accounting for one evaluation, passed once to `on_metrics`
/// when the evaluator's stream or iterator is dropped. Nothing is reported
/// when the stream evaluator rejects a misconfigured budget up front.
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

/// Bucket budget shared by all leaf scans of one evaluation; each leaf charges
/// a bucket as it pulls it (`stream::budgeted_bucket_stream`,
/// `iter::BudgetedBucketIter`).
#[derive(Clone)]
pub(crate) struct BitmapScanBudget {
    initial: u64,
    remaining: Arc<AtomicU64>,
    discarded: Arc<AtomicU64>,
    seeks: Arc<AtomicU64>,
    reopen_steps: Arc<AtomicU64>,
}

impl BitmapScanBudget {
    pub(crate) fn new(initial: u64) -> Self {
        Self {
            initial,
            remaining: Arc::new(AtomicU64::new(initial)),
            discarded: Arc::new(AtomicU64::new(0)),
            seeks: Arc::new(AtomicU64::new(0)),
            reopen_steps: Arc::new(AtomicU64::new(0)),
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

    /// Charge a leaf's first bucket; always succeeds. Without this
    /// reservation a sparse term could drain the shared pool before a sibling
    /// leaf reaches its first bucket, and that leaf could never report a
    /// position (a cursorless `SCAN_LIMIT`). The pool is still decremented
    /// when not empty, so `buckets_evaluated` undercounts only first buckets
    /// taken after it ran out.
    fn take_first(&self) {
        let _ = self
            .remaining
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |b| {
                Some(b.saturating_sub(1))
            });
    }

    /// Bucket rows charged so far ([`BitmapScanMetrics::buckets_evaluated`]);
    /// [`WatermarkPacer`] counts these.
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

    /// Count one gap catch-up step that reopened at least one leaf scan.
    fn note_reopen_step(&self) {
        self.reopen_steps.fetch_add(1, Ordering::Relaxed);
    }

    fn discarded(&self) -> u64 {
        self.discarded.load(Ordering::Relaxed)
    }

    fn seeks(&self) -> u64 {
        self.seeks.load(Ordering::Relaxed)
    }

    /// Gap catch-up steps that reopened a leaf scan so far;
    /// [`WatermarkPacer`] counts these.
    fn reopen_steps(&self) -> u64 {
        self.reopen_steps.load(Ordering::Relaxed)
    }
}

/// Calls `on_metrics` once, on drop, with the final [`BitmapScanMetrics`].
/// Evaluators keep it inside the stream or iterator they return.
pub(crate) struct ObserveOnDrop<F: FnOnce(BitmapScanMetrics)> {
    budget: BitmapScanBudget,
    callback: Option<F>,
}

impl<F: FnOnce(BitmapScanMetrics)> Drop for ObserveOnDrop<F> {
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

/// Applies a [`WatermarkPacing`] to one evaluation's bucket-level output,
/// reading the scan work done from the evaluation's shared budget.
///
/// The stream evaluator outputs a round before that round's gap catch-up and
/// the iterator evaluator after it, so when catch-up drains rows or reopens
/// leaves the two can pace the same scan differently.
pub(crate) struct WatermarkPacer {
    pacing: WatermarkPacing,
    budget: BitmapScanBudget,
    /// [`SkipPolicy::reopen_cost_rows`] of the evaluation's skip policy.
    reopen_cost_rows: u64,
    /// Scan range; the member-level output drops matches outside it.
    range: Range<u64>,
    bucket_size: u64,
    /// Newest watermark not yet emitted; always newer than every emitted one.
    held: Option<u64>,
    /// Scan work done at the last emission; `None` before the first.
    work_at_last_emission: Option<u64>,
}

impl WatermarkPacer {
    pub(crate) fn new(
        pacing: WatermarkPacing,
        policy: SkipPolicy,
        budget: BitmapScanBudget,
        range: Range<u64>,
        bucket_size: u64,
    ) -> Self {
        Self {
            pacing,
            budget,
            reopen_cost_rows: policy.reopen_cost_rows(),
            range,
            bucket_size,
            held: None,
            work_at_last_emission: None,
        }
    }

    /// The output to emit for one unpaced output: the output itself, or
    /// nothing while a watermark is held.
    pub(crate) fn pace(&mut self, output: WatermarkedBucket) -> Option<WatermarkedBucket> {
        match output {
            Ok(Watermarked::Item((bucket, bitmap))) => {
                if self.has_member_in_range(bucket, &bitmap) {
                    self.record_emission(self.scan_work());
                }
                Some(Ok(Watermarked::Item((bucket, bitmap))))
            }
            Ok(Watermarked::Watermark(position)) => {
                let work = self.scan_work();
                let due = self
                    .work_at_last_emission
                    .is_none_or(|last| work.saturating_sub(last) >= self.pacing.min_buckets);
                if due {
                    self.record_emission(work);
                    Some(Ok(Watermarked::Watermark(position)))
                } else {
                    self.held = Some(position);
                    None
                }
            }
            Err(stop) => {
                self.held = None;
                Some(Err(stop))
            }
        }
    }

    /// Scan work done so far, in bucket rows (see [`WatermarkPacing`]).
    fn scan_work(&self) -> u64 {
        self.budget.buckets_evaluated().saturating_add(
            self.reopen_cost_rows
                .saturating_mul(self.budget.reopen_steps()),
        )
    }

    fn record_emission(&mut self, work: u64) {
        self.held = None;
        self.work_at_last_emission = Some(work);
    }

    /// Whether `bitmap` (offsets within `bucket`) has a member inside the scan
    /// range.
    fn has_member_in_range(&self, bucket: u64, bitmap: &RoaringBitmap) -> bool {
        let bucket_start = bucket.saturating_mul(self.bucket_size);
        let low = self
            .range
            .start
            .saturating_sub(bucket_start)
            .min(self.bucket_size);
        let high = self
            .range
            .end
            .saturating_sub(bucket_start)
            .min(self.bucket_size);
        low < high && bitmap.range(low as u32..high as u32).next().is_some()
    }

    /// The watermark to emit when the unpaced output ends without a stop.
    pub(crate) fn finish(&mut self) -> Option<u64> {
        let held = self.held.take();
        held.filter(|_| self.pacing.emit_held_at_end)
    }
}

/// Synthesized bucket sequence for the dense tx-seq universe: one full bitmap
/// per bucket touched by `range`, in scan-direction order. Backends return this
/// for the query-only `IndexDimension::TxUniverse` key instead of reading
/// storage — the tx-seq namespace is dense, so the universe is computable.
///
/// Bitmaps carry all `[0, bucket_size)` relative bits even in edge buckets;
/// trimming against the requested range happens at the flatten step, same as
/// for stored buckets.
pub fn dense_universe_buckets(
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
) -> DenseUniverseBuckets {
    let bits = u32::try_from(bucket_size).expect("bucket size fits in u32");
    let buckets = if range.is_empty() {
        0..0
    } else {
        (range.start / bucket_size)..(range.end - 1) / bucket_size + 1
    };
    DenseUniverseBuckets {
        buckets,
        direction,
        bits,
    }
}

/// Synthesized dense-universe bucket iterator.
pub struct DenseUniverseBuckets {
    buckets: Range<u64>,
    direction: ScanDirection,
    bits: u32,
}

impl DenseUniverseBuckets {
    /// Reposition to the first bucket at or past `bucket` in scan direction.
    pub fn seek_bucket(&mut self, bucket: u64) {
        match self.direction {
            ScanDirection::Ascending => self.buckets.start = self.buckets.start.max(bucket),
            ScanDirection::Descending => {
                self.buckets.end = self.buckets.end.min(bucket.saturating_add(1))
            }
        }
    }
}

impl Iterator for DenseUniverseBuckets {
    type Item = (u64, RoaringBitmap);

    fn next(&mut self) -> Option<Self::Item> {
        let bucket = match self.direction {
            ScanDirection::Ascending => self.buckets.next(),
            ScanDirection::Descending => self.buckets.next_back(),
        }?;
        let mut bitmap = RoaringBitmap::new();
        bitmap.insert_range(0..self.bits);
        Some((bucket, bitmap))
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

/// Synchronous bucket iterator that can reposition within its scan bounds.
pub trait SeekableBucketIterator: Iterator<Item = BucketItem> {
    /// Reposition so the next bucket is the first at or past `bucket` in scan
    /// direction. Only the backend scan moves: the evaluator's adapter around
    /// each leaf keeps the budget and the first-bucket reservation.
    fn seek_bucket(&mut self, bucket: u64);
}

/// Storage backend that can scan one bitmap dimension key synchronously.
///
/// This is for request-local backends such as RocksDB, where the bucket scan
/// naturally owns or borrows a synchronous iterator. The iterator evaluator is
/// fully synchronous so these iterators can stay on the blocking task that owns
/// them.
pub trait BitmapBucketIteratorSource<'a>: Clone + 'a {
    type Iter: SeekableBucketIterator + 'a;

    fn scan_bucket_iter(
        &self,
        dimension_key: Vec<u8>,
        range: Range<u64>,
        direction: ScanDirection,
    ) -> Self::Iter;
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

    pub fn key_bytes(&self) -> &[u8] {
        match self {
            BitmapLiteral::Include(k) | BitmapLiteral::Exclude(k) => k.as_bytes(),
        }
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

    /// Count of distinct dimension-key leaves the query will scan. Identical
    /// keys across literals (whether within a term or across terms) collapse
    /// to one leaf at evaluation time, so the per-request budget floor —
    /// "every leaf can emit its first watermark" — applies to this
    /// deduplicated count, not the raw literal total.
    pub fn unique_leaf_count(&self) -> usize {
        self.terms
            .iter()
            .flat_map(|t| t.literals.iter().map(|l| l.key_bytes()))
            .collect::<HashSet<_>>()
            .len()
    }

    pub fn terms(&self) -> &[BitmapTerm] {
        &self.terms
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

    pub fn literals(&self) -> &[BitmapLiteral] {
        &self.literals
    }
}

/// Deduplicated leaf list + per-term references over those leaves, ready for
/// the evaluator. `keys[i]` is the dimension-key bytes for leaf `i`; each
/// [`TermSpec`] references leaves by that index.
pub(crate) struct DedupedQuery {
    pub(crate) keys: Vec<Vec<u8>>,
    pub(crate) terms: Vec<TermSpec>,
}

/// Deduplicate literals across the whole query and translate each term's
/// includes/excludes into indices over the shared leaf list.
///
/// Dedup is keyed on encoded dimension-key bytes, so the same key appearing
/// in multiple terms (e.g. `(sender=A AND module=X) OR (sender=A AND
/// type=Y)`) maps to a single backend scan instead of two duplicate scans.
pub(crate) fn build_term_specs(terms: Vec<BitmapTerm>) -> DedupedQuery {
    let mut key_to_idx: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut keys: Vec<Vec<u8>> = Vec::new();
    let mut specs: Vec<TermSpec> = Vec::with_capacity(terms.len());
    for term in terms {
        let mut include_idx = Vec::with_capacity(term.literals.len());
        let mut exclude_idx = Vec::with_capacity(term.literals.len());
        for literal in term.literals {
            let (push_target, key) = match literal {
                BitmapLiteral::Include(k) => (&mut include_idx, k.into_inner()),
                BitmapLiteral::Exclude(k) => (&mut exclude_idx, k.into_inner()),
            };
            let idx = match key_to_idx.get(&key) {
                Some(&i) => i,
                None => {
                    let i = keys.len();
                    key_to_idx.insert(key.clone(), i);
                    keys.push(key);
                    i
                }
            };
            push_target.push(idx);
        }
        specs.push(TermSpec {
            includes: include_idx,
            excludes: exclude_idx,
            unsatisfiable: false,
        });
    }
    DedupedQuery { keys, terms: specs }
}

/// The less-advanced of two frontier positions in scan direction: the min
/// ascending, the max descending. Used to keep a merged frontier bounded by
/// the slowest contributor.
fn bound_in_direction(a: u64, b: u64, direction: ScanDirection) -> u64 {
    match direction {
        ScanDirection::Ascending => a.min(b),
        ScanDirection::Descending => a.max(b),
    }
}
/// The more-advanced of two positions in scan direction: max ascending, min
/// descending.
pub(crate) fn advance_in_direction(a: u64, b: u64, direction: ScanDirection) -> u64 {
    match direction {
        ScanDirection::Ascending => a.max(b),
        ScanDirection::Descending => a.min(b),
    }
}

/// Whether `a` is strictly before `b` in scan direction.
pub(crate) fn strictly_before(a: u64, b: u64, direction: ScanDirection) -> bool {
    match direction {
        ScanDirection::Ascending => a < b,
        ScanDirection::Descending => a > b,
    }
}

/// Whether emitting `next` as a watermark advances the frontier past the
/// previously emitted one. Ascending frontiers strictly increase,
/// descending strictly decrease; the first watermark always advances.
fn frontier_advanced(prev: Option<u64>, next: u64, direction: ScanDirection) -> bool {
    match prev {
        None => true,
        Some(prev) => match direction {
            ScanDirection::Ascending => next > prev,
            ScanDirection::Descending => next < prev,
        },
    }
}

/// Clamped member-id edges of `bucket` in scan direction: `(pre, post)` where
/// `pre` is the leading edge (everything before it is already covered) and
/// `post` is the trailing edge (everything up to and including the bucket is
/// covered). Ascending: `(low, high)`; descending: `(high, low)`. Both clamped
/// to the request range so cursors stay in-bounds when they round-trip into a
/// follow-up request with a different range.
pub(crate) fn bucket_edges(
    bucket: u64,
    bucket_size: u64,
    range: &Range<u64>,
    direction: ScanDirection,
) -> (u64, u64) {
    let start = bucket.saturating_mul(bucket_size);
    let end = start.saturating_add(bucket_size);
    match direction {
        ScanDirection::Ascending => (start.max(range.start), end.min(range.end)),
        ScanDirection::Descending => (end.min(range.end), start.max(range.start)),
    }
}

/// Pull the snapshotted bitmap for leaf `i` to give to one referencing term
/// this round. Returns `None` if the leaf isn't on the floor bucket (so the
/// term short-circuits). When `remaining_refs[i] > 1`, the bitmap is cloned so
/// other referencing terms still see it; the last reference takes by value to
/// avoid an unnecessary copy in the common single-reference case.
pub(crate) fn take_snapshot_bitmap(
    snapshot: &mut [Option<RoaringBitmap>],
    remaining_refs: &mut [usize],
    on_floor: &[bool],
    i: usize,
) -> Option<RoaringBitmap> {
    if !on_floor[i] {
        return None;
    }
    if remaining_refs[i] > 1 {
        remaining_refs[i] -= 1;
        snapshot[i].clone()
    } else {
        remaining_refs[i] = remaining_refs[i].saturating_sub(1);
        snapshot[i].take()
    }
}

/// Per-round refcount: how many satisfiable term-side slots reference each
/// on-floor leaf. Drives [`take_snapshot_bitmap`]'s take-vs-clone decision so
/// the last referencing slot reclaims the bitmap by value.
pub(crate) fn count_on_floor_refs(terms: &[TermSpec], on_floor: &[bool]) -> Vec<usize> {
    let mut refs = vec![0usize; on_floor.len()];
    for term in terms {
        if term.unsatisfiable {
            continue;
        }
        for &i in term.includes.iter().chain(term.excludes.iter()) {
            if on_floor[i] {
                refs[i] += 1;
            }
        }
    }
    refs
}

/// Recompute leaf liveness from current term state. A leaf becomes
/// `unreferenced` when no satisfiable term still points at it, or when its
/// head is at EOF (the bucket stream is permanently exhausted; any further
/// peek would be wasted work, and any include-referencing term will be marked
/// `unsatisfiable` separately).
///
/// `unreferenced` is monotonic — entries only transition false → true — so
/// this is safe to invoke each round; previously-retired leaves stay retired.
pub(crate) fn recompute_unreferenced(
    terms: &[TermSpec],
    class: &[Option<LeafHead>],
    unreferenced: &mut [bool],
) {
    let leaf_count = unreferenced.len();
    let mut referenced = vec![false; leaf_count];
    for term in terms {
        if term.unsatisfiable {
            continue;
        }
        for &i in term.includes.iter().chain(term.excludes.iter()) {
            if !unreferenced[i] && !matches!(class[i], Some(LeafHead::Eof)) {
                referenced[i] = true;
            }
        }
    }
    for i in 0..leaf_count {
        if !referenced[i] {
            unreferenced[i] = true;
        }
    }
}

/// Evaluate one DNF term at a single bucket from the per-leaf bitmaps present
/// there: intersect the includes (any absent include ⇒ empty term), then
/// subtract the union of the present excludes (`a AND NOT b`). Returns the
/// term's matches at the bucket, or `None` if empty. Bitmaps are taken by value
/// so the caller hands over the consumed leaf rows without cloning.
pub(crate) fn eval_term_at_bucket(
    includes: Vec<Option<RoaringBitmap>>,
    excludes: Vec<Option<RoaringBitmap>>,
) -> Option<RoaringBitmap> {
    let mut acc: Option<RoaringBitmap> = None;
    for include in includes {
        // A missing include means the intersection is empty at this bucket.
        let bitmap = include?;
        acc = Some(match acc {
            None => bitmap,
            Some(a) => a & bitmap,
        });
    }
    // Anchored terms always carry at least one include, so `acc` is `Some`.
    let mut acc = acc?;
    for exclude in excludes.into_iter().flatten() {
        acc -= exclude;
    }
    (!acc.is_empty()).then_some(acc)
}

/// One DNF term, as index spans into the flat leaf vector. Shared by the stream
/// and iterator drivers.
pub(crate) struct TermSpec {
    pub(crate) includes: Vec<usize>,
    pub(crate) excludes: Vec<usize>,
    /// Set once any include leaf hits EOF: the term's intersection is
    /// permanently empty (it can never match again). Latched.
    pub(crate) unsatisfiable: bool,
}

/// A leaf's head this round, from a non-consuming peek.
pub(crate) enum LeafHead {
    Bucket(u64),
    Eof,
    Error,
}
/// Compute the furthest bucket each active leaf can skip to without changing
/// query results.
pub(crate) fn leaf_skip_targets(
    terms: &[TermSpec],
    class: &[Option<LeafHead>],
    unreferenced: &[bool],
    direction: ScanDirection,
) -> Vec<Option<u64>> {
    // Go through each conjunction in the top-level query disjunction.
    let term_not_before: Vec<Option<u64>> = terms
        .iter()
        .map(|term| {
            if term.unsatisfiable {
                return None;
            }
            // For each conjunction, find its furthest-ahead include leaf.
            // As far as this conjunction is concerned, every leaf it references
            // can skip to that bucket. A shared leaf may still be constrained by
            // another conjunction. Excludes don't set this bound. No exclude row
            // just means nothing to subtract.
            term.includes
                .iter()
                .filter_map(|&i| match class.get(i).and_then(Option::as_ref) {
                    Some(LeafHead::Bucket(bucket)) => Some(*bucket),
                    Some(LeafHead::Error | LeafHead::Eof) | None => None,
                })
                .reduce(|a, b| advance_in_direction(a, b, direction))
        })
        .collect();

    // Now with term-level knowledge, iterate through the deduped leaves. Find the nearest
    // safe bound that we can skip ahead to among all terms that reference a leaf.
    class
        .iter()
        .enumerate()
        .map(|(i, head)| {
            if unreferenced.get(i).copied().unwrap_or(true) {
                return None;
            }
            let Some(LeafHead::Bucket(head)) = head else {
                return None;
            };

            let mut target = None;
            for (term_index, term) in terms.iter().enumerate() {
                if term.unsatisfiable
                    || !term
                        .includes
                        .iter()
                        .chain(&term.excludes)
                        .any(|&leaf| leaf == i)
                {
                    continue;
                }

                let not_before = term_not_before[term_index]?;
                target = Some(match target {
                    None => not_before,
                    Some(current) => bound_in_direction(current, not_before, direction),
                });
            }
            target.filter(|&target| strictly_before(*head, target, direction))
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod test_utils {
    use std::collections::BTreeMap;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::Mutex;

    use futures::StreamExt;
    use futures::stream;

    use super::*;

    pub(crate) const BUCKET_SIZE: u64 = 100_000;
    pub(crate) type TestBuckets = BTreeMap<Vec<u8>, Vec<(u64, Vec<u32>)>>;
    type SeekRecorder = (Vec<u8>, Arc<Mutex<HashMap<Vec<u8>, usize>>>);
    pub(crate) struct VecBucketIter {
        items: std::vec::IntoIter<BucketItem>,
        direction: ScanDirection,
        recorder: Option<SeekRecorder>,
    }

    impl Iterator for VecBucketIter {
        type Item = BucketItem;

        fn next(&mut self) -> Option<Self::Item> {
            self.items.next()
        }
    }

    impl SeekableBucketIterator for VecBucketIter {
        fn seek_bucket(&mut self, target: u64) {
            if let Some((key, seek_counts)) = &self.recorder {
                *seek_counts.lock().unwrap().entry(key.clone()).or_insert(0) += 1;
            }
            while matches!(
                self.items.as_slice().first(),
                Some(Ok((bucket, _))) if strictly_before(*bucket, target, self.direction)
            ) {
                self.items.next();
            }
        }
    }

    #[derive(Clone)]
    pub(crate) struct TestBucketSource {
        pub(crate) buckets: Arc<TestBuckets>,
    }

    impl BitmapBucketSource for TestBucketSource {
        fn scan_bucket_stream(
            &self,
            dimension_key: Vec<u8>,
            range: Range<u64>,
            direction: ScanDirection,
        ) -> BucketStream {
            stream::iter(self.bucket_items(&dimension_key, range, direction)).boxed()
        }
    }

    impl<'a> BitmapBucketIteratorSource<'a> for TestBucketSource {
        type Iter = VecBucketIter;

        fn scan_bucket_iter(
            &self,
            dimension_key: Vec<u8>,
            range: Range<u64>,
            direction: ScanDirection,
        ) -> Self::Iter {
            VecBucketIter {
                items: self
                    .bucket_items(&dimension_key, range, direction)
                    .into_iter(),
                direction,
                recorder: None,
            }
        }
    }

    impl TestBucketSource {
        pub(crate) fn bucket_items(
            &self,
            dimension_key: &[u8],
            range: Range<u64>,
            direction: ScanDirection,
        ) -> Vec<BucketItem> {
            // Mirror the real backends: the tx-universe key is synthesized at
            // scan time, never read from stored buckets.
            if dimension_key == universe_key() {
                return dense_universe_buckets(range, BUCKET_SIZE, direction)
                    .map(Ok)
                    .collect();
            }
            let mut buckets = self.buckets.get(dimension_key).cloned().unwrap_or_default();
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
            buckets
                .into_iter()
                .map(|(bucket_id, bits)| Ok((bucket_id, make_bitmap(&bits))))
                .collect()
        }
    }

    pub(crate) fn make_bitmap(bits: &[u32]) -> RoaringBitmap {
        let mut bm = RoaringBitmap::new();
        for &b in bits {
            bm.insert(b);
        }
        bm
    }

    pub(crate) fn test_key(value: &[u8]) -> Vec<u8> {
        crate::dimensions::encode_dimension_key(crate::dimensions::IndexDimension::Sender, value)
    }

    pub(crate) fn universe_key() -> Vec<u8> {
        crate::dimensions::encode_dimension_key(
            crate::dimensions::IndexDimension::TxUniverse,
            crate::dimensions::TX_UNIVERSE_VALUE,
        )
    }

    pub(crate) fn include(value: &[u8]) -> BitmapLiteral {
        BitmapLiteral::include(test_key(value)).unwrap()
    }

    pub(crate) fn include_universe() -> BitmapLiteral {
        BitmapLiteral::include(universe_key()).unwrap()
    }

    /// Full `[0, BUCKET_SIZE)` bitmap — what the synthesized universe leaf
    /// yields per bucket.
    pub(crate) fn full_bucket() -> RoaringBitmap {
        let mut bm = RoaringBitmap::new();
        bm.insert_range(0..BUCKET_SIZE as u32);
        bm
    }

    /// A `TestBucketSource` that records how many times `scan_bucket_*` is
    /// invoked per dimension key. Used to verify the evaluator deduplicates
    /// leaves across terms (a key referenced from multiple terms should be
    /// scanned exactly once).
    #[derive(Clone)]
    pub(crate) struct CountingBucketSource {
        pub(crate) buckets: Arc<TestBuckets>,
        scan_counts: Arc<Mutex<HashMap<Vec<u8>, usize>>>,
        seek_counts: Arc<Mutex<HashMap<Vec<u8>, usize>>>,
    }

    impl CountingBucketSource {
        pub(crate) fn new(buckets: TestBuckets) -> Self {
            Self {
                buckets: Arc::new(buckets),
                scan_counts: Arc::new(Mutex::new(HashMap::new())),
                seek_counts: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        pub(crate) fn scan_count(&self, key: &[u8]) -> usize {
            self.scan_counts
                .lock()
                .unwrap()
                .get(key)
                .copied()
                .unwrap_or(0)
        }
        pub(crate) fn seek_count(&self, key: &[u8]) -> usize {
            self.seek_counts
                .lock()
                .unwrap()
                .get(key)
                .copied()
                .unwrap_or(0)
        }

        fn record(&self, key: &[u8]) {
            *self
                .scan_counts
                .lock()
                .unwrap()
                .entry(key.to_vec())
                .or_insert(0) += 1;
        }

        fn bucket_items(
            &self,
            dimension_key: &[u8],
            range: Range<u64>,
            direction: ScanDirection,
        ) -> Vec<BucketItem> {
            if dimension_key == universe_key() {
                return dense_universe_buckets(range, BUCKET_SIZE, direction)
                    .map(Ok)
                    .collect();
            }
            let mut buckets = self.buckets.get(dimension_key).cloned().unwrap_or_default();
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
            buckets
                .into_iter()
                .map(|(bucket_id, bits)| Ok((bucket_id, make_bitmap(&bits))))
                .collect()
        }
    }

    impl BitmapBucketSource for CountingBucketSource {
        fn scan_bucket_stream(
            &self,
            dimension_key: Vec<u8>,
            range: Range<u64>,
            direction: ScanDirection,
        ) -> BucketStream {
            self.record(&dimension_key);
            stream::iter(self.bucket_items(&dimension_key, range, direction)).boxed()
        }
    }

    impl<'a> BitmapBucketIteratorSource<'a> for CountingBucketSource {
        type Iter = VecBucketIter;

        fn scan_bucket_iter(
            &self,
            dimension_key: Vec<u8>,
            range: Range<u64>,
            direction: ScanDirection,
        ) -> Self::Iter {
            self.record(&dimension_key);
            VecBucketIter {
                items: self
                    .bucket_items(&dimension_key, range, direction)
                    .into_iter(),
                direction,
                recorder: Some((dimension_key, self.seek_counts.clone())),
            }
        }
    }

    pub(crate) fn exclude(value: &[u8]) -> BitmapLiteral {
        BitmapLiteral::exclude(test_key(value)).unwrap()
    }

    /// A [`TestBucketSource`] whose `stop_key` leaf yields `stop()` after its
    /// first `rows_before_stop` bucket rows in scan order, in place of the
    /// rest of its rows.
    #[derive(Clone)]
    pub(crate) struct StoppingBucketSource {
        pub(crate) inner: TestBucketSource,
        pub(crate) stop_key: Vec<u8>,
        pub(crate) rows_before_stop: usize,
        pub(crate) stop: fn() -> LeafStop,
    }

    impl StoppingBucketSource {
        fn bucket_items(
            &self,
            dimension_key: &[u8],
            range: Range<u64>,
            direction: ScanDirection,
        ) -> Vec<BucketItem> {
            let mut items = self.inner.bucket_items(dimension_key, range, direction);
            if dimension_key == self.stop_key {
                items.truncate(self.rows_before_stop);
                items.push(Err((self.stop)()));
            }
            items
        }
    }

    impl BitmapBucketSource for StoppingBucketSource {
        fn scan_bucket_stream(
            &self,
            dimension_key: Vec<u8>,
            range: Range<u64>,
            direction: ScanDirection,
        ) -> BucketStream {
            stream::iter(self.bucket_items(&dimension_key, range, direction)).boxed()
        }
    }

    impl<'a> BitmapBucketIteratorSource<'a> for StoppingBucketSource {
        type Iter = VecBucketIter;

        fn scan_bucket_iter(
            &self,
            dimension_key: Vec<u8>,
            range: Range<u64>,
            direction: ScanDirection,
        ) -> Self::Iter {
            VecBucketIter {
                items: self
                    .bucket_items(&dimension_key, range, direction)
                    .into_iter(),
                direction,
                recorder: None,
            }
        }
    }

    /// Leaves `a` (member 1) and `b` (member 2) with a row in every bucket of
    /// `0..bucket_count`; `b` also holds member 1 in `matching_buckets`, the
    /// only buckets [`parallel_leaves_query`] matches. Neither leaf lags, so
    /// each bucket charges exactly two rows.
    pub(crate) fn parallel_leaves_source(
        bucket_count: u64,
        matching_buckets: &[u64],
    ) -> TestBucketSource {
        const MATCHED_MEMBERS: &[u32] = &[1];
        let matches: Vec<(u64, &[u32])> = matching_buckets
            .iter()
            .map(|&bucket| (bucket, MATCHED_MEMBERS))
            .collect();
        parallel_leaves_source_with_matches(bucket_count, &matches)
    }

    /// [`parallel_leaves_source`] where [`parallel_leaves_query`] matches
    /// exactly the listed members (relative to the bucket start) of each
    /// listed bucket: both leaves hold them in addition to their own member.
    pub(crate) fn parallel_leaves_source_with_matches(
        bucket_count: u64,
        matches: &[(u64, &[u32])],
    ) -> TestBucketSource {
        let leaf_rows = |own_member: u32| -> Vec<(u64, Vec<u32>)> {
            (0..bucket_count)
                .map(|bucket| {
                    let mut members = vec![own_member];
                    for (_, matched) in matches.iter().filter(|(b, _)| *b == bucket) {
                        members.extend_from_slice(matched);
                    }
                    (bucket, members)
                })
                .collect()
        };
        TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (test_key(b"a"), leaf_rows(1)),
                (test_key(b"b"), leaf_rows(2)),
            ])),
        }
    }

    /// A [`TestBucketSource`] that counts the scan work its scans do: the
    /// rows they yield (until the budget runs out this equals
    /// [`BitmapScanMetrics::buckets_evaluated`]) and the leaf scans reopened
    /// ([`BitmapScanMetrics::leaf_seeks`]). A stream scan opened again for a
    /// key it has already opened, or a seek of an iterator scan, is a reopen.
    #[derive(Clone)]
    pub(crate) struct ScanWorkCountingSource {
        inner: TestBucketSource,
        rows_yielded: Arc<AtomicU64>,
        reopens: Arc<AtomicU64>,
        opened_keys: Arc<Mutex<HashSet<Vec<u8>>>>,
    }

    impl ScanWorkCountingSource {
        pub(crate) fn new(inner: TestBucketSource) -> Self {
            Self {
                inner,
                rows_yielded: Arc::new(AtomicU64::new(0)),
                reopens: Arc::new(AtomicU64::new(0)),
                opened_keys: Arc::new(Mutex::new(HashSet::new())),
            }
        }

        pub(crate) fn rows_yielded(&self) -> u64 {
            self.rows_yielded.load(Ordering::SeqCst)
        }

        pub(crate) fn reopens(&self) -> u64 {
            self.reopens.load(Ordering::SeqCst)
        }
    }

    impl BitmapBucketSource for ScanWorkCountingSource {
        fn scan_bucket_stream(
            &self,
            dimension_key: Vec<u8>,
            range: Range<u64>,
            direction: ScanDirection,
        ) -> BucketStream {
            let first_open = self
                .opened_keys
                .lock()
                .unwrap()
                .insert(dimension_key.clone());
            if !first_open {
                self.reopens.fetch_add(1, Ordering::SeqCst);
            }
            let rows_yielded = self.rows_yielded.clone();
            self.inner
                .scan_bucket_stream(dimension_key, range, direction)
                .inspect(move |row| {
                    if row.is_ok() {
                        rows_yielded.fetch_add(1, Ordering::SeqCst);
                    }
                })
                .boxed()
        }
    }

    impl<'a> BitmapBucketIteratorSource<'a> for ScanWorkCountingSource {
        type Iter = ScanWorkCountingIter;

        fn scan_bucket_iter(
            &self,
            dimension_key: Vec<u8>,
            range: Range<u64>,
            direction: ScanDirection,
        ) -> Self::Iter {
            ScanWorkCountingIter {
                inner: self.inner.scan_bucket_iter(dimension_key, range, direction),
                rows_yielded: self.rows_yielded.clone(),
                reopens: self.reopens.clone(),
            }
        }
    }

    pub(crate) struct ScanWorkCountingIter {
        inner: VecBucketIter,
        rows_yielded: Arc<AtomicU64>,
        reopens: Arc<AtomicU64>,
    }

    impl Iterator for ScanWorkCountingIter {
        type Item = BucketItem;

        fn next(&mut self) -> Option<BucketItem> {
            let row = self.inner.next();
            if matches!(row, Some(Ok(_))) {
                self.rows_yielded.fetch_add(1, Ordering::SeqCst);
            }
            row
        }
    }

    impl SeekableBucketIterator for ScanWorkCountingIter {
        fn seek_bucket(&mut self, target: u64) {
            self.reopens.fetch_add(1, Ordering::SeqCst);
            self.inner.seek_bucket(target);
        }
    }

    /// `a AND b` over [`parallel_leaves_source`].
    pub(crate) fn parallel_leaves_query() -> BitmapQuery {
        BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b")]).unwrap(),
        ])
        .unwrap()
    }

    /// Pacing that holds no watermark: the evaluation's own output.
    pub(crate) const EVERY_WATERMARK: WatermarkPacing = WatermarkPacing {
        min_buckets: 0,
        emit_held_at_end: false,
    };

    pub(crate) fn charged_buckets(min_buckets: u64, emit_held_at_end: bool) -> WatermarkPacing {
        WatermarkPacing {
            min_buckets,
            emit_held_at_end,
        }
    }

    /// Comparable form of a [`ScanStop`].
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(crate) enum ObservedStop {
        ScanLimit(u64),
        Cancelled,
        Fault,
    }

    /// Comparable form of one member-level evaluator output.
    pub(crate) type ObservedMemberOutput = Result<Watermarked<u64>, ObservedStop>;

    pub(crate) fn observe_member_outputs(
        outputs: Vec<Result<Watermarked<u64>, ScanStop>>,
    ) -> Vec<ObservedMemberOutput> {
        outputs
            .into_iter()
            .map(|output| {
                output.map_err(|stop| match stop {
                    ScanStop::ScanLimit { scan_frontier } => ObservedStop::ScanLimit(scan_frontier),
                    ScanStop::Cancelled => ObservedStop::Cancelled,
                    ScanStop::Fault(_) => ObservedStop::Fault,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::BUCKET_SIZE;
    use super::test_utils::exclude;
    use super::test_utils::include;
    use super::test_utils::make_bitmap;
    use super::*;

    fn paced_watermark(position: u64) -> WatermarkedBucket {
        Ok(Watermarked::Watermark(position))
    }

    /// An item of bucket 5 with a match inside the pacers' scan range.
    fn paced_item() -> WatermarkedBucket {
        Ok(Watermarked::Item((5, make_bitmap(&[1]))))
    }

    /// Scan work adds rows charged and, per reopen step, the skip policy's
    /// drain probe rows: here 100 of work is due, and a reopen step costs 50.
    /// Neither 60 rows nor one reopen step alone is due, but together they
    /// are. An item with a match in range is an emission too, so it restarts
    /// the count.
    #[test]
    fn scan_work_adds_charged_rows_and_reopen_steps() {
        let budget = BitmapScanBudget::new(1_000);
        let pacing = WatermarkPacing {
            min_buckets: 100,
            emit_held_at_end: false,
        };
        let policy = SkipPolicy {
            drain_probe_rows: std::num::NonZeroU32::new(50),
        };
        let mut pacer = WatermarkPacer::new(
            pacing,
            policy,
            budget.clone(),
            0..(10 * BUCKET_SIZE),
            BUCKET_SIZE,
        );
        // (rows charged and reopen steps just before the output, the output,
        // whether it is emitted)
        let steps = [
            (0, 0, paced_watermark(1), true),
            (60, 0, paced_watermark(2), false),
            (0, 1, paced_watermark(3), true),
            (0, 1, paced_watermark(4), false),
            (0, 0, paced_item(), true),
            // 100 of work since the last watermark, but 50 since the item.
            (0, 1, paced_watermark(6), false),
            (49, 0, paced_watermark(7), false),
            (1, 0, paced_watermark(8), true),
        ];
        for (step, (rows, reopen_steps, output, emitted)) in steps.into_iter().enumerate() {
            for _ in 0..rows {
                assert!(budget.try_take());
            }
            for _ in 0..reopen_steps {
                budget.note_reopen_step();
            }
            let paced = pacer.pace(output);
            assert_eq!(paced.is_some(), emitted, "step {step}: {paced:?}");
        }
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

    #[test]
    fn dense_universe_buckets_covers_range_in_direction_order() {
        use super::test_utils::BUCKET_SIZE;

        // Partial edge buckets still yield full bitmaps — trimming happens at
        // the flatten step.
        let asc: Vec<u64> =
            dense_universe_buckets(150_000..350_001, BUCKET_SIZE, ScanDirection::Ascending)
                .map(|(bucket, bitmap)| {
                    assert_eq!(bitmap.len(), BUCKET_SIZE);
                    bucket
                })
                .collect();
        assert_eq!(asc, vec![1, 2, 3]);

        let desc: Vec<u64> =
            dense_universe_buckets(150_000..350_001, BUCKET_SIZE, ScanDirection::Descending)
                .map(|(bucket, _)| bucket)
                .collect();
        assert_eq!(desc, vec![3, 2, 1]);

        let single: Vec<u64> = dense_universe_buckets(5..6, BUCKET_SIZE, ScanDirection::Ascending)
            .map(|(bucket, _)| bucket)
            .collect();
        assert_eq!(single, vec![0]);

        assert_eq!(
            dense_universe_buckets(7..7, BUCKET_SIZE, ScanDirection::Ascending).count(),
            0
        );
    }

    #[test]
    fn unique_leaf_count_counts_distinct_keys_across_terms() {
        // Two terms both include `a` and `b`; only `c` is unique to term 1
        // and `d` to term 2. The raw literal count is 6, but only 4 unique
        // dimension keys are scanned at eval time.
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b"), include(b"c")]).unwrap(),
            BitmapTerm::new(vec![include(b"a"), include(b"b"), include(b"d")]).unwrap(),
        ])
        .unwrap();
        assert_eq!(query.unique_leaf_count(), 4);
    }

    #[test]
    fn build_term_specs_collapses_duplicate_keys_to_one_leaf() {
        // Same key used as include in one term and exclude in another should
        // share a single leaf slot, with each term referring to it by the
        // same index.
        let terms = vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b")]).unwrap(),
            BitmapTerm::new(vec![include(b"b"), exclude(b"a")]).unwrap(),
        ];
        let DedupedQuery { keys, terms: specs } = build_term_specs(terms);
        assert_eq!(keys.len(), 2, "only `a` and `b` are unique");
        // Term 0: include a (slot 0), include b (slot 1).
        assert_eq!(specs[0].includes, vec![0, 1]);
        assert!(specs[0].excludes.is_empty());
        // Term 1: include b (slot 1), exclude a (slot 0) — same slots as term 0.
        assert_eq!(specs[1].includes, vec![1]);
        assert_eq!(specs[1].excludes, vec![0]);
    }
    #[test]
    fn leaf_skip_targets_advance_lagging_include_in_both_directions() {
        let term = TermSpec {
            includes: vec![0, 1],
            excludes: vec![],
            unsatisfiable: false,
        };
        for (direction, heads, expected) in [
            (ScanDirection::Ascending, [2, 10], vec![Some(10), None]),
            (ScanDirection::Descending, [10, 2], vec![Some(2), None]),
        ] {
            let class = heads.map(|bucket| Some(LeafHead::Bucket(bucket)));
            assert_eq!(
                leaf_skip_targets(std::slice::from_ref(&term), &class, &[false; 2], direction),
                expected
            );
        }
    }

    #[test]
    fn leaf_skip_targets_use_least_candidate_across_shared_terms() {
        let terms = [
            TermSpec {
                includes: vec![0],
                excludes: vec![1],
                unsatisfiable: false,
            },
            TermSpec {
                includes: vec![1],
                excludes: vec![],
                unsatisfiable: false,
            },
        ];
        for (direction, heads) in [
            (ScanDirection::Ascending, [50, 10]),
            (ScanDirection::Descending, [10, 50]),
        ] {
            let class = heads.map(|bucket| Some(LeafHead::Bucket(bucket)));
            assert_eq!(
                leaf_skip_targets(&terms, &class, &[false; 2], direction)[1],
                None
            );
        }
    }

    #[test]
    fn leaf_skip_targets_drag_exclude_to_include_candidate() {
        let term = TermSpec {
            includes: vec![0],
            excludes: vec![1],
            unsatisfiable: false,
        };
        for (direction, heads, expected) in [
            (ScanDirection::Ascending, [50, 10], Some(50)),
            (ScanDirection::Descending, [10, 50], Some(10)),
        ] {
            let class = heads.map(|bucket| Some(LeafHead::Bucket(bucket)));
            assert_eq!(
                leaf_skip_targets(std::slice::from_ref(&term), &class, &[false; 2], direction)[1],
                expected
            );
        }
    }

    #[test]
    fn leaf_skip_targets_drop_error_heads_and_poison_unknown_candidates() {
        for (direction, a_head, c_head) in [
            (ScanDirection::Ascending, 0, 9),
            (ScanDirection::Descending, 9, 0),
        ] {
            let terms = [
                TermSpec {
                    includes: vec![0, 1],
                    excludes: vec![],
                    unsatisfiable: false,
                },
                TermSpec {
                    includes: vec![0, 2],
                    excludes: vec![],
                    unsatisfiable: false,
                },
            ];
            let class = [
                Some(LeafHead::Bucket(a_head)),
                Some(LeafHead::Error),
                Some(LeafHead::Bucket(c_head)),
            ];
            assert_eq!(
                leaf_skip_targets(&terms, &class, &[false; 3], direction)[0],
                None
            );

            let poisoned_term = TermSpec {
                includes: vec![1],
                excludes: vec![0],
                unsatisfiable: false,
            };
            assert_eq!(
                leaf_skip_targets(&[poisoned_term], &class, &[false; 3], direction)[0],
                None
            );
        }
    }
}
