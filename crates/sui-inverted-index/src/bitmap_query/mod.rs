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
pub use stream::intersect_n;
pub use stream::subtract_two;
pub use stream::union_n;

// Cross-checked against the iterative evaluator in iter.rs tests.
#[cfg(test)]
pub(crate) use stream::BitmapScanBudget;
#[cfg(test)]
pub(crate) use stream::eval_bitmap_query_bucket_stream;

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

/// A stream of `(bucket_id, RoaringBitmap)` in the requested bucket order.
/// Bitmap positions are **relative** to the bucket (u32 offsets `[0, BUCKET_SIZE)`)
/// - edge trimming against the requested range happens at the flatten step.
pub type BucketItem = Result<(u64, RoaringBitmap)>;
pub type BucketStream = BoxStream<'static, BucketItem>;

/// A bucket stream that interleaves data buckets with per-source progress
/// watermarks. Combinators (`intersect_n`, `union_n`, `subtract_two`)
/// merge child watermarks structurally so the output always reflects
/// "every source has scanned past P."
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

/// The less-advanced of two frontier positions in scan direction: the min
/// ascending, the max descending. Used to keep a merged frontier bounded by
/// the slowest contributor.
fn bound_in_direction(a: u64, b: u64, direction: ScanDirection) -> u64 {
    match direction {
        ScanDirection::Ascending => a.min(b),
        ScanDirection::Descending => a.max(b),
    }
}

/// Like [`merge_watermarks`] but also folds in a `ceiling` frozen from
/// errored (retired) union branches. A retired branch no longer polls, but
/// the position it reached still bounds how far the union can claim progress,
/// so it stays in the merge as a fixed floor. Live slots dominate as usual
/// (any `None` live slot blocks progress); with no live slots left the
/// ceiling alone is the frontier.
fn merge_with_ceiling(
    slots: &[Option<u64>],
    ceiling: Option<u64>,
    direction: ScanDirection,
) -> Option<u64> {
    // A live branch that has not yet reported blocks progress past its
    // unknown floor.
    if slots.iter().any(Option::is_none) {
        return None;
    }
    let live = match direction {
        ScanDirection::Ascending => slots.iter().flatten().copied().min(),
        ScanDirection::Descending => slots.iter().flatten().copied().max(),
    };
    match (live, ceiling) {
        (Some(l), Some(c)) => Some(bound_in_direction(l, c, direction)),
        (Some(l), None) => Some(l),
        (None, c) => c,
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
