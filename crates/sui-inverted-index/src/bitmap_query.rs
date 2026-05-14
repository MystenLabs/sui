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

use anyhow::Result;
use anyhow::bail;
use futures::Stream;
use futures::StreamExt;
use futures::stream::BoxStream;
use futures::stream::Peekable;
use itertools::Itertools;
use roaring::RoaringBitmap;

use crate::dimensions::IndexDimension;

/// A stream of `(bucket_id, RoaringBitmap)` in the requested bucket order.
/// Bitmap positions are **relative** to the bucket (u32 offsets `[0, BUCKET_SIZE)`)
/// - edge trimming against the requested range happens at the flatten step.
type BucketItem = Result<(u64, RoaringBitmap)>;
pub type BucketStream = BoxStream<'static, BucketItem>;

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
/// This first evaluates the query into ordered bucket bitmaps, then flattens
/// each relative bucket bitmap into absolute member ids inside `range`.
pub fn eval_bitmap_query_stream<S>(
    source: S,
    query: BitmapQuery,
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
) -> BoxStream<'static, Result<u64>>
where
    S: BitmapBucketSource,
{
    let stream = eval_bitmap_query_bucket_stream(source, query, range.clone(), direction);
    flatten_bucket_stream(stream, range, bucket_size, direction).boxed()
}

/// Evaluate a DNF `BitmapQuery` as an ordered stream of relative bucket bitmaps.
pub fn eval_bitmap_query_bucket_stream<S>(
    source: S,
    query: BitmapQuery,
    range: Range<u64>,
    direction: ScanDirection,
) -> BucketStream
where
    S: BitmapBucketSource,
{
    let streams: Vec<BucketStream> = query
        .terms
        .into_iter()
        .map(|term| term_bucket_stream(source.clone(), term, range.clone(), direction).boxed())
        .collect();
    union_n(streams, direction).boxed()
}

/// Evaluate one DNF term: intersect all includes, then subtract excludes.
fn term_bucket_stream<S>(
    source: S,
    term: BitmapTerm,
    range: Range<u64>,
    direction: ScanDirection,
) -> impl Stream<Item = BucketItem> + Send + 'static
where
    S: BitmapBucketSource,
{
    let mut include = Vec::new();
    let mut exclude = Vec::new();
    for literal in term.literals {
        match literal {
            BitmapLiteral::Include(key) => include.push(key.into_inner()),
            BitmapLiteral::Exclude(key) => exclude.push(key.into_inner()),
        }
    }

    let include_streams: Vec<BucketStream> = include
        .into_iter()
        .map(|key| source.scan_bucket_stream(key, range.clone(), direction))
        .collect();

    let include_stream = intersect_n(include_streams, direction);

    let exclude_streams: Vec<BucketStream> = exclude
        .into_iter()
        .map(|key| source.scan_bucket_stream(key, range.clone(), direction))
        .collect();
    let exclude_stream = union_n(exclude_streams, direction);

    // Stream construction above is lazy. `subtract_two` polls both sides with
    // `try_join!`, so the include intersection and exclude union are opened/read
    // concurrently when this term stream is consumed.
    subtract_two(include_stream, exclude_stream, direction)
}

/// Multi-way merge intersection over ordered bucket streams. Emits only those
/// bucket_ids present in every child stream, with the bitwise AND of all their
/// bitmaps. Drops empty results.
pub fn intersect_n<S>(
    streams: Vec<S>,
    direction: ScanDirection,
) -> impl Stream<Item = BucketItem> + Send + 'static
where
    S: Stream<Item = BucketItem> + Send + Unpin + 'static,
{
    async_stream::try_stream! {
        if streams.is_empty() {
            return;
        }
        let mut children: Vec<Peekable<S>> =
            streams.into_iter().map(|s| s.peekable()).collect();

        loop {
            // Poll all child streams together so independent backend scans are
            // opened/read concurrently instead of serializing one dimension at a
            // time.
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
                    let (bid, bitmap) = Pin::new(child)
                        .next()
                        .await
                        .expect("peek reported a value")?;
                    debug_assert_eq!(bid, target_bucket);
                    acc = Some(match acc {
                        None => bitmap,
                        Some(a) => a & bitmap,
                    });
                }
                let bitmap = acc.expect("children non-empty");
                if !bitmap.is_empty() {
                    yield (target_bucket, bitmap);
                }
            } else {
                // Sparse bitmap rows encode only non-empty buckets. If a child
                // is behind the current alignment target, then at least one
                // other include stream is already past that bucket and has an
                // implicit all-zero bitmap there. The intersection for the
                // lagging bucket is empty, so consume it and re-peek.
                for (i, child) in children.iter_mut().enumerate() {
                    let drop_bucket = match direction {
                        ScanDirection::Ascending => peeks[i] < target_bucket,
                        ScanDirection::Descending => peeks[i] > target_bucket,
                    };
                    if drop_bucket {
                        let _ = Pin::new(child)
                            .next()
                            .await
                            .expect("peek reported a value")?;
                    }
                }
            }
        }
    }
}

/// Multi-way merge union over ordered bucket streams. Emits every bucket_id
/// produced by any child, with the bitwise OR of the bitmaps at that bucket.
pub fn union_n<S>(
    streams: Vec<S>,
    direction: ScanDirection,
) -> impl Stream<Item = BucketItem> + Send + 'static
where
    S: Stream<Item = BucketItem> + Send + Unpin + 'static,
{
    async_stream::try_stream! {
        if streams.is_empty() {
            return;
        }
        let mut children: Vec<Peekable<S>> =
            streams.into_iter().map(|s| s.peekable()).collect();

        loop {
            let peeks = peek_buckets(&mut children).await?;

            // Evict exhausted children so their underlying streams (and any
            // resources they own, such as request-scoped semaphore permits) are
            // released promptly rather than held until the slowest peer finishes.
            let mut surviving_children = Vec::with_capacity(children.len());
            let mut surviving_peeks = Vec::with_capacity(peeks.len());
            for (child, peek) in children.into_iter().zip_eq(peeks) {
                if let Some(b) = peek {
                    surviving_children.push(child);
                    surviving_peeks.push(b);
                }
            }
            children = surviving_children;

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
                    let (_, bitmap) = Pin::new(child)
                        .next()
                        .await
                        .expect("peek reported a value")?;
                    acc = Some(match acc {
                        None => bitmap,
                        Some(a) => a | bitmap,
                    });
                }
            }
            if let Some(bitmap) = acc
                && !bitmap.is_empty()
            {
                yield (next_bucket, bitmap);
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
) -> impl Stream<Item = BucketItem> + Send + 'static
where
    A: Stream<Item = BucketItem> + Send + 'static,
    B: Stream<Item = BucketItem> + Send + 'static,
{
    async_stream::try_stream! {
        let a = a.peekable();
        let b = b.peekable();
        futures::pin_mut!(a);
        futures::pin_mut!(b);

        loop {
            // The peeks poll both streams concurrently and buffer their head
            // rows; the later `next()` calls consume those buffered rows.
            let (a_peek, b_peek) =
                futures::try_join!(peek_bucket(a.as_mut()), peek_bucket(b.as_mut()))?;
            let Some(a_bucket) = a_peek else {
                return;
            };

            match b_peek {
                None => {
                    // No more negatives: flush a.
                    let (bid, bitmap) = a
                        .as_mut()
                        .next()
                        .await
                        .expect("peek reported a value")?;
                    if !bitmap.is_empty() {
                        yield (bid, bitmap);
                    }
                }
                Some(bb)
                    if (direction.is_ascending() && bb > a_bucket)
                        || (!direction.is_ascending() && bb < a_bucket) =>
                {
                    // b is ahead, emit a unchanged.
                    let (bid, bitmap) = a
                        .as_mut()
                        .next()
                        .await
                        .expect("peek reported a value")?;
                    if !bitmap.is_empty() {
                        yield (bid, bitmap);
                    }
                }
                Some(bb)
                    if (direction.is_ascending() && bb < a_bucket)
                        || (!direction.is_ascending() && bb > a_bucket) =>
                {
                    // b is behind; skip it.
                    let _ = b
                        .as_mut()
                        .next()
                        .await
                        .expect("peek reported a value")?;
                }
                Some(_) => {
                    // Same bucket: subtract.
                    let (bid, a_bm) = a
                        .as_mut()
                        .next()
                        .await
                        .expect("peek reported a value")?;
                    let (_, b_bm) = b
                        .as_mut()
                        .next()
                        .await
                        .expect("peek reported a value")?;
                    let diff = a_bm - b_bm;
                    if !diff.is_empty() {
                        yield (bid, diff);
                    }
                }
            }
        }
    }
}

/// Convert a stream of `(bucket_id, relative RoaringBitmap)` into absolute
/// member ids, applying edge-bucket trimming against `range`.
pub fn flatten_bucket_stream<S>(
    stream: S,
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
) -> impl Stream<Item = Result<u64>> + Send + 'static
where
    S: Stream<Item = BucketItem> + Send + 'static,
{
    async_stream::try_stream! {
        if range.is_empty() {
            return;
        }
        let start_bucket = range.start / bucket_size;
        let end_bucket = (range.end - 1) / bucket_size;
        futures::pin_mut!(stream);
        while let Some(item) = stream.next().await {
            let (bucket_id, bitmap) = item?;
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
                        yield bucket_start + bit as u64;
                    }
                }
            } else {
                for bit in bitmap.iter().rev() {
                    if bit >= lo && bit < hi {
                        yield bucket_start + bit as u64;
                    }
                }
            }
        }
    }
}

/// Peek at the head of a `Peekable<BucketStream>`, returning its bucket_id.
/// If the stream's next item is an error, consumes it to propagate via `?`.
async fn peek_bucket<S>(mut s: Pin<&mut Peekable<S>>) -> Result<Option<u64>>
where
    S: Stream<Item = BucketItem>,
{
    let peeked = match s.as_mut().peek().await {
        None => None,
        Some(Ok((b, _))) => Some(Ok(*b)),
        Some(Err(_)) => Some(Err(())),
    };

    match peeked {
        None => Ok(None),
        Some(Ok(bucket)) => Ok(Some(bucket)),
        Some(Err(())) => match s.as_mut().next().await {
            Some(Err(e)) => Err(e),
            Some(Ok((bucket, _))) => Ok(Some(bucket)),
            None => Ok(None),
        },
    }
}

async fn peek_buckets<S>(streams: &mut [Peekable<S>]) -> Result<Vec<Option<u64>>>
where
    S: Stream<Item = BucketItem> + Unpin,
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

    fn collect_bitmap_items(items: Vec<BucketItem>) -> Vec<(u64, Vec<u32>)> {
        items
            .into_iter()
            .map(|r| {
                let (b, bm) = r.unwrap();
                (b, bm.iter().collect())
            })
            .collect()
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

        let out: Vec<u64> = eval_bitmap_query_stream(
            source,
            query,
            0..200_000,
            BUCKET_SIZE,
            ScanDirection::Ascending,
        )
        .try_collect()
        .await
        .unwrap();

        assert_eq!(out, vec![2, BUCKET_SIZE + 5]);
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

        let out: Vec<u64> = eval_bitmap_query_stream(
            source,
            query,
            0..200_000,
            BUCKET_SIZE,
            ScanDirection::Descending,
        )
        .try_collect()
        .await
        .unwrap();

        assert_eq!(out, vec![BUCKET_SIZE + 5, 2]);
    }

    #[tokio::test]
    async fn intersect_n_basic() {
        let a = make_bucket_stream(vec![(0, &[1, 2, 3]), (1, &[4, 5]), (2, &[6])]);
        let b = make_bucket_stream(vec![(0, &[2, 3]), (2, &[6, 7])]);
        let c = make_bucket_stream(vec![(0, &[3, 4]), (2, &[6])]);
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
        let a = make_bucket_stream(vec![(2, &[6]), (1, &[4, 5]), (0, &[1, 2, 3])]);
        let b = make_bucket_stream(vec![(2, &[6, 7]), (0, &[2, 3])]);
        let c = make_bucket_stream(vec![(2, &[6]), (0, &[3, 4])]);
        let out: Vec<_> = intersect_n(vec![a, b, c], ScanDirection::Descending)
            .collect()
            .await;
        let out = collect_bitmap_items(out);
        assert_eq!(out, vec![(2, vec![6]), (0, vec![3])]);
    }

    #[tokio::test]
    async fn peek_bucket_propagates_errors_without_panicking() {
        let stream: BucketStream = stream::iter(vec![Err(anyhow::anyhow!("boom"))]).boxed();
        let mut stream = stream.peekable();

        let err = peek_bucket(Pin::new(&mut stream)).await.unwrap_err();

        assert!(err.to_string().contains("boom"));
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn intersect_n_disjoint_dropped() {
        let a = make_bucket_stream(vec![(0, &[1])]);
        let b = make_bucket_stream(vec![(0, &[2])]);
        let out: Vec<_> = intersect_n(vec![a, b], ScanDirection::Ascending)
            .collect()
            .await;
        let out = collect_bitmap_items(out);
        // intersection is empty, bucket dropped
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn intersect_n_one_empty() {
        let a = make_bucket_stream(vec![(0, &[1]), (1, &[2])]);
        let b = make_bucket_stream(vec![]);
        let out: Vec<_> = intersect_n(vec![a, b], ScanDirection::Ascending)
            .collect()
            .await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn union_n_basic() {
        let a = make_bucket_stream(vec![(0, &[1, 2]), (2, &[6])]);
        let b = make_bucket_stream(vec![(0, &[2, 3]), (1, &[4])]);
        let out: Vec<_> = union_n(vec![a, b], ScanDirection::Ascending)
            .collect()
            .await;
        let out = collect_bitmap_items(out);
        assert_eq!(out, vec![(0, vec![1, 2, 3]), (1, vec![4]), (2, vec![6])]);
    }

    #[tokio::test]
    async fn union_n_descending() {
        let a = make_bucket_stream(vec![(2, &[6]), (0, &[1, 2])]);
        let b = make_bucket_stream(vec![(1, &[4]), (0, &[2, 3])]);
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

        impl<S: Stream<Item = BucketItem> + Unpin> Stream for ObserveDrop<S> {
            type Item = BucketItem;
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
        let short: BucketStream = ObserveDrop {
            inner: stream::iter(vec![Ok::<_, anyhow::Error>((0u64, make_bitmap(&[1])))]),
            dropped: dropped.clone(),
        }
        .boxed();
        let long = make_bucket_stream(vec![(0, &[2]), (1, &[3]), (2, &[4])]);

        let merged = union_n(vec![short, long], ScanDirection::Ascending);
        futures::pin_mut!(merged);

        // Bucket 0 merges both children; the short stream is now exhausted
        // underneath but eviction has not run yet — it happens on the next
        // peek_buckets call.
        let (b, _) = merged.try_next().await.unwrap().unwrap();
        assert_eq!(b, 0);
        assert!(!dropped.load(Ordering::SeqCst));

        // The iteration that yields bucket 1 first peeks, sees the short
        // stream's peek is None, evicts it (dropping the Peekable and its
        // inner ObserveDrop), then yields. The long stream still has bucket 2
        // pending — proving the drop happened mid-merge, not at completion.
        let (b, _) = merged.try_next().await.unwrap().unwrap();
        assert_eq!(b, 1);
        assert!(dropped.load(Ordering::SeqCst));

        let (b, _) = merged.try_next().await.unwrap().unwrap();
        assert_eq!(b, 2);
        assert!(merged.try_next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn subtract_two_basic() {
        let a = make_bucket_stream(vec![(0, &[1, 2, 3]), (1, &[4, 5]), (2, &[6, 7])]);
        let b = make_bucket_stream(vec![(0, &[2]), (2, &[7]), (3, &[100])]);
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
        let a = make_bucket_stream(vec![(2, &[6, 7]), (1, &[4, 5]), (0, &[1, 2, 3])]);
        let b = make_bucket_stream(vec![(3, &[100]), (2, &[7]), (0, &[2])]);
        let out: Vec<_> = subtract_two(a, b, ScanDirection::Descending)
            .collect()
            .await;
        let out = collect_bitmap_items(out);
        assert_eq!(out, vec![(2, vec![6]), (1, vec![4, 5]), (0, vec![1, 3]),],);
    }

    #[tokio::test]
    async fn subtract_two_drops_fully_erased_buckets() {
        let a = make_bucket_stream(vec![(0, &[1])]);
        let b = make_bucket_stream(vec![(0, &[1])]);
        let out: Vec<_> = subtract_two(a, b, ScanDirection::Ascending).collect().await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn subtract_two_empty_right() {
        let a = make_bucket_stream(vec![(0, &[1, 2]), (5, &[3])]);
        let b = make_bucket_stream(vec![]);
        let out: Vec<_> = subtract_two(a, b, ScanDirection::Ascending).collect().await;
        let out = collect_bitmap_items(out);
        assert_eq!(out, vec![(0, vec![1, 2]), (5, vec![3])]);
    }

    #[tokio::test]
    async fn flatten_bucket_stream_edge_trimming() {
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
        let out: Vec<u64> =
            flatten_bucket_stream(items, range, BUCKET_SIZE, ScanDirection::Ascending)
                .try_collect()
                .await
                .unwrap();
        assert_eq!(
            out,
            vec![
                50,
                BUCKET_SIZE - 1,
                BUCKET_SIZE,
                2 * BUCKET_SIZE - 1,
                2 * BUCKET_SIZE,
                2 * BUCKET_SIZE + 50_000,
            ],
        );
    }

    #[tokio::test]
    async fn flatten_bucket_stream_descending_edge_trimming() {
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
        let out: Vec<u64> =
            flatten_bucket_stream(items, range, BUCKET_SIZE, ScanDirection::Descending)
                .try_collect()
                .await
                .unwrap();
        assert_eq!(
            out,
            vec![
                2 * BUCKET_SIZE + 50_000,
                2 * BUCKET_SIZE,
                2 * BUCKET_SIZE - 1,
                BUCKET_SIZE,
                BUCKET_SIZE - 1,
                50,
            ],
        );
    }
}
