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

use anyhow::Result;
use anyhow::bail;
use futures::stream::BoxStream;
use roaring::RoaringBitmap;

use crate::dimensions::IndexDimension;

mod iter;
mod stream;

pub use iter::eval_bitmap_query_bucket_iter;
pub use stream::BitmapScanMetrics;
pub use stream::eval_bitmap_query_stream;
pub use stream::flatten_watermarked_buckets;

// Cross-checked against the iterative evaluator in iter.rs tests.
#[cfg(test)]
pub(crate) use stream::BitmapScanBudget;
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

impl ScanDirection {
    pub fn is_ascending(self) -> bool {
        matches!(self, Self::Ascending)
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
) -> impl Iterator<Item = (u64, RoaringBitmap)> + Send + 'static {
    let bits = u32::try_from(bucket_size).expect("bucket size fits in u32");
    let mut buckets = if range.is_empty() {
        0..0
    } else {
        (range.start / bucket_size)..(range.end - 1) / bucket_size + 1
    };
    std::iter::from_fn(move || {
        let bucket = match direction {
            ScanDirection::Ascending => buckets.next(),
            ScanDirection::Descending => buckets.next_back(),
        }?;
        let mut bitmap = RoaringBitmap::new();
        bitmap.insert_range(0..bits);
        Some((bucket, bitmap))
    })
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

/// Storage backend that can scan one bitmap dimension key synchronously.
///
/// This is for request-local backends such as RocksDB, where the bucket scan
/// naturally owns or borrows a synchronous iterator. The iterator evaluator is
/// fully synchronous so these iterators can stay on the blocking task that owns
/// them.
pub trait BitmapBucketIteratorSource<'a>: Clone + 'a {
    type Iter: Iterator<Item = BucketItem> + 'a;

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
        type Iter = std::vec::IntoIter<BucketItem>;

        fn scan_bucket_iter(
            &self,
            dimension_key: Vec<u8>,
            range: Range<u64>,
            direction: ScanDirection,
        ) -> Self::Iter {
            self.bucket_items(&dimension_key, range, direction)
                .into_iter()
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
    }

    impl CountingBucketSource {
        pub(crate) fn new(buckets: TestBuckets) -> Self {
            Self {
                buckets: Arc::new(buckets),
                scan_counts: Arc::new(Mutex::new(HashMap::new())),
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
        type Iter = std::vec::IntoIter<BucketItem>;

        fn scan_bucket_iter(
            &self,
            dimension_key: Vec<u8>,
            range: Range<u64>,
            direction: ScanDirection,
        ) -> Self::Iter {
            self.record(&dimension_key);
            self.bucket_items(&dimension_key, range, direction)
                .into_iter()
        }
    }

    pub(crate) fn exclude(value: &[u8]) -> BitmapLiteral {
        BitmapLiteral::exclude(test_key(value)).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::exclude;
    use super::test_utils::include;
    use super::*;

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
}
