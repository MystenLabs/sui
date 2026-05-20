// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Synchronous iterator evaluator for DNF bitmap queries.
//!
//! A single flat driver merge-joins every leaf scan against one shared *floor*
//! (the slowest leaf's position), evaluates the whole DNF — `OR` of
//! (`AND` includes `MINUS` `OR` excludes) — at the floor bucket, and derives the
//! progress watermark from the floor. Because leaves only ever advance at the
//! floor (peeked one bucket ahead), no branch can run ahead of the others: the
//! resume cursor is always within one sparse read of every leaf, and there is no
//! windowing/parking machinery to get wrong. Mirrors the async
//! [`super::stream`] evaluator, which shares the per-bucket evaluation
//! ([`eval_term_at_bucket`]) and is cross-checked against this one in tests.
//!
//! Budget accounting lives in the request layer for the iterator path (each
//! backend leaf iterator charges its own per-request budget and yields an error
//! on exhaustion), so this evaluator only propagates those errors.

use std::collections::VecDeque;
use std::iter::Peekable as IterPeekable;
use std::ops::Range;

use roaring::RoaringBitmap;

use super::BitmapBucketIteratorSource;
use super::BitmapQuery;
use super::BucketItem;
use super::MultiError;
use super::ScanDirection;
use super::Watermarked;
use super::WatermarkedBucket;
use super::bound_in_direction;
use super::bucket_edges;
use super::eval_term_at_bucket;
use super::frontier_advanced;
use super::split_term_literals;

/// One DNF term, as index spans into the flat leaf vector.
struct TermSpec {
    includes: Vec<usize>,
    excludes: Vec<usize>,
    /// Cleared once any include leaf hits EOF: the intersection can never match
    /// again, so the whole term is retired and its leaves dropped.
    dead: bool,
}

/// A leaf's head this round, from a non-consuming peek.
enum LeafHead {
    Bucket(u64),
    Eof,
    Error,
}

/// Evaluate a DNF `BitmapQuery` as an ordered iterator of marked bucket bitmaps.
/// Output emits `Watermarked::Item((bucket_id, bitmap))` interleaved with
/// `Watermarked::Watermark(p)` derived from the slowest leaf's progress.
pub fn eval_bitmap_query_bucket_iter<'a, S>(
    source: S,
    query: BitmapQuery,
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
) -> impl Iterator<Item = WatermarkedBucket> + 'a
where
    S: BitmapBucketIteratorSource<'a>,
{
    // One peekable leaf per literal; terms reference them by index. Each leaf
    // iterator borrows the backend's `'a` store, not `source`, so the thin
    // `source` handle is dropped once the leaves are built.
    let mut leaves: Vec<IterPeekable<S::Iter>> = Vec::new();
    let mut terms: Vec<TermSpec> = Vec::new();
    for term in query.terms {
        let (includes, excludes) = split_term_literals(term);
        let mut include_idx = Vec::with_capacity(includes.len());
        for key in includes {
            include_idx.push(leaves.len());
            leaves.push(
                source
                    .scan_bucket_iter(key, range.clone(), direction)
                    .peekable(),
            );
        }
        let mut exclude_idx = Vec::with_capacity(excludes.len());
        for key in excludes {
            exclude_idx.push(leaves.len());
            leaves.push(
                source
                    .scan_bucket_iter(key, range.clone(), direction)
                    .peekable(),
            );
        }
        terms.push(TermSpec {
            includes: include_idx,
            excludes: exclude_idx,
            dead: false,
        });
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
    // `gone[i]`: leaf retired (its term died or it is a spent exclude).
    let mut gone = vec![false; leaf_count];
    // `front[i]`: clamped position each leaf has provably scanned to. Bounds the
    // resume cursor when a leaf errors before it can advance.
    let mut front = vec![request_floor; leaf_count];
    let mut last_emitted: Option<u64> = None;
    let mut done = false;
    let mut pending: VecDeque<WatermarkedBucket> = VecDeque::new();

    std::iter::from_fn(move || {
        loop {
            if let Some(out) = pending.pop_front() {
                return Some(out);
            }
            if done {
                return None;
            }

            // Peek every active leaf (non-consuming); record its head and the
            // position it has now scanned to.
            let mut class: Vec<Option<LeafHead>> = (0..leaf_count).map(|_| None).collect();
            for i in 0..leaf_count {
                if gone[i] {
                    continue;
                }
                match leaves[i].peek() {
                    Some(Ok((bucket, _))) => {
                        let (pre, _post) = bucket_edges(*bucket, bucket_size, &range, direction);
                        front[i] = pre;
                        class[i] = Some(LeafHead::Bucket(*bucket));
                    }
                    None => {
                        front[i] = terminus;
                        class[i] = Some(LeafHead::Eof);
                    }
                    // Budget exhaustion: leave `front[i]` at its last scanned
                    // position so the resume cursor cannot claim past it.
                    Some(Err(_)) => class[i] = Some(LeafHead::Error),
                }
            }

            // An include at EOF kills its term (the intersection is permanently
            // empty); drop all of that term's leaves so they stop being polled.
            for term in terms.iter_mut() {
                if !term.dead
                    && term
                        .includes
                        .iter()
                        .any(|&i| matches!(class[i], Some(LeafHead::Eof)))
                {
                    term.dead = true;
                }
            }
            for term in &terms {
                if term.dead {
                    for &i in term.includes.iter().chain(term.excludes.iter()) {
                        gone[i] = true;
                    }
                } else {
                    // A spent exclude just stops contributing subtractions.
                    for &i in &term.excludes {
                        if matches!(class[i], Some(LeafHead::Eof)) {
                            gone[i] = true;
                        }
                    }
                }
            }

            // Consume any budget-error frame so the error surfaces (after the
            // floor watermark below).
            let mut errors: Vec<anyhow::Error> = Vec::new();
            for i in 0..leaf_count {
                if !gone[i] && matches!(class[i], Some(LeafHead::Error)) {
                    match leaves[i].next() {
                        Some(Err(e)) => errors.push(e),
                        _ => unreachable!("peek classified Error"),
                    }
                }
            }

            let active: Vec<usize> = (0..leaf_count).filter(|&i| !gone[i]).collect();
            if active.is_empty() {
                // Every term retired naturally: cap the scan at the range
                // terminus so the client learns it covered the whole range.
                done = true;
                if frontier_advanced(last_emitted, terminus, direction) {
                    return Some(Ok(Watermarked::Watermark(terminus)));
                }
                return None;
            }

            // The floor is the slowest active leaf's scanned-to position; it is
            // the merged "every source has scanned past here" watermark.
            let floor_pos = active
                .iter()
                .map(|&i| front[i])
                .reduce(|a, b| bound_in_direction(a, b, direction))
                .expect("active non-empty");
            if frontier_advanced(last_emitted, floor_pos, direction) {
                pending.push_back(Ok(Watermarked::Watermark(floor_pos)));
                last_emitted = Some(floor_pos);
            }

            // Budget exhausted: the floor watermark above is the resume cursor;
            // everything below it was fully evaluated in prior rounds.
            if !errors.is_empty() {
                done = true;
                pending.push_back(Err(MultiError::collapse(errors)));
                continue;
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

            let mut result: Option<RoaringBitmap> = None;
            for term in &terms {
                if term.dead {
                    continue;
                }
                let includes = take_term_side(
                    &term.includes,
                    floor_bucket,
                    post,
                    &class,
                    &mut front,
                    &mut leaves,
                );
                let excludes = take_term_side(
                    &term.excludes,
                    floor_bucket,
                    post,
                    &class,
                    &mut front,
                    &mut leaves,
                );
                if let Some(bitmap) = eval_term_at_bucket(includes, excludes) {
                    result = Some(match result {
                        None => bitmap,
                        Some(acc) => acc | bitmap,
                    });
                }
            }

            if let Some(bitmap) = result {
                pending.push_back(Ok(Watermarked::Item((floor_bucket, bitmap))));
            }
            if frontier_advanced(last_emitted, post, direction) {
                pending.push_back(Ok(Watermarked::Watermark(post)));
                last_emitted = Some(post);
            }
        }
    })
}

/// Gather one term side's bitmaps at `floor_bucket`, consuming the leaves that
/// sit there (and advancing their `front` to the bucket's trailing edge) and
/// recording `None` for leaves that sit on a later bucket.
fn take_term_side<I>(
    indices: &[usize],
    floor_bucket: u64,
    post: u64,
    class: &[Option<LeafHead>],
    front: &mut [u64],
    leaves: &mut [IterPeekable<I>],
) -> Vec<Option<RoaringBitmap>>
where
    I: Iterator<Item = BucketItem>,
{
    indices
        .iter()
        .map(|&i| {
            if matches!(class[i], Some(LeafHead::Bucket(b)) if b == floor_bucket) {
                front[i] = post;
                match leaves[i].next() {
                    Some(Ok((_, bitmap))) => Some(bitmap),
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use futures::StreamExt;

    use super::*;
    use crate::bitmap_query::BitmapScanBudget;
    use crate::bitmap_query::BitmapTerm;
    use crate::bitmap_query::eval_bitmap_query_bucket_stream;
    use crate::bitmap_query::test_utils::*;

    /// Collect a marked sequence into a comparable `(bucket_id, bits)` /
    /// watermark form.
    fn collect_marked(items: Vec<WatermarkedBucket>) -> Vec<Watermarked<(u64, Vec<u32>)>> {
        items
            .into_iter()
            .map(|r| r.unwrap().map_item(|(b, bm)| (b, bm.iter().collect())))
            .collect()
    }

    fn items_only(marked: &[Watermarked<(u64, Vec<u32>)>]) -> Vec<(u64, Vec<u32>)> {
        marked
            .iter()
            .filter_map(|w| match w {
                Watermarked::Item(it) => Some(it.clone()),
                Watermarked::Watermark(_) => None,
            })
            .collect()
    }

    #[test]
    fn eval_bitmap_query_bucket_iter_uses_iterator_source() {
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

        let out = eval_bitmap_query_bucket_iter(
            source,
            query,
            0..200_000,
            BUCKET_SIZE,
            ScanDirection::Ascending,
        )
        .collect::<Vec<_>>();
        let out = items_only(&collect_marked(out));

        assert_eq!(out, vec![(0, vec![2]), (1, vec![5])]);
    }

    /// The iterator evaluator must produce the exact same `Watermarked` sequence
    /// — items AND progress watermarks — as the stream evaluator for the same
    /// query, since both share the per-bucket DNF evaluation and floor logic.
    #[tokio::test]
    async fn eval_bitmap_query_bucket_iter_matches_stream_for_or_terms() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (
                    test_key(b"a"),
                    vec![(0, vec![1, 2, 3]), (1, vec![5, 6]), (2, vec![9])],
                ),
                (
                    test_key(b"b"),
                    vec![(0, vec![2, 3]), (1, vec![6]), (2, vec![9, 10])],
                ),
                (test_key(b"c"), vec![(0, vec![3]), (2, vec![9])]),
                (test_key(b"d"), vec![(1, vec![1, 8]), (2, vec![7])]),
                (test_key(b"e"), vec![(1, vec![8])]),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b"), exclude(b"c")]).unwrap(),
            BitmapTerm::new(vec![include(b"d"), exclude(b"e")]).unwrap(),
        ])
        .unwrap();

        for direction in [ScanDirection::Ascending, ScanDirection::Descending] {
            let stream_out: Vec<_> = eval_bitmap_query_bucket_stream(
                source.clone(),
                query.clone(),
                0..300_000,
                BUCKET_SIZE,
                direction,
                BitmapScanBudget::new(1_000_000),
            )
            .collect()
            .await;
            let iter_out: Vec<_> = eval_bitmap_query_bucket_iter(
                source.clone(),
                query.clone(),
                0..300_000,
                BUCKET_SIZE,
                direction,
            )
            .collect();

            assert_eq!(
                collect_marked(stream_out),
                collect_marked(iter_out),
                "iter and stream marked sequences diverged for {direction:?}"
            );
        }
    }

    /// Parity holds even when buckets are spread far apart (sparse gaps, leaves
    /// leapfrogging) — the regime where a naive merge could drift between the two
    /// evaluators.
    #[tokio::test]
    async fn eval_bitmap_query_bucket_iter_matches_stream_over_sparse_gaps() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (
                    test_key(b"a"),
                    vec![(0, vec![1, 2, 3]), (5, vec![5, 6]), (9, vec![9])],
                ),
                (
                    test_key(b"b"),
                    vec![(0, vec![2, 3]), (5, vec![6]), (9, vec![9, 10])],
                ),
                (test_key(b"c"), vec![(0, vec![3]), (9, vec![9])]),
                (test_key(b"d"), vec![(3, vec![1, 8]), (7, vec![7])]),
                (test_key(b"e"), vec![(3, vec![8])]),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b"), exclude(b"c")]).unwrap(),
            BitmapTerm::new(vec![include(b"d"), exclude(b"e")]).unwrap(),
        ])
        .unwrap();

        for direction in [ScanDirection::Ascending, ScanDirection::Descending] {
            let stream_out: Vec<_> = eval_bitmap_query_bucket_stream(
                source.clone(),
                query.clone(),
                0..(10 * BUCKET_SIZE),
                BUCKET_SIZE,
                direction,
                BitmapScanBudget::new(1_000_000),
            )
            .collect()
            .await;
            let iter_out: Vec<_> = eval_bitmap_query_bucket_iter(
                source.clone(),
                query.clone(),
                0..(10 * BUCKET_SIZE),
                BUCKET_SIZE,
                direction,
            )
            .collect();

            assert_eq!(
                collect_marked(stream_out),
                collect_marked(iter_out),
                "iter and stream diverged over sparse gaps for {direction:?}"
            );
        }
    }

    /// A sparse intersection that matches nothing in a gap must still emit
    /// watermarks that advance the frontier, and cap with the range terminus.
    #[test]
    fn intersect_emits_coalesced_watermarks_over_sparse_gap() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (test_key(b"a"), vec![(0, vec![1]), (2, vec![5])]),
                (test_key(b"b"), vec![(0, vec![1]), (2, vec![9])]),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b")]).unwrap(),
        ])
        .unwrap();

        let marked = collect_marked(
            eval_bitmap_query_bucket_iter(
                source,
                query,
                0..300_000,
                BUCKET_SIZE,
                ScanDirection::Ascending,
            )
            .collect(),
        );

        // Only bucket 0 intersects (member 1); bucket 2 disjoint -> dropped.
        assert_eq!(items_only(&marked), vec![(0, vec![1])]);

        // Watermarks must be non-decreasing and reach the range terminus.
        let watermarks: Vec<u64> = marked
            .iter()
            .filter_map(|w| match w {
                Watermarked::Watermark(p) => Some(*p),
                Watermarked::Item(_) => None,
            })
            .collect();
        assert!(
            watermarks.windows(2).all(|w| w[0] <= w[1]),
            "ascending watermarks must be non-decreasing: {watermarks:?}"
        );
        assert_eq!(
            watermarks.last().copied(),
            Some(300_000),
            "final watermark must reach the range terminus"
        );
    }
}
