// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Synchronous iterator evaluator for DNF bitmap queries.
//!
//! Mirrors the async [`super::stream`] evaluator's merge-joins and watermark
//! propagation, but stays fully synchronous so request-local backends (e.g.
//! RocksDB) can drive it from the blocking task that owns their iterators. Like
//! the stream evaluator it emits matching members as [`Watermarked`] items
//! interleaved with progress watermarks: each leaf brackets every bucket with
//! `Watermark(pre), Item, Watermark(post)` and caps natural EOF with a
//! range-terminus watermark, and the combinators merge per-child watermarks
//! (min ascending / max descending) so sparse scans still report progress at the
//! rate the slowest source advances.

use std::collections::VecDeque;
use std::iter::Peekable as IterPeekable;
use std::ops::Range;

use anyhow::Result;
use anyhow::bail;
use itertools::Itertools;
use roaring::RoaringBitmap;

use super::BitmapBucketIteratorSource;
use super::BitmapQuery;
use super::BitmapTerm;
use super::BucketItem;
use super::MultiError;
use super::ScanDirection;
use super::Watermarked;
use super::WatermarkedBucket;
use super::complete_peeks;
use super::frontier_advanced;
use super::merge_watermarks;
use super::split_term_literals;

/// Evaluate a DNF `BitmapQuery` as an ordered iterator of marked bucket
/// bitmaps. Output emits `Watermarked::Item((bucket_id, bitmap))` interleaved
/// with `Watermarked::Watermark(p)` merged from the leaf scans.
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
    let scan_bucket_iter = move |dimension_key, range, direction| {
        source.scan_bucket_iter(dimension_key, range, direction)
    };
    let iters: Vec<_> = query
        .terms
        .into_iter()
        .map(|term| {
            term_bucket_iter_with(
                scan_bucket_iter.clone(),
                term,
                range.clone(),
                bucket_size,
                direction,
            )
        })
        .collect();
    union_n_iter(iters, direction)
}

/// Synchronous term evaluator for borrowed/local storage iterators. This
/// mirrors the stream merge-joins, but does not adapt iterators through a stream.
fn term_bucket_iter_with<'a, F, I>(
    scan_bucket_iter: F,
    term: BitmapTerm,
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
) -> Box<dyn Iterator<Item = WatermarkedBucket> + 'a>
where
    F: Fn(Vec<u8>, Range<u64>, ScanDirection) -> I + Clone + 'a,
    I: Iterator<Item = BucketItem> + 'a,
{
    let (include, exclude) = split_term_literals(term);
    let include_iters: Vec<_> = include
        .into_iter()
        .map(|key| {
            marked_bucket_iter(
                scan_bucket_iter(key, range.clone(), direction),
                range.clone(),
                bucket_size,
                direction,
            )
        })
        .collect();
    let include_iter = intersect_n_iter(include_iters, direction);

    // Skip subtract when nothing to exclude. `union_n_iter` of zero iterators
    // emits nothing — not even a terminal watermark — which would leave
    // `subtract_two_iter`'s `b_watermark` slot `None` forever and suppress every
    // merged watermark for this term.
    if exclude.is_empty() {
        return Box::new(include_iter);
    }
    let exclude_iters: Vec<_> = exclude
        .into_iter()
        .map(|key| {
            marked_bucket_iter(
                scan_bucket_iter(key, range.clone(), direction),
                range.clone(),
                bucket_size,
                direction,
            )
        })
        .collect();
    let exclude_iter = union_n_iter(exclude_iters, direction);
    Box::new(subtract_two_iter(include_iter, exclude_iter, direction))
}

enum LeafState {
    Active,
    Terminus,
    Done,
}

/// Wrap a per-dimension raw bucket iterator into a marked iterator, emitting
/// `Watermark(pre), Item, Watermark(post)` per bucket and a final
/// `Watermark(range_terminus)` on natural EOF. On error it yields the error and
/// stops without synthesizing a terminus — the scan did NOT reach it.
///
/// The synchronous analogue of [`super::stream`]'s `budget_limited_bucket_stream`;
/// budget accounting lives in the request layer for the iterator path.
fn marked_bucket_iter<'a, I>(
    inner: I,
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
) -> impl Iterator<Item = WatermarkedBucket> + 'a
where
    I: Iterator<Item = BucketItem> + 'a,
{
    let range_terminus = if direction.is_ascending() {
        range.end
    } else {
        range.start
    };
    let mut inner = inner;
    let mut pending: VecDeque<WatermarkedBucket> = VecDeque::new();
    let mut state = LeafState::Active;

    std::iter::from_fn(move || {
        loop {
            if let Some(out) = pending.pop_front() {
                return Some(out);
            }
            match state {
                LeafState::Done => return None,
                LeafState::Terminus => {
                    state = LeafState::Done;
                    return Some(Ok(Watermarked::Watermark(range_terminus)));
                }
                LeafState::Active => match inner.next() {
                    None => state = LeafState::Terminus,
                    Some(Err(e)) => {
                        state = LeafState::Done;
                        return Some(Err(e));
                    }
                    Some(Ok(item)) => {
                        let (bucket_id, _) = &item;
                        let bucket_start = bucket_id.saturating_mul(bucket_size);
                        let bucket_end_exclusive = bucket_start.saturating_add(bucket_size);
                        // Clamp to the request range: cursors round-trip into
                        // subsequent requests with different ranges, so an
                        // out-of-bound watermark would be a foot-gun.
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
                        pending.push_back(Ok(Watermarked::Watermark(pre)));
                        pending.push_back(Ok(Watermarked::Item(item)));
                        pending.push_back(Ok(Watermarked::Watermark(post)));
                    }
                },
            }
        }
    })
}

/// Multi-way merge intersection. Emits bucket_ids present in every child, with
/// the bitwise AND of their bitmaps; drops empty results. Per-child watermarks
/// drain each iteration; the min/max merged across children emits when it
/// advances.
fn intersect_n_iter<'a, I>(
    iters: Vec<I>,
    direction: ScanDirection,
) -> impl Iterator<Item = WatermarkedBucket> + 'a
where
    I: Iterator<Item = WatermarkedBucket> + 'a,
{
    let mut children: Vec<IterPeekable<I>> = iters.into_iter().map(Iterator::peekable).collect();
    let mut child_watermarks: Vec<Option<u64>> = vec![None; children.len()];
    let mut last_emitted: Option<u64> = None;
    let mut pending: VecDeque<WatermarkedBucket> = VecDeque::new();
    let mut done = false;

    std::iter::from_fn(move || {
        loop {
            if let Some(out) = pending.pop_front() {
                return Some(out);
            }
            if done || children.is_empty() {
                return None;
            }

            // Drain pending watermarks from each child; defer errors until AFTER
            // emitting the merged watermark so progress up to the error point
            // still reaches the consumer.
            let mut deferred_errors: Vec<anyhow::Error> = Vec::new();
            for (i, child) in children.iter_mut().enumerate() {
                let outcome = drain_pending_watermarks_iter(child);
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
                pending.push_back(Ok(Watermarked::Watermark(merged)));
                last_emitted = Some(merged);
            }
            if !deferred_errors.is_empty() {
                done = true;
                pending.push_back(Err(MultiError::collapse(deferred_errors)));
                continue;
            }

            let peeks = match peek_buckets_iter(&mut children) {
                Ok(peeks) => peeks,
                Err(e) => {
                    done = true;
                    pending.push_back(Err(e));
                    continue;
                }
            };
            let Some((peeks, max_bucket)) = complete_peeks(peeks) else {
                done = true;
                continue;
            };
            let target_bucket = match direction {
                ScanDirection::Ascending => max_bucket,
                ScanDirection::Descending => peeks.iter().copied().min().expect("non-empty peeks"),
            };

            if peeks.iter().all(|&b| b == target_bucket) {
                let mut acc: Option<RoaringBitmap> = None;
                let mut err = None;
                for child in children.iter_mut() {
                    match take_bucket_item_iter(child) {
                        Ok((bid, bitmap)) => {
                            debug_assert_eq!(bid, target_bucket);
                            acc = Some(match acc {
                                None => bitmap,
                                Some(a) => a & bitmap,
                            });
                        }
                        Err(e) => {
                            err = Some(e);
                            break;
                        }
                    }
                }
                if let Some(e) = err {
                    done = true;
                    pending.push_back(Err(e));
                    continue;
                }
                let bitmap = acc.expect("children non-empty");
                if !bitmap.is_empty() {
                    pending.push_back(Ok(Watermarked::Item((target_bucket, bitmap))));
                }
            } else {
                // Sparse bitmap rows encode only non-empty buckets. A child
                // lagging the alignment target intersects with an implicit
                // all-zero bitmap at that bucket → empty result. Consume the
                // lagging bucket and re-peek.
                for (i, child) in children.iter_mut().enumerate() {
                    let drop_bucket = match direction {
                        ScanDirection::Ascending => peeks[i] < target_bucket,
                        ScanDirection::Descending => peeks[i] > target_bucket,
                    };
                    if drop_bucket && let Err(e) = take_bucket_item_iter(child) {
                        done = true;
                        pending.push_back(Err(e));
                        break;
                    }
                }
            }
        }
    })
}

/// Multi-way merge union. Emits every bucket_id produced by any child, with the
/// bitwise OR of bitmaps at that bucket. Per-child watermarks drain each
/// iteration; the min/max merged across surviving children emits when it
/// advances.
fn union_n_iter<'a, I>(
    iters: Vec<I>,
    direction: ScanDirection,
) -> impl Iterator<Item = WatermarkedBucket> + 'a
where
    I: Iterator<Item = WatermarkedBucket> + 'a,
{
    let mut children: Vec<IterPeekable<I>> = iters.into_iter().map(Iterator::peekable).collect();
    let mut child_watermarks: Vec<Option<u64>> = vec![None; children.len()];
    let mut last_emitted: Option<u64> = None;
    let mut pending: VecDeque<WatermarkedBucket> = VecDeque::new();
    let mut done = false;

    std::iter::from_fn(move || {
        loop {
            if let Some(out) = pending.pop_front() {
                return Some(out);
            }
            if done || children.is_empty() {
                return None;
            }

            let mut deferred_errors: Vec<anyhow::Error> = Vec::new();
            for (i, child) in children.iter_mut().enumerate() {
                let outcome = drain_pending_watermarks_iter(child);
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
                pending.push_back(Ok(Watermarked::Watermark(merged)));
                last_emitted = Some(merged);
            }
            if !deferred_errors.is_empty() {
                done = true;
                pending.push_back(Err(MultiError::collapse(deferred_errors)));
                continue;
            }

            let peeks = match peek_buckets_iter(&mut children) {
                Ok(peeks) => peeks,
                Err(e) => {
                    done = true;
                    pending.push_back(Err(e));
                    continue;
                }
            };

            // Evict exhausted children. The evicted child's last watermark
            // already drained into the merge above, so dropping its slot is safe.
            let mut surviving_children = Vec::with_capacity(children.len());
            let mut surviving_peeks = Vec::with_capacity(peeks.len());
            let mut surviving_watermarks = Vec::with_capacity(child_watermarks.len());
            for ((child, peek), wm) in std::mem::take(&mut children)
                .into_iter()
                .zip_eq(peeks)
                .zip_eq(std::mem::take(&mut child_watermarks))
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
                done = true;
                continue;
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
            let mut err = None;
            for (i, child) in children.iter_mut().enumerate() {
                if surviving_peeks[i] == next_bucket {
                    match take_bucket_item_iter(child) {
                        Ok((_, bitmap)) => {
                            acc = Some(match acc {
                                None => bitmap,
                                Some(a) => a | bitmap,
                            });
                        }
                        Err(e) => {
                            err = Some(e);
                            break;
                        }
                    }
                }
            }
            if let Some(e) = err {
                done = true;
                pending.push_back(Err(e));
                continue;
            }
            if let Some(bitmap) = acc
                && !bitmap.is_empty()
            {
                pending.push_back(Ok(Watermarked::Item((next_bucket, bitmap))));
            }
        }
    })
}

/// Merge-join subtraction for an anchored negative literal: `a AND NOT b`. For
/// each bucket in `a`, emits `a_bm - b_bm` if `b` has the same bucket, else emits
/// `a_bm` unchanged; drops empty results. Watermarks from both sides merge as
/// "both sources past P."
fn subtract_two_iter<'a, A, B>(
    a: A,
    b: B,
    direction: ScanDirection,
) -> impl Iterator<Item = WatermarkedBucket> + 'a
where
    A: Iterator<Item = WatermarkedBucket> + 'a,
    B: Iterator<Item = WatermarkedBucket> + 'a,
{
    let mut a = a.peekable();
    let mut b = b.peekable();
    let mut a_watermark: Option<u64> = None;
    let mut b_watermark: Option<u64> = None;
    let mut last_emitted: Option<u64> = None;
    let mut pending: VecDeque<WatermarkedBucket> = VecDeque::new();
    let mut done = false;

    std::iter::from_fn(move || {
        loop {
            if let Some(out) = pending.pop_front() {
                return Some(out);
            }
            if done {
                return None;
            }

            let a_outcome = drain_pending_watermarks_iter(&mut a);
            let b_outcome = drain_pending_watermarks_iter(&mut b);
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
                pending.push_back(Ok(Watermarked::Watermark(merged)));
                last_emitted = Some(merged);
            }
            if !deferred_errors.is_empty() {
                done = true;
                pending.push_back(Err(MultiError::collapse(deferred_errors)));
                continue;
            }

            let a_peek = match peek_bucket_iter(&mut a) {
                Ok(peek) => peek,
                Err(e) => {
                    done = true;
                    pending.push_back(Err(e));
                    continue;
                }
            };
            let b_peek = match peek_bucket_iter(&mut b) {
                Ok(peek) => peek,
                Err(e) => {
                    done = true;
                    pending.push_back(Err(e));
                    continue;
                }
            };
            let Some(a_bucket) = a_peek else {
                done = true;
                continue;
            };

            match b_peek {
                None => emit_a_unchanged(&mut a, &mut pending, &mut done),
                Some(bb)
                    if (direction.is_ascending() && bb > a_bucket)
                        || (!direction.is_ascending() && bb < a_bucket) =>
                {
                    // b is ahead, emit a unchanged.
                    emit_a_unchanged(&mut a, &mut pending, &mut done)
                }
                Some(bb)
                    if (direction.is_ascending() && bb < a_bucket)
                        || (!direction.is_ascending() && bb > a_bucket) =>
                {
                    // b is behind; skip it.
                    if let Err(e) = take_bucket_item_iter(&mut b) {
                        done = true;
                        pending.push_back(Err(e));
                    }
                }
                Some(_) => {
                    // Same bucket: subtract.
                    let a_item = take_bucket_item_iter(&mut a);
                    let b_item = take_bucket_item_iter(&mut b);
                    match (a_item, b_item) {
                        (Ok((bid, a_bm)), Ok((_, b_bm))) => {
                            let diff = a_bm - b_bm;
                            if !diff.is_empty() {
                                pending.push_back(Ok(Watermarked::Item((bid, diff))));
                            }
                        }
                        (Err(e), _) | (_, Err(e)) => {
                            done = true;
                            pending.push_back(Err(e));
                        }
                    }
                }
            }
        }
    })
}

/// Consume the next `a` bucket and queue it unchanged (skipping empties).
fn emit_a_unchanged<A>(
    a: &mut IterPeekable<A>,
    pending: &mut VecDeque<WatermarkedBucket>,
    done: &mut bool,
) where
    A: Iterator<Item = WatermarkedBucket>,
{
    match take_bucket_item_iter(a) {
        Ok((bid, bitmap)) => {
            if !bitmap.is_empty() {
                pending.push_back(Ok(Watermarked::Item((bid, bitmap))));
            }
        }
        Err(e) => {
            *done = true;
            pending.push_back(Err(e));
        }
    }
}

/// Outcome of `drain_pending_watermarks_iter`: latest watermark consumed (if
/// any) AND any terminal error at the same head. Combinators apply the watermark
/// before propagating the error so mid-drain progress still surfaces.
struct DrainOutcome {
    last_watermark: Option<u64>,
    error: Option<anyhow::Error>,
}

/// Drain consecutive `Watermark` frames from the head of a peekable marked
/// iterator. Combinators call this at the top of each loop iteration so
/// subsequent peeks see Items only.
fn drain_pending_watermarks_iter<I>(s: &mut IterPeekable<I>) -> DrainOutcome
where
    I: Iterator<Item = WatermarkedBucket>,
{
    let mut last: Option<u64> = None;
    loop {
        match s.peek() {
            None | Some(Ok(Watermarked::Item(_))) => {
                return DrainOutcome {
                    last_watermark: last,
                    error: None,
                };
            }
            Some(Ok(Watermarked::Watermark(_))) => match s.next() {
                Some(Ok(Watermarked::Watermark(p))) => last = Some(p),
                _ => unreachable!("peek confirmed Watermark"),
            },
            Some(Err(_)) => match s.next() {
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

/// Consume the next bucket Item. Caller must have confirmed via `peek_bucket_iter`
/// that the head is an Item — a watermark or EOF here is a logic error.
fn take_bucket_item_iter<I>(s: &mut IterPeekable<I>) -> Result<(u64, RoaringBitmap)>
where
    I: Iterator<Item = WatermarkedBucket>,
{
    match s.next() {
        Some(Ok(Watermarked::Item(it))) => Ok(it),
        Some(Ok(Watermarked::Watermark(_))) => {
            unreachable!("take_bucket_item_iter on Watermark — drain should have run first")
        }
        Some(Err(e)) => Err(e),
        None => unreachable!("take_bucket_item_iter on EOF — peek should have caught it"),
    }
}

/// Peek the next bucket_id. Caller MUST have run `drain_pending_watermarks_iter`
/// this iteration — a Watermark at the head means the drain step was skipped,
/// which is a refactor bug, not a runtime condition. `None` on EOF; errors via
/// `?`.
fn peek_bucket_iter<I>(s: &mut IterPeekable<I>) -> Result<Option<u64>>
where
    I: Iterator<Item = WatermarkedBucket>,
{
    match s.peek() {
        None => Ok(None),
        Some(Ok(Watermarked::Item((b, _)))) => Ok(Some(*b)),
        Some(Ok(Watermarked::Watermark(_))) => {
            // Surface rather than silently consume: a stray WM here means the
            // per-iteration drain contract was violated by a future refactor.
            bail!(
                "peek_bucket_iter observed a stray Watermark — drain_pending_watermarks_iter \
                 must run first in each combinator loop iteration"
            );
        }
        Some(Err(_)) => match s.next() {
            Some(Err(e)) => Err(e),
            _ => unreachable!("peek confirmed Err"),
        },
    }
}

fn peek_buckets_iter<I>(iters: &mut [IterPeekable<I>]) -> Result<Vec<Option<u64>>>
where
    I: Iterator<Item = WatermarkedBucket>,
{
    iters.iter_mut().map(peek_bucket_iter).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use futures::StreamExt;

    use super::*;
    use crate::bitmap_query::BitmapScanBudget;
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
    /// query, since both share the merge/watermark logic.
    #[tokio::test]
    async fn eval_bitmap_query_bucket_iter_matches_stream_for_or_terms_descending() {
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

            let stream_marked = collect_marked(stream_out);
            let iter_marked = collect_marked(iter_out);
            assert_eq!(
                stream_marked, iter_marked,
                "iter and stream marked sequences diverged for {direction:?}"
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

    #[test]
    fn peek_bucket_iter_propagates_errors_without_panicking() {
        let mut iter = vec![Err::<Watermarked<(u64, RoaringBitmap)>, anyhow::Error>(
            anyhow::anyhow!("boom"),
        )]
        .into_iter()
        .peekable();

        let err = peek_bucket_iter(&mut iter).unwrap_err();

        assert!(err.to_string().contains("boom"));
        assert!(iter.next().is_none());
    }
}
