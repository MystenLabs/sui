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
//! Backends provide one ordered `(bucket_id, RoaringBitmap)` stream or iterator
//! per dimension key. The merge-join machinery here is storage-agnostic:
//! BigTable, RocksDB, or any other backend can reuse it as long as its bucket
//! source is sparse, ordered by the requested scan direction, and stores bitmap
//! positions relative to that bucket.

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
pub use stream::buckets_with_watermarks;
pub use stream::eval_bitmap_query_stream;
pub use stream::flatten_watermarked_buckets;

// Cross-checked against the iterative evaluator in iter.rs tests.
#[cfg(test)]
pub(crate) use stream::BitmapScanBudget;
#[cfg(test)]
pub(crate) use stream::eval_bitmap_query_bucket_stream;

/// Terminal signal: the per-request bucket-fetch budget is exhausted.
/// Surfaced as `anyhow::Error` so it short-circuits `try_stream!`
/// pipelines through the existing error path. A silent EOF would be
/// indistinguishable from a leaf reaching the range terminus, so the
/// driver would advance the cursor to the end and claim full coverage
/// instead of truncating the scan at the current floor.
#[derive(Debug, thiserror::Error)]
#[error("bitmap scan budget exhausted")]
pub struct BitmapScanLimitExceeded;

/// Aggregate of multiple terminal errors raised in the same driver round
/// (e.g. two leaves both exhaust the shared budget before the next round).
/// The single-error shortcut returns the inner error directly so existing
/// downcasts on the wire keep working — `MultiError` only appears when 2+
/// errors coincide, in which case downstream consumers should use
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
/// at sites that may receive aggregated errors from the bitmap eval
/// driver.
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

/// A stream of `(bucket_id, RoaringBitmap)` in the requested bucket order.
/// Bitmap positions are **relative** to the bucket (u32 offsets `[0, BUCKET_SIZE)`)
/// - edge trimming against the requested range happens at the flatten step.
pub type BucketItem = Result<(u64, RoaringBitmap)>;
pub type BucketStream = BoxStream<'static, BucketItem>;

/// A bucket stream that interleaves data buckets with progress watermarks.
/// The flat DNF driver derives each watermark from the slowest leaf's
/// position, so the output always reflects "every source has scanned past P."
pub(crate) type WatermarkedBucket = Result<Watermarked<(u64, RoaringBitmap)>>;
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

fn split_term_literals(term: BitmapTerm) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let mut include = Vec::new();
    let mut exclude = Vec::new();
    for literal in term.literals {
        match literal {
            BitmapLiteral::Include(key) => include.push(key.into_inner()),
            BitmapLiteral::Exclude(key) => exclude.push(key.into_inner()),
        }
    }
    (include, exclude)
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

#[cfg(test)]
mod test_utils {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use futures::StreamExt;
    use futures::stream;

    use super::*;

    pub(super) const BUCKET_SIZE: u64 = 100_000;
    pub(super) type TestBuckets = BTreeMap<Vec<u8>, Vec<(u64, Vec<u32>)>>;

    #[derive(Clone)]
    pub(super) struct TestBucketSource {
        pub(super) buckets: Arc<TestBuckets>,
    }

    impl BitmapBucketSource for TestBucketSource {
        fn scan_bucket_stream(
            &self,
            dimension_key: Vec<u8>,
            _range: Range<u64>,
            direction: ScanDirection,
        ) -> BucketStream {
            stream::iter(self.bucket_items(&dimension_key, direction)).boxed()
        }
    }

    impl<'a> BitmapBucketIteratorSource<'a> for TestBucketSource {
        type Iter = std::vec::IntoIter<BucketItem>;

        fn scan_bucket_iter(
            &self,
            dimension_key: Vec<u8>,
            _range: Range<u64>,
            direction: ScanDirection,
        ) -> Self::Iter {
            self.bucket_items(&dimension_key, direction).into_iter()
        }
    }

    impl TestBucketSource {
        pub(super) fn bucket_items(
            &self,
            dimension_key: &[u8],
            direction: ScanDirection,
        ) -> Vec<BucketItem> {
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

    pub(super) fn make_bitmap(bits: &[u32]) -> RoaringBitmap {
        let mut bm = RoaringBitmap::new();
        for &b in bits {
            bm.insert(b);
        }
        bm
    }

    pub(super) fn test_key(value: &[u8]) -> Vec<u8> {
        crate::dimensions::encode_dimension_key(crate::dimensions::IndexDimension::Sender, value)
    }

    pub(super) fn include(value: &[u8]) -> BitmapLiteral {
        BitmapLiteral::include(test_key(value)).unwrap()
    }

    pub(super) fn exclude(value: &[u8]) -> BitmapLiteral {
        BitmapLiteral::exclude(test_key(value)).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::exclude;
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
}
