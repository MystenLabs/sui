// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Composable expression tree for bitmap index queries.
//!
//! Callers build a `BitmapQuery` from dimension keys and logical operators,
//! then evaluate it via `BigTableClient::eval_bitmap_query` (collects into a
//! `Vec<u64>`) or `eval_bitmap_query_stream` (yields tx_sequence_numbers as
//! they are produced, so back-pressure from downstream consumers — e.g. a
//! `.take(page_size)` — propagates all the way back to BigTable and avoids
//! materializing matches we won't use).
//!
//! The streaming evaluator produces `(bucket_id, RoaringBitmap)` from each
//! dimension scan, then performs operator-specific merge-joins on the ordered
//! bucket streams. Bucket row keys are `v{version}#{dimension}#{bucket_id:010}`,
//! so BigTable delivers buckets in ascending order — the basis for the
//! streaming merge-join.

use std::ops::Range;
use std::pin::Pin;

use crate::tables::event_bitmap_index;
use crate::tables::transaction_bitmap_index;
use anyhow::Context;
use anyhow::Result;
use futures::Stream;
use futures::StreamExt;
use futures::stream::BoxStream;
use futures::stream::Peekable;
use roaring::RoaringBitmap;
use roaring::RoaringTreemap;

use super::BigTableClient;

/// Identifies which inverted-index table a `BitmapQuery` evaluates against.
///
/// Tx-keyed scans bit positions correspond to `tx_sequence_number`s;
/// event-keyed scans correspond to packed `event_seq` values produced by
/// [`crate::tables::event_bitmap_index::encode_event_seq`].
#[derive(Clone, Copy)]
pub struct BitmapIndexSpec {
    pub table_name: &'static str,
    pub schema_version: u32,
    pub bucket_size: u64,
    pub bucket_id_width: usize,
    pub bitmap_column: &'static str,
}

impl BitmapIndexSpec {
    pub const fn tx() -> Self {
        Self {
            table_name: transaction_bitmap_index::NAME,
            schema_version: transaction_bitmap_index::SCHEMA_VERSION,
            bucket_size: transaction_bitmap_index::BUCKET_SIZE,
            bucket_id_width: 10,
            bitmap_column: transaction_bitmap_index::col::BITMAP,
        }
    }

    pub const fn event() -> Self {
        Self {
            table_name: event_bitmap_index::NAME,
            schema_version: event_bitmap_index::SCHEMA_VERSION,
            bucket_size: event_bitmap_index::BUCKET_SIZE,
            bucket_id_width: 12,
            bitmap_column: event_bitmap_index::col::BITMAP,
        }
    }

    fn encode_row_key(&self, dimension_key: &[u8], bucket_id: u64) -> Vec<u8> {
        match self.bucket_id_width {
            10 => transaction_bitmap_index::encode_row_key(
                self.schema_version,
                dimension_key,
                bucket_id,
            ),
            12 => event_bitmap_index::encode_row_key(self.schema_version, dimension_key, bucket_id),
            w => panic!("unsupported bucket_id_width {w}"),
        }
    }
}

/// A stream of `(bucket_id, RoaringBitmap)` in ascending `bucket_id` order.
/// Bitmap positions are **relative** to the bucket (u32 offsets `[0, BUCKET_SIZE)`)
/// — edge trimming against the requested `tx_range` happens at the flatten step.
type BucketStream = BoxStream<'static, Result<(u64, RoaringBitmap)>>;

/// A logical expression over bitmap dimension scans.
#[derive(Clone)]
pub enum BitmapQuery {
    /// Scan a single dimension key. Leaf node.
    Scan(Vec<u8>),
    /// Intersection: all sub-expressions must match.
    And(Vec<BitmapQuery>),
    /// Union: any sub-expression matches.
    Or(Vec<BitmapQuery>),
    /// Complement: matches everything in the range NOT matched by the inner
    /// expression. Best used as a child of `And` where the positive terms
    /// constrain the universe first — a standalone top-level `Not` must
    /// materialize the full range as the universe.
    Not(Box<BitmapQuery>),
    /// Symmetric difference: matches tx_seqs in exactly one of the two operands.
    Xor(Box<BitmapQuery>, Box<BitmapQuery>),
}

impl BitmapQuery {
    pub fn scan(dimension_key: Vec<u8>) -> Self {
        Self::Scan(dimension_key)
    }

    pub fn and(children: Vec<BitmapQuery>) -> Self {
        Self::And(children)
    }

    pub fn or(children: Vec<BitmapQuery>) -> Self {
        Self::Or(children)
    }

    pub fn complement(inner: BitmapQuery) -> Self {
        Self::Not(Box::new(inner))
    }

    pub fn xor(a: BitmapQuery, b: BitmapQuery) -> Self {
        Self::Xor(Box::new(a), Box::new(b))
    }
}

impl BigTableClient {
    /// Streaming evaluation of a `BitmapQuery` against the tx-keyed index.
    /// Yields matching tx_sequence_numbers in ascending order, without
    /// materializing intermediate bitmaps for the entire range up-front.
    /// Back-pressure from the consumer (e.g. `.take(N)`) propagates back to
    /// BigTable, so a selective filter over a large range only reads as many
    /// bitmap buckets as are needed to produce the requested number of tx_seqs.
    pub fn eval_bitmap_query_stream(
        &self,
        query: BitmapQuery,
        tx_range: Range<u64>,
    ) -> impl Stream<Item = Result<u64>> + Send + 'static {
        self.eval_bitmap_query_stream_with_spec(query, tx_range, BitmapIndexSpec::tx())
    }

    /// Streaming evaluation against an arbitrary bitmap index (tx-keyed or
    /// event-keyed). See [`Self::eval_bitmap_query_stream`] for back-pressure
    /// semantics.
    pub fn eval_bitmap_query_stream_with_spec(
        &self,
        query: BitmapQuery,
        range: Range<u64>,
        spec: BitmapIndexSpec,
    ) -> impl Stream<Item = Result<u64>> + Send + 'static {
        let stream = bucket_stream(self.clone(), query, range.clone(), spec);
        flatten_bucket_stream(stream, range, spec)
    }

    /// Evaluate a `BitmapQuery` expression, returning matching tx_sequence_numbers
    /// as a sorted `Vec<u64>` within the given range.
    pub async fn eval_bitmap_query(
        &mut self,
        query: &BitmapQuery,
        tx_range: Range<u64>,
    ) -> Result<Vec<u64>> {
        let bitmap =
            Box::pin(self.eval_bitmap_inner(query, tx_range, BitmapIndexSpec::tx())).await?;
        Ok(bitmap.iter().collect())
    }

    /// Internal evaluator that keeps results as `RoaringTreemap` throughout,
    /// leveraging Roaring's native chunk-level set operations.
    async fn eval_bitmap_inner(
        &mut self,
        query: &BitmapQuery,
        range: Range<u64>,
        spec: BitmapIndexSpec,
    ) -> Result<RoaringTreemap> {
        match query {
            BitmapQuery::Scan(key) => self.scan_bitmap_index_roaring(key, range, spec).await,

            BitmapQuery::And(children) => {
                if children.is_empty() {
                    return Ok(RoaringTreemap::new());
                }

                // Separate positive and negative (NOT) children.
                // Evaluate positive terms first and intersect to get a small
                // working set, then subtract each NOT term directly. This avoids
                // materializing the full complement of sparse NOT operands.
                let mut positive = Vec::new();
                let mut negative = Vec::new();
                for child in children {
                    if let BitmapQuery::Not(inner) = child {
                        negative.push(inner.as_ref());
                    } else {
                        positive.push(child);
                    }
                }

                let mut result = if positive.is_empty() {
                    // All children are NOT — start with the full range.
                    let mut universe = RoaringTreemap::new();
                    universe.insert_range(range.clone());
                    universe
                } else {
                    let mut acc: Option<RoaringTreemap> = None;
                    for child in &positive {
                        let child_set =
                            Box::pin(self.eval_bitmap_inner(child, range.clone(), spec)).await?;
                        acc = Some(match acc {
                            None => child_set,
                            Some(a) => a & child_set,
                        });
                    }
                    acc.unwrap()
                };

                // Subtract each NOT term from the (already small) result
                for neg_inner in &negative {
                    let neg_set =
                        Box::pin(self.eval_bitmap_inner(neg_inner, range.clone(), spec)).await?;
                    result -= neg_set;
                }

                Ok(result)
            }

            BitmapQuery::Or(children) => {
                if children.is_empty() {
                    return Ok(RoaringTreemap::new());
                }
                let mut result = RoaringTreemap::new();
                for child in children {
                    let child_set =
                        Box::pin(self.eval_bitmap_inner(child, range.clone(), spec)).await?;
                    result |= child_set;
                }
                Ok(result)
            }

            BitmapQuery::Not(inner) => {
                let inner_set =
                    Box::pin(self.eval_bitmap_inner(inner, range.clone(), spec)).await?;
                let mut universe = RoaringTreemap::new();
                universe.insert_range(range);
                Ok(universe - inner_set)
            }

            BitmapQuery::Xor(a, b) => {
                let set_a = Box::pin(self.eval_bitmap_inner(a, range.clone(), spec)).await?;
                let set_b = Box::pin(self.eval_bitmap_inner(b, range.clone(), spec)).await?;
                Ok(set_a ^ set_b)
            }
        }
    }

    /// Scan the bitmap index and return results as a `RoaringTreemap` with
    /// absolute bit positions. Used internally by the query evaluator.
    pub(super) async fn scan_bitmap_index_roaring(
        &mut self,
        dimension_key: &[u8],
        range: Range<u64>,
        spec: BitmapIndexSpec,
    ) -> Result<RoaringTreemap> {
        if range.is_empty() {
            return Ok(RoaringTreemap::new());
        }

        let start_bucket = range.start / spec.bucket_size;
        let end_bucket = (range.end - 1) / spec.bucket_size;

        let start_row = spec.encode_row_key(dimension_key, start_bucket);
        let end_row = spec.encode_row_key(dimension_key, end_bucket);

        // Stream rows from BigTable, building the RoaringTreemap incrementally
        // without buffering raw rows in memory.
        use futures::StreamExt;
        let stream = self
            .range_scan_stream(
                spec.table_name,
                Some(bytes::Bytes::from(start_row)),
                Some(bytes::Bytes::from(end_row)),
                0,
                false,
                None,
            )
            .await?;
        futures::pin_mut!(stream);

        let mut result = RoaringTreemap::new();
        while let Some(row) = stream.next().await {
            let (row_key, cells) = row?;
            let bitmap_bytes = cells
                .iter()
                .find(|(col, _)| col.as_ref() == spec.bitmap_column.as_bytes())
                .map(|(_, v)| v);

            let Some(bitmap_bytes) = bitmap_bytes else {
                continue;
            };

            let hash_pos = row_key
                .iter()
                .rposition(|&b| b == b'#')
                .context("malformed bitmap index row key: no '#' separator")?;
            let suffix = &row_key[hash_pos + 1..];
            let bucket_id: u64 = std::str::from_utf8(suffix)
                .context("non-ascii bucket_id suffix")?
                .parse()
                .context("invalid bucket_id in row key")?;

            let bucket_bitmap = roaring::RoaringBitmap::deserialize_from(bitmap_bytes.as_ref())
                .context("deserializing bitmap")?;

            let bucket_start = bucket_id * spec.bucket_size;
            let is_first_bucket = bucket_id == start_bucket;
            let is_last_bucket = bucket_id == end_bucket;

            if !is_first_bucket && !is_last_bucket {
                // Middle bucket: entirely within range, no filtering needed.
                result
                    .append(bucket_bitmap.iter().map(|bit| bucket_start + bit as u64))
                    .expect("bucket bits are sorted");
            } else {
                // Edge bucket: intersect with the intra-bucket range [lo, hi).
                let lo = if is_first_bucket {
                    (range.start - bucket_start) as u32
                } else {
                    0
                };
                let hi = if is_last_bucket {
                    ((range.end - bucket_start).min(spec.bucket_size)) as u32
                } else {
                    spec.bucket_size as u32
                };

                let mut range_mask = RoaringBitmap::new();
                range_mask.insert_range(lo..hi);
                let filtered = bucket_bitmap & range_mask;
                result
                    .append(filtered.iter().map(|bit| bucket_start + bit as u64))
                    .expect("bucket bits are sorted");
            }
        }

        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Streaming evaluator
// ---------------------------------------------------------------------------

/// Dispatch a `BitmapQuery` node to its streaming implementation.
fn bucket_stream(
    client: BigTableClient,
    query: BitmapQuery,
    range: Range<u64>,
    spec: BitmapIndexSpec,
) -> BucketStream {
    match query {
        BitmapQuery::Scan(key) => scan_bucket_stream(client, key, range, spec).boxed(),
        BitmapQuery::And(children) => and_bucket_stream(client, children, range, spec).boxed(),
        BitmapQuery::Or(children) => or_bucket_stream(client, children, range, spec).boxed(),
        BitmapQuery::Not(inner) => not_bucket_stream(client, *inner, range, spec).boxed(),
        BitmapQuery::Xor(a, b) => xor_bucket_stream(client, *a, *b, range, spec).boxed(),
    }
}

/// Stream a single bitmap-index dimension's buckets in order, one
/// `RoaringBitmap` per bucket with **relative** bit positions.
fn scan_bucket_stream(
    mut client: BigTableClient,
    dimension_key: Vec<u8>,
    range: Range<u64>,
    spec: BitmapIndexSpec,
) -> impl Stream<Item = Result<(u64, RoaringBitmap)>> + Send + 'static {
    async_stream::try_stream! {
        if range.is_empty() {
            return;
        }
        let start_bucket = range.start / spec.bucket_size;
        let end_bucket = (range.end - 1) / spec.bucket_size;

        let start_row = spec.encode_row_key(&dimension_key, start_bucket);
        let end_row = spec.encode_row_key(&dimension_key, end_bucket);

        let stream = client
            .range_scan_stream(
                spec.table_name,
                Some(bytes::Bytes::from(start_row)),
                Some(bytes::Bytes::from(end_row)),
                0,
                false,
                None,
            )
            .await?;
        futures::pin_mut!(stream);

        while let Some(row) = stream.next().await {
            let (row_key, cells) = row?;
            let Some(bitmap_bytes) = cells
                .iter()
                .find(|(col, _)| col.as_ref() == spec.bitmap_column.as_bytes())
                .map(|(_, v)| v)
            else {
                continue;
            };

            let hash_pos = row_key
                .iter()
                .rposition(|&b| b == b'#')
                .context("malformed bitmap index row key: no '#' separator")?;
            let suffix = &row_key[hash_pos + 1..];
            let bucket_id: u64 = std::str::from_utf8(suffix)
                .context("non-ascii bucket_id suffix")?
                .parse()
                .context("invalid bucket_id in row key")?;

            let bitmap = RoaringBitmap::deserialize_from(bitmap_bytes.as_ref())
                .context("deserializing bitmap")?;
            yield (bucket_id, bitmap);
        }
    }
}

/// Emit a full-bucket bitmap for every bucket touching `range`, used as the
/// "universe" in `And` branches whose children are all negative, and in `Not`.
fn universe_bucket_stream(
    range: Range<u64>,
    spec: BitmapIndexSpec,
) -> impl Stream<Item = Result<(u64, RoaringBitmap)>> + Send + 'static {
    async_stream::try_stream! {
        if range.is_empty() {
            return;
        }
        let start_bucket = range.start / spec.bucket_size;
        let end_bucket = (range.end - 1) / spec.bucket_size;
        for bucket_id in start_bucket..=end_bucket {
            let mut bm = RoaringBitmap::new();
            bm.insert_range(0..spec.bucket_size as u32);
            yield (bucket_id, bm);
        }
    }
}

/// AND with NOT optimization: evaluate positives as a multi-way merge
/// intersection, then merge-subtract each negative. Mirrors the
/// `eval_bitmap_inner` And branch in streaming form.
fn and_bucket_stream(
    client: BigTableClient,
    children: Vec<BitmapQuery>,
    range: Range<u64>,
    spec: BitmapIndexSpec,
) -> impl Stream<Item = Result<(u64, RoaringBitmap)>> + Send + 'static {
    async_stream::try_stream! {
        if children.is_empty() {
            return;
        }

        let mut positive = Vec::new();
        let mut negative = Vec::new();
        for child in children {
            if let BitmapQuery::Not(inner) = child {
                negative.push(*inner);
            } else {
                positive.push(child);
            }
        }

        let base: BucketStream = if positive.is_empty() {
            universe_bucket_stream(range.clone(), spec).boxed()
        } else {
            let streams: Vec<BucketStream> = positive
                .into_iter()
                .map(|q| bucket_stream(client.clone(), q, range.clone(), spec))
                .collect();
            intersect_n(streams).boxed()
        };

        let mut result: BucketStream = base;
        for neg in negative {
            let neg_stream = bucket_stream(client.clone(), neg, range.clone(), spec);
            result = subtract_two(result, neg_stream).boxed();
        }

        futures::pin_mut!(result);
        while let Some(item) = result.next().await {
            yield item?;
        }
    }
}

/// Multi-way merge intersection over ordered bucket streams. Emits only those
/// bucket_ids present in every child stream, with the bitwise AND of all their
/// bitmaps. Drops empty results.
fn intersect_n(
    streams: Vec<BucketStream>,
) -> impl Stream<Item = Result<(u64, RoaringBitmap)>> + Send + 'static {
    async_stream::try_stream! {
        let mut children: Vec<Pin<Box<Peekable<BucketStream>>>> =
            streams.into_iter().map(|s| Box::pin(s.peekable())).collect();

        'outer: loop {
            // Peek all children, collecting current bucket_ids.
            let mut peeks: Vec<u64> = Vec::with_capacity(children.len());
            for child in children.iter_mut() {
                match peek_bucket(child.as_mut()).await? {
                    None => break 'outer,
                    Some(b) => peeks.push(b),
                }
            }
            let max_bucket = *peeks.iter().max().expect("children non-empty");

            if peeks.iter().all(|&b| b == max_bucket) {
                // All children at the same bucket — intersect and emit.
                let mut acc: Option<RoaringBitmap> = None;
                for child in children.iter_mut() {
                    let (bid, bitmap) = child
                        .as_mut()
                        .next()
                        .await
                        .expect("peek reported a value")?;
                    debug_assert_eq!(bid, max_bucket);
                    acc = Some(match acc {
                        None => bitmap,
                        Some(a) => a & bitmap,
                    });
                }
                let bitmap = acc.expect("children non-empty");
                if !bitmap.is_empty() {
                    yield (max_bucket, bitmap);
                }
            } else {
                // Advance laggards past the max; we'll re-peek on the next iter.
                for (i, child) in children.iter_mut().enumerate() {
                    if peeks[i] < max_bucket {
                        let _ = child
                            .as_mut()
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
fn or_bucket_stream(
    client: BigTableClient,
    children: Vec<BitmapQuery>,
    range: Range<u64>,
    spec: BitmapIndexSpec,
) -> impl Stream<Item = Result<(u64, RoaringBitmap)>> + Send + 'static {
    async_stream::try_stream! {
        if children.is_empty() {
            return;
        }
        let streams: Vec<BucketStream> = children
            .into_iter()
            .map(|q| bucket_stream(client.clone(), q, range.clone(), spec))
            .collect();
        let mut children: Vec<Pin<Box<Peekable<BucketStream>>>> =
            streams.into_iter().map(|s| Box::pin(s.peekable())).collect();

        loop {
            let mut min_bucket: Option<u64> = None;
            for child in children.iter_mut() {
                if let Some(b) = peek_bucket(child.as_mut()).await? {
                    min_bucket = Some(match min_bucket {
                        None => b,
                        Some(m) => m.min(b),
                    });
                }
            }
            let Some(min_bucket) = min_bucket else {
                return;
            };

            let mut acc: Option<RoaringBitmap> = None;
            for child in children.iter_mut() {
                let peeked = peek_bucket(child.as_mut()).await?;
                let take = matches!(peeked, Some(b) if b == min_bucket);
                if take {
                    let (_, bitmap) = child
                        .as_mut()
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
                yield (min_bucket, bitmap);
            }
        }
    }
}

/// Standalone NOT: walks every bucket touching `tx_range`, subtracting the
/// inner stream's bitmap where present, emitting a full bucket where absent.
fn not_bucket_stream(
    client: BigTableClient,
    inner: BitmapQuery,
    range: Range<u64>,
    spec: BitmapIndexSpec,
) -> impl Stream<Item = Result<(u64, RoaringBitmap)>> + Send + 'static {
    async_stream::try_stream! {
        if range.is_empty() {
            return;
        }
        let start_bucket = range.start / spec.bucket_size;
        let end_bucket = (range.end - 1) / spec.bucket_size;
        let inner_stream = bucket_stream(client, inner, range.clone(), spec);
        let mut inner = Box::pin(inner_stream.peekable());

        for bucket_id in start_bucket..=end_bucket {
            // Skip any inner buckets before `bucket_id` (shouldn't normally happen
            // since both sides iterate the same range, but defensively handle it).
            loop {
                match peek_bucket(inner.as_mut()).await? {
                    Some(b) if b < bucket_id => {
                        let _ = inner
                            .as_mut()
                            .next()
                            .await
                            .expect("peek reported a value")?;
                    }
                    _ => break,
                }
            }

            let mut universe = RoaringBitmap::new();
            universe.insert_range(0..spec.bucket_size as u32);

            let peeked = peek_bucket(inner.as_mut()).await?;
            let matches = matches!(peeked, Some(b) if b == bucket_id);
            let bitmap = if matches {
                let (_, inner_bm) = inner
                    .as_mut()
                    .next()
                    .await
                    .expect("peek reported a value")?;
                universe - inner_bm
            } else {
                universe
            };
            if !bitmap.is_empty() {
                yield (bucket_id, bitmap);
            }
        }
    }
}

/// Merge-join subtraction: for each bucket in `a`, emits `a_bm - b_bm` if `b`
/// has the same bucket, else emits `a_bm` unchanged. Drops empty results.
fn subtract_two(
    a: BucketStream,
    b: BucketStream,
) -> impl Stream<Item = Result<(u64, RoaringBitmap)>> + Send + 'static {
    async_stream::try_stream! {
        let mut a = Box::pin(a.peekable());
        let mut b = Box::pin(b.peekable());

        loop {
            let Some(a_bucket) = peek_bucket(a.as_mut()).await? else {
                return;
            };
            let b_peek = peek_bucket(b.as_mut()).await?;

            match b_peek {
                None => {
                    // No more negatives — flush a.
                    let (bid, bitmap) = a
                        .as_mut()
                        .next()
                        .await
                        .expect("peek reported a value")?;
                    if !bitmap.is_empty() {
                        yield (bid, bitmap);
                    }
                }
                Some(bb) if bb > a_bucket => {
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
                Some(bb) if bb < a_bucket => {
                    // b is behind; skip it.
                    let _ = b
                        .as_mut()
                        .next()
                        .await
                        .expect("peek reported a value")?;
                }
                Some(_) => {
                    // Same bucket — subtract.
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

/// Merge-join XOR: emits every bucket in either side, combining via `^` when
/// both are present.
fn xor_bucket_stream(
    client: BigTableClient,
    a: BitmapQuery,
    b: BitmapQuery,
    range: Range<u64>,
    spec: BitmapIndexSpec,
) -> impl Stream<Item = Result<(u64, RoaringBitmap)>> + Send + 'static {
    async_stream::try_stream! {
        let a_stream = bucket_stream(client.clone(), a, range.clone(), spec);
        let b_stream = bucket_stream(client, b, range, spec);
        let mut a = Box::pin(a_stream.peekable());
        let mut b = Box::pin(b_stream.peekable());

        loop {
            let ap = peek_bucket(a.as_mut()).await?;
            let bp = peek_bucket(b.as_mut()).await?;
            match (ap, bp) {
                (None, None) => return,
                (Some(_), None) => {
                    let (bid, bm) = a
                        .as_mut()
                        .next()
                        .await
                        .expect("peek reported a value")?;
                    if !bm.is_empty() {
                        yield (bid, bm);
                    }
                }
                (None, Some(_)) => {
                    let (bid, bm) = b
                        .as_mut()
                        .next()
                        .await
                        .expect("peek reported a value")?;
                    if !bm.is_empty() {
                        yield (bid, bm);
                    }
                }
                (Some(ab), Some(bb)) if ab < bb => {
                    let (bid, bm) = a
                        .as_mut()
                        .next()
                        .await
                        .expect("peek reported a value")?;
                    if !bm.is_empty() {
                        yield (bid, bm);
                    }
                }
                (Some(ab), Some(bb)) if ab > bb => {
                    let (bid, bm) = b
                        .as_mut()
                        .next()
                        .await
                        .expect("peek reported a value")?;
                    if !bm.is_empty() {
                        yield (bid, bm);
                    }
                }
                (Some(_), Some(_)) => {
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
                    let xored = a_bm ^ b_bm;
                    if !xored.is_empty() {
                        yield (bid, xored);
                    }
                }
            }
        }
    }
}

/// Convert a stream of `(bucket_id, relative RoaringBitmap)` into absolute
/// tx_sequence_numbers, applying edge-bucket trimming against `tx_range`.
fn flatten_bucket_stream<S>(
    stream: S,
    range: Range<u64>,
    spec: BitmapIndexSpec,
) -> impl Stream<Item = Result<u64>> + Send + 'static
where
    S: Stream<Item = Result<(u64, RoaringBitmap)>> + Send + 'static,
{
    async_stream::try_stream! {
        if range.is_empty() {
            return;
        }
        let start_bucket = range.start / spec.bucket_size;
        let end_bucket = (range.end - 1) / spec.bucket_size;
        futures::pin_mut!(stream);
        while let Some(item) = stream.next().await {
            let (bucket_id, bitmap) = item?;
            let bucket_start = bucket_id * spec.bucket_size;
            let is_first = bucket_id == start_bucket;
            let is_last = bucket_id == end_bucket;

            if !is_first && !is_last {
                for bit in bitmap.iter() {
                    yield bucket_start + bit as u64;
                }
            } else {
                let lo = if is_first {
                    (range.start - bucket_start) as u32
                } else {
                    0
                };
                let hi = if is_last {
                    ((range.end - bucket_start).min(spec.bucket_size)) as u32
                } else {
                    spec.bucket_size as u32
                };
                for bit in bitmap.iter() {
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
async fn peek_bucket(mut s: Pin<&mut Peekable<BucketStream>>) -> Result<Option<u64>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::TryStreamExt;
    use futures::stream;

    fn make_bitmap(bits: &[u32]) -> RoaringBitmap {
        let mut bm = RoaringBitmap::new();
        for &b in bits {
            bm.insert(b);
        }
        bm
    }

    fn make_bucket_stream(items: Vec<(u64, &[u32])>) -> BucketStream {
        let items: Vec<Result<(u64, RoaringBitmap)>> = items
            .into_iter()
            .map(|(bid, bits)| Ok((bid, make_bitmap(bits))))
            .collect();
        stream::iter(items).boxed()
    }

    fn collect_bitmap_items(items: Vec<Result<(u64, RoaringBitmap)>>) -> Vec<(u64, Vec<u32>)> {
        items
            .into_iter()
            .map(|r| {
                let (b, bm) = r.unwrap();
                (b, bm.iter().collect())
            })
            .collect()
    }

    #[tokio::test]
    async fn intersect_n_basic() {
        let a = make_bucket_stream(vec![(0, &[1, 2, 3]), (1, &[4, 5]), (2, &[6])]);
        let b = make_bucket_stream(vec![(0, &[2, 3]), (2, &[6, 7])]);
        let c = make_bucket_stream(vec![(0, &[3, 4]), (2, &[6])]);
        let out: Vec<_> = intersect_n(vec![a, b, c]).boxed().collect().await;
        let out = collect_bitmap_items(out);
        // bucket 0: {1,2,3} ∩ {2,3} ∩ {3,4} = {3}
        // bucket 1: only in a → dropped by AND
        // bucket 2: {6} ∩ {6,7} ∩ {6} = {6}
        assert_eq!(out, vec![(0, vec![3]), (2, vec![6])]);
    }

    #[tokio::test]
    async fn peek_bucket_propagates_errors_without_panicking() {
        let stream: BucketStream = stream::iter(vec![Err(anyhow::anyhow!("boom"))]).boxed();
        let mut stream = Box::pin(stream.peekable());

        let err = peek_bucket(stream.as_mut()).await.unwrap_err();

        assert!(err.to_string().contains("boom"));
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn intersect_n_disjoint_dropped() {
        let a = make_bucket_stream(vec![(0, &[1])]);
        let b = make_bucket_stream(vec![(0, &[2])]);
        let out: Vec<_> = intersect_n(vec![a, b]).boxed().collect().await;
        let out = collect_bitmap_items(out);
        // intersection is empty → bucket dropped
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn intersect_n_one_empty() {
        let a = make_bucket_stream(vec![(0, &[1]), (1, &[2])]);
        let b = make_bucket_stream(vec![]);
        let out: Vec<_> = intersect_n(vec![a, b]).boxed().collect().await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn subtract_two_basic() {
        let a = make_bucket_stream(vec![(0, &[1, 2, 3]), (1, &[4, 5]), (2, &[6, 7])]);
        let b = make_bucket_stream(vec![(0, &[2]), (2, &[7]), (3, &[100])]);
        let out: Vec<_> = subtract_two(a, b).boxed().collect().await;
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
    async fn subtract_two_drops_fully_erased_buckets() {
        let a = make_bucket_stream(vec![(0, &[1])]);
        let b = make_bucket_stream(vec![(0, &[1])]);
        let out: Vec<_> = subtract_two(a, b).boxed().collect().await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn subtract_two_empty_right() {
        let a = make_bucket_stream(vec![(0, &[1, 2]), (5, &[3])]);
        let b = make_bucket_stream(vec![]);
        let out: Vec<_> = subtract_two(a, b).boxed().collect().await;
        let out = collect_bitmap_items(out);
        assert_eq!(out, vec![(0, vec![1, 2]), (5, vec![3])]);
    }

    #[tokio::test]
    async fn flatten_bucket_stream_edge_trimming() {
        // Pick a tx_range that spans 3 buckets at the current BUCKET_SIZE,
        // with partial start (bucket 0) and partial end (bucket 2).
        let bs = transaction_bitmap_index::BUCKET_SIZE;
        let tx_range = 50u64..(2 * bs + 50_001);
        let items = stream::iter(vec![
            // bucket 0: bit 10 trimmed (< 50); 50 and bs-1 kept.
            Ok((0u64, {
                let mut bm = RoaringBitmap::new();
                bm.insert(10);
                bm.insert(50);
                bm.insert((bs - 1) as u32);
                bm
            })),
            // bucket 1: middle, full pass-through.
            Ok((1u64, {
                let mut bm = RoaringBitmap::new();
                bm.insert(0);
                bm.insert((bs - 1) as u32);
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
        let out: Vec<u64> = flatten_bucket_stream(items, tx_range, BitmapIndexSpec::tx())
            .try_collect()
            .await
            .unwrap();
        assert_eq!(
            out,
            vec![50, bs - 1, bs, 2 * bs - 1, 2 * bs, 2 * bs + 50_000,],
        );
    }
}
