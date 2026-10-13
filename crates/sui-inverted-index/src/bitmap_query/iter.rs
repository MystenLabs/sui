// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Synchronous iterator evaluator for DNF bitmap queries.
//!
//! A single flat driver merge-joins every leaf scan against one shared *floor*
//! (the slowest leaf's position). At the floor bucket it evaluates the query —
//! intersect each term's included dimensions, subtract its excluded ones, then
//! union across terms — and emits a watermark at the floor. Because leaves only
//! ever advance at the floor (peeked one bucket ahead), no branch can run ahead
//! of the others: the resume cursor is always within one sparse read of every
//! leaf, and there is no windowing/parking machinery to get wrong. Mirrors the
//! async [`super::stream`] evaluator, which shares the per-bucket evaluation
//! ([`eval_term_at_bucket`]) and is cross-checked against this one in tests.
//!
//! Like the stream evaluator, this evaluator owns the scan budget: it wraps
//! every backend leaf iterator in [`BudgetedBucketIter`], which charges each
//! pulled bucket row to one budget shared by all leaves and turns exhaustion
//! into [`LeafStop::BudgetExhausted`]. Backend iterators only scan storage and
//! report their own stops (cancellation, faults).
//!
//! Watermark emission is paced by a [`WatermarkPacing`]: [`PacedBucketIter`]
//! applies it to the unpaced evaluation's output.

use std::collections::VecDeque;
use std::ops::Range;

use roaring::RoaringBitmap;

use super::BitmapBucketIteratorSource;
use super::BitmapQuery;
use super::BitmapScanBudget;
use super::BitmapScanMetrics;
use super::BucketItem;
use super::DedupedQuery;
use super::LeafHead;
use super::LeafStop;
use super::ObserveOnDrop;
use super::ScanDirection;
use super::ScanStop;
use super::SkipPolicy;
use super::WatermarkPacer;
use super::WatermarkPacing;
use super::Watermarked;
use super::WatermarkedBucket;
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

struct Leaf<I> {
    iter: I,
    peeked: Option<Option<BucketItem>>,
    drained: u64,
}

impl<I: Iterator<Item = BucketItem>> Leaf<I> {
    fn new(iter: I) -> Self {
        Self {
            iter,
            peeked: None,
            drained: 0,
        }
    }

    fn peek(&mut self) -> Option<&BucketItem> {
        if self.peeked.is_none() {
            self.peeked = Some(self.iter.next());
        }
        self.peeked.as_ref().and_then(Option::as_ref)
    }

    fn next(&mut self) -> Option<BucketItem> {
        match self.peeked.take() {
            Some(item) => item,
            None => self.iter.next(),
        }
    }
}

impl<I: super::SeekableBucketIterator> Leaf<I> {
    /// Reposition the leaf so its next bucket is the first at or past `bucket`,
    /// dropping whatever sits in the peek slot. Returns whether the dropped
    /// head was a fetched (and therefore charged) bucket row.
    fn seek_bucket(&mut self, bucket: u64) -> bool {
        let discarded_row = matches!(self.peeked.as_ref(), Some(Some(Ok(_))));
        self.peeked = None;
        self.iter.seek_bucket(bucket);
        discarded_row
    }
}

/// Leaf scan charged against the evaluation's shared [`BitmapScanBudget`].
/// Mirrors the stream evaluator's `budgeted_bucket_stream`: the leaf's first
/// bucket row is charged with `take_first` (always succeeds, so every leaf can
/// report its first position), later rows with `try_take`. On underflow it
/// yields [`LeafStop::BudgetExhausted`] once and then ends — never a silent
/// EOF. Backend stops pass through uncharged.
struct BudgetedBucketIter<I> {
    inner: I,
    budget: BitmapScanBudget,
    first_bucket_pending: bool,
    exhausted: bool,
}

impl<I> BudgetedBucketIter<I> {
    fn new(inner: I, budget: BitmapScanBudget) -> Self {
        Self {
            inner,
            budget,
            first_bucket_pending: true,
            exhausted: false,
        }
    }
}

impl<I: Iterator<Item = BucketItem>> Iterator for BudgetedBucketIter<I> {
    type Item = BucketItem;

    fn next(&mut self) -> Option<BucketItem> {
        if self.exhausted {
            return None;
        }
        let item = self.inner.next()?;
        if item.is_err() {
            return Some(item);
        }
        if self.first_bucket_pending {
            self.budget.take_first();
            self.first_bucket_pending = false;
        } else if !self.budget.try_take() {
            self.exhausted = true;
            return Some(Err(LeafStop::BudgetExhausted));
        }
        Some(item)
    }
}

impl<I: super::SeekableBucketIterator> super::SeekableBucketIterator for BudgetedBucketIter<I> {
    /// A seek repositions the same logical leaf, so the first-bucket
    /// reservation is not granted again.
    fn seek_bucket(&mut self, bucket: u64) {
        self.inner.seek_bucket(bucket);
    }
}

/// The evaluator's unpaced output with a [`WatermarkPacer`] applied. When the
/// unpaced iterator ends, it emits the watermark [`WatermarkPacer::finish`]
/// returns, if any, and then stays ended.
struct PacedBucketIter<I> {
    unpaced: I,
    pacer: WatermarkPacer,
    unpaced_ended: bool,
}

impl<I: Iterator<Item = WatermarkedBucket>> Iterator for PacedBucketIter<I> {
    type Item = WatermarkedBucket;

    fn next(&mut self) -> Option<WatermarkedBucket> {
        while !self.unpaced_ended {
            match self.unpaced.next() {
                Some(output) => {
                    if let Some(paced) = self.pacer.pace(output) {
                        return Some(paced);
                    }
                }
                None => {
                    self.unpaced_ended = true;
                    return self
                        .pacer
                        .finish()
                        .map(|position| Ok(Watermarked::Watermark(position)));
                }
            }
        }
        None
    }
}

/// Evaluate a DNF `BitmapQuery` as an ordered iterator of marked bucket bitmaps.
/// Output emits `Watermarked::Item((bucket_id, bitmap))` interleaved with
/// `Watermarked::Watermark(p)` derived from the slowest leaf's progress.
///
/// `budget` caps the bucket rows charged across all leaf scans, including dead
/// rows drained during gap catch-up; exhaustion ends the iterator with
/// `ScanStop::ScanLimit` carrying the merged frontier. `on_metrics` fires
/// exactly once when the returned iterator is dropped, whether or not it was
/// polled.
///
/// `pacing` decides which watermarks are emitted (see [`WatermarkPacing`]);
/// the bucket rows it counts are the ones charged against `budget`.
///
/// Unlike [`super::eval_bitmap_query_stream`], there is no `budget >=
/// unique_leaf_count` misconfiguration guard: callers such as the fullnode
/// pass a per-chunk budget that legitimately falls below the leaf count near
/// the end of a request, and the per-leaf first-bucket reservation still lets
/// every leaf report its first position.
pub fn eval_bitmap_query_bucket_iter<'a, S, F>(
    source: S,
    query: BitmapQuery,
    range: Range<u64>,
    bucket_size: u64,
    direction: ScanDirection,
    budget: u64,
    policy: SkipPolicy,
    pacing: WatermarkPacing,
    on_metrics: F,
) -> impl Iterator<Item = WatermarkedBucket> + 'a
where
    S: BitmapBucketIteratorSource<'a>,
    F: FnOnce(BitmapScanMetrics) + 'a,
{
    // Build one leaf per unique dimension key. Every term addresses these
    // deduplicated leaves by index, so a shared dimension is scanned once.
    let DedupedQuery {
        keys: unique_keys,
        mut terms,
    } = build_term_specs(query.terms);
    let budget = BitmapScanBudget::new(budget);
    let mut leaves: Vec<Leaf<BudgetedBucketIter<S::Iter>>> = Vec::with_capacity(unique_keys.len());
    for key in unique_keys {
        leaves.push(Leaf::new(BudgetedBucketIter::new(
            source.scan_bucket_iter(key, range.clone(), direction),
            budget.clone(),
        )));
    }
    // The iterator closure owns the guard, so `on_metrics` fires when the
    // caller drops the returned iterator.
    let observer = ObserveOnDrop {
        budget: budget.clone(),
        callback: Some(on_metrics),
    };
    let pacer = WatermarkPacer::new(pacing, policy, budget.clone(), range.clone(), bucket_size);

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
    // `unreferenced[i]` retires a leaf once no satisfiable term references it
    // or its scan is permanently exhausted.
    let mut unreferenced = vec![false; leaf_count];
    // `front[i]` is the furthest position proven safe for this leaf, either by
    // physical scanning or by a conjunction's leapfrog bound. The slowest live
    // front limits the resume cursor.
    let mut front = vec![request_floor; leaf_count];
    // The request floor is a baseline, not progress earned by evaluation.
    let mut progress_frontier = Some(request_floor);
    let mut done = false;
    let mut pending: VecDeque<WatermarkedBucket> = VecDeque::new();

    let unpaced = std::iter::from_fn(move || {
        let _observer = &observer;
        loop {
            if let Some(out) = pending.pop_front() {
                return Some(out);
            }
            if done {
                return None;
            }

            // Peek every active leaf without consuming it, then classify its
            // current bucket, EOF, or error state.
            let mut class: Vec<Option<LeafHead>> = (0..leaf_count).map(|_| None).collect();
            for i in 0..leaf_count {
                if unreferenced[i] {
                    continue;
                }
                match leaves[i].peek() {
                    Some(Ok((bucket, _))) => {
                        let (pre, _) = bucket_edges(*bucket, bucket_size, &range, direction);
                        front[i] = advance_in_direction(front[i], pre, direction);
                        class[i] = Some(LeafHead::Bucket(*bucket));
                    }
                    None => {
                        front[i] = advance_in_direction(front[i], terminus, direction);
                        class[i] = Some(LeafHead::Eof);
                    }
                    Some(Err(_)) => class[i] = Some(LeafHead::Error),
                }
            }

            // An include at EOF makes its conjunction permanently empty. A
            // deduplicated include may make several conjunctions unsatisfiable.
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
            // A shared leaf remains live until no satisfiable conjunction
            // references it, whether as an include or an exclude.
            recompute_unreferenced(&terms, &class, &mut unreferenced);

            // Leapfrog bounds advance logical progress before the physical
            // iterator drains or seeks across the corresponding dead rows.
            let targets = leaf_skip_targets(&terms, &class, &unreferenced, direction);
            for (i, target) in targets.iter().enumerate() {
                if let Some(target) = target {
                    let (pre, _) = bucket_edges(*target, bucket_size, &range, direction);
                    front[i] = advance_in_direction(front[i], pre, direction);
                }
            }

            // Consume stop frames so they surface. `collapse` attaches this
            // round's proven-safe floor to scan-limit errors.
            let mut errors: Vec<LeafStop> = Vec::new();
            for i in 0..leaf_count {
                if !unreferenced[i] && matches!(class[i], Some(LeafHead::Error)) {
                    match leaves[i].next() {
                        Some(Err(error)) => errors.push(error),
                        _ => unreachable!("peek classified Error"),
                    }
                }
            }

            let active: Vec<usize> = (0..leaf_count).filter(|&i| !unreferenced[i]).collect();
            // Natural completion is represented by the caller's terminal
            // boundary; only progress earned here is emitted.
            if active.is_empty() {
                done = true;
                return None;
            }

            // The floor is the slowest active leaf's proven-safe position and
            // therefore the furthest safe merged watermark.
            let floor_pos = active
                .iter()
                .map(|&i| front[i])
                .reduce(|a, b| bound_in_direction(a, b, direction))
                .expect("active non-empty");
            let collapsed = (!errors.is_empty()).then(|| collapse(errors, floor_pos));
            let scan_limited = matches!(collapsed, Some(ScanStop::ScanLimit { .. }));
            if !scan_limited && frontier_advanced(progress_frontier, floor_pos, direction) {
                pending.push_back(Ok(Watermarked::Watermark(floor_pos)));
                progress_frontier = Some(floor_pos);
            }
            if let Some(stop) = collapsed {
                done = true;
                pending.push_back(Err(stop));
                continue;
            }

            // A target marks a leaf whose logical frontier is ahead of its
            // physical iterator. Rows before that target are proven dead.
            let lagging: Vec<usize> = active
                .iter()
                .copied()
                .filter(|&i| targets[i].is_some())
                .collect();
            // Preserve the drain count while a leaf remains lagging so a moving
            // target cannot repeatedly restart its probe allowance.
            for i in 0..leaf_count {
                if targets[i].is_none() {
                    leaves[i].drained = 0;
                }
            }

            // Lagging heads cannot participate yet. Select the next physical
            // bucket from leaves that are ready for evaluation.
            let eval_bucket = active
                .iter()
                .filter(|&&i| targets[i].is_none())
                .filter_map(|&i| match class[i] {
                    Some(LeafHead::Bucket(bucket)) => Some(bucket),
                    _ => None,
                })
                .reduce(|a, b| bound_in_direction(a, b, direction))
                .expect("a least term candidate leaf is not lagging");
            // Ready leaves may be evaluated strictly before the nearest lagging
            // target. Equality waits so the lagging leaf joins that snapshot.
            let lagging_target = lagging
                .iter()
                .filter_map(|&i| targets[i])
                .reduce(|a, b| bound_in_direction(a, b, direction));
            let evaluate =
                lagging_target.is_none_or(|target| strictly_before(eval_bucket, target, direction));

            if evaluate {
                let (_, post) = bucket_edges(eval_bucket, bucket_size, &range, direction);
                // Consume each ready leaf at this bucket once. Shared terms
                // reuse its snapshot instead of advancing its iterator twice.
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
                        snapshot[i] = match leaves[i].next() {
                            Some(Ok((_, bitmap))) => Some(bitmap),
                            _ => None,
                        };
                    }
                }
                // Evaluate each conjunction from the shared snapshot, then
                // union non-empty bitmaps to implement the top-level OR.
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
                            take_snapshot_bitmap(&mut snapshot, &mut remaining_refs, &on_floor, i)
                        })
                        .collect();
                    let excludes = term
                        .excludes
                        .iter()
                        .map(|&i| {
                            take_snapshot_bitmap(&mut snapshot, &mut remaining_refs, &on_floor, i)
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
                    pending.push_back(Ok(Watermarked::Item((eval_bucket, bitmap))));
                }
                if frontier_advanced(progress_frontier, post, direction) {
                    pending.push_back(Ok(Watermarked::Watermark(post)));
                    progress_frontier = Some(post);
                }
            }

            // Physically catch up lagging leaves only after emitting any safe
            // earlier bucket, because catch-up itself can exhaust the budget.
            let seeks_before_catch_up = budget.seeks();
            for i in lagging {
                let target = targets[i].expect("lagging leaf has target");
                loop {
                    let dead_row = matches!(
                        leaves[i].peek(),
                        Some(Ok((bucket, _))) if strictly_before(*bucket, target, direction)
                    );
                    if !dead_row {
                        break;
                    }
                    if policy
                        .drain_probe_rows
                        .is_some_and(|probe| leaves[i].drained >= u64::from(probe.get()))
                    {
                        // A dead row in the peek slot was already charged
                        // through the budgeted leaf; the seek discards it.
                        if leaves[i].seek_bucket(target) {
                            budget.note_discarded();
                        }
                        budget.note_seek();
                        break;
                    }
                    // Consuming a dead row keeps using the open scan and
                    // counts both the charged read and the discard.
                    let discarded = leaves[i].next();
                    debug_assert!(matches!(discarded, Some(Ok(_))));
                    leaves[i].drained += 1;
                    budget.note_discarded();
                }
            }
            if budget.seeks() > seeks_before_catch_up {
                budget.note_reopen_step();
            }
        }
    });
    PacedBucketIter {
        unpaced,
        pacer,
        unpaced_ended: false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use futures::StreamExt;

    use super::*;
    use crate::bitmap_query::BitmapBucketSource;
    use crate::bitmap_query::BitmapTerm;
    use crate::bitmap_query::eval_bitmap_query_bucket_stream;
    use crate::bitmap_query::eval_bitmap_query_stream;
    use crate::bitmap_query::flatten_watermarked_buckets;
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
            1_000_000,
            SkipPolicy::DRAIN_ONLY,
            EVERY_WATERMARK,
            |_| {},
        )
        .collect::<Vec<_>>();
        let out = items_only(&collect_marked(out));

        assert_eq!(out, vec![(0, vec![2]), (1, vec![5])]);
    }

    /// Two terms share the same include `a`. The iter evaluator must collapse
    /// them to a single backend scan and distribute its per-bucket bitmap to
    /// both terms. Mirrors the stream-side
    /// `shared_include_across_terms_scans_dimension_once` test so the dedup
    /// invariant is exercised on both evaluators.
    #[test]
    fn shared_include_across_terms_scans_dimension_once() {
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

        let out = items_only(&collect_marked(
            eval_bitmap_query_bucket_iter(
                source.clone(),
                query,
                0..200_000,
                BUCKET_SIZE,
                ScanDirection::Ascending,
                1_000_000,
                SkipPolicy::DRAIN_ONLY,
                EVERY_WATERMARK,
                |_| {},
            )
            .collect(),
        ));

        // Bucket 0: term1 = a∩b = {1}; term2 = a∩c = {2}; union = {1, 2}.
        assert_eq!(out, vec![(0, vec![1, 2])]);
        assert_eq!(source.scan_count(&test_key(b"a")), 1);
        assert_eq!(source.scan_count(&test_key(b"b")), 1);
        assert_eq!(source.scan_count(&test_key(b"c")), 1);
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
            for policy in [
                SkipPolicy::DRAIN_ONLY,
                SkipPolicy {
                    drain_probe_rows: std::num::NonZeroU32::new(2),
                },
            ] {
                let stream_out: Vec<_> = eval_bitmap_query_bucket_stream(
                    source.clone(),
                    query.clone(),
                    0..300_000,
                    BUCKET_SIZE,
                    direction,
                    BitmapScanBudget::new(1_000_000),
                    policy,
                )
                .collect()
                .await;
                let iter_out: Vec<_> = eval_bitmap_query_bucket_iter(
                    source.clone(),
                    query.clone(),
                    0..300_000,
                    BUCKET_SIZE,
                    direction,
                    1_000_000,
                    policy,
                    EVERY_WATERMARK,
                    |_| {},
                )
                .collect();

                assert_eq!(
                    collect_marked(stream_out),
                    collect_marked(iter_out),
                    "iter and stream marked sequences diverged for {direction:?}"
                );
            }
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
            for policy in [
                SkipPolicy::DRAIN_ONLY,
                SkipPolicy {
                    drain_probe_rows: std::num::NonZeroU32::new(2),
                },
            ] {
                let stream_out: Vec<_> = eval_bitmap_query_bucket_stream(
                    source.clone(),
                    query.clone(),
                    0..(10 * BUCKET_SIZE),
                    BUCKET_SIZE,
                    direction,
                    BitmapScanBudget::new(1_000_000),
                    policy,
                )
                .collect()
                .await;
                let iter_out: Vec<_> = eval_bitmap_query_bucket_iter(
                    source.clone(),
                    query.clone(),
                    0..(10 * BUCKET_SIZE),
                    BUCKET_SIZE,
                    direction,
                    1_000_000,
                    policy,
                    EVERY_WATERMARK,
                    |_| {},
                )
                .collect();

                assert_eq!(
                    collect_marked(stream_out),
                    collect_marked(iter_out),
                    "iter and stream diverged over sparse gaps for {direction:?}"
                );
            }
        }
    }

    /// A sparse intersection that matches nothing in a gap still emits earned
    /// progress, including the last bucket's post edge at the range terminus.
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
                1_000_000,
                SkipPolicy::DRAIN_ONLY,
                EVERY_WATERMARK,
                |_| {},
            )
            .collect(),
        );

        // Only bucket 0 intersects (member 1); bucket 2 disjoint -> dropped.
        assert_eq!(items_only(&marked), vec![(0, vec![1])]);

        // Watermarks must be non-decreasing; the last bucket earns the terminus.
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
            "last bucket's post edge must reach the range terminus"
        );
    }

    #[test]
    fn descending_request_floor_is_not_emitted_as_progress() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([(
                test_key(b"a"),
                vec![(2, vec![0, 50_000])],
            )])),
        };
        let query = BitmapQuery::new(vec![BitmapTerm::new(vec![include(b"a")]).unwrap()]).unwrap();

        let marked = collect_marked(
            eval_bitmap_query_bucket_iter(
                source,
                query,
                50..(2 * BUCKET_SIZE + 50_001),
                BUCKET_SIZE,
                ScanDirection::Descending,
                1_000_000,
                SkipPolicy::DRAIN_ONLY,
                EVERY_WATERMARK,
                |_| {},
            )
            .collect(),
        );

        assert_eq!(
            marked,
            vec![
                Watermarked::Item((2, vec![0, 50_000])),
                Watermarked::Watermark(2 * BUCKET_SIZE),
            ],
        );
    }

    #[test]
    fn natural_completion_omits_terminus_but_retains_earned_progress() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([(test_key(b"a"), vec![(3, vec![5])])])),
        };
        let query = BitmapQuery::new(vec![BitmapTerm::new(vec![include(b"a")]).unwrap()]).unwrap();

        let marked = collect_marked(
            eval_bitmap_query_bucket_iter(
                source,
                query,
                0..(5 * BUCKET_SIZE),
                BUCKET_SIZE,
                ScanDirection::Ascending,
                1_000_000,
                SkipPolicy::DRAIN_ONLY,
                EVERY_WATERMARK,
                |_| {},
            )
            .collect(),
        );

        assert_eq!(
            marked,
            vec![
                Watermarked::Watermark(3 * BUCKET_SIZE),
                Watermarked::Item((3, vec![5])),
                Watermarked::Watermark(4 * BUCKET_SIZE),
            ],
        );
    }

    /// An unanchored term (`NOT x`, anchored on the synthesized universe leaf)
    /// emits the complement at exclude-occupied buckets, full bitmaps at gap
    /// buckets, and keeps emitting full buckets after the exclude leaf EOFs.
    #[test]
    fn unanchored_term_emits_complement_over_gaps_and_past_exclude_eof() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([(
                test_key(b"x"),
                vec![(0, vec![1, 2]), (2, vec![5])],
            )])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include_universe(), exclude(b"x")]).unwrap(),
        ])
        .unwrap();

        let items: Vec<(u64, RoaringBitmap)> = eval_bitmap_query_bucket_iter(
            source,
            query,
            0..(4 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            1_000_000,
            SkipPolicy::DRAIN_ONLY,
            EVERY_WATERMARK,
            |_| {},
        )
        .filter_map(|r| match r.unwrap() {
            Watermarked::Item(it) => Some(it),
            Watermarked::Watermark(_) => None,
        })
        .collect();

        let complement = |bits: &[u32]| {
            let mut bm = full_bucket();
            for &b in bits {
                bm.remove(b);
            }
            bm
        };
        let expected = vec![
            (0, complement(&[1, 2])),
            (1, full_bucket()),
            (2, complement(&[5])),
            (3, full_bucket()),
        ];
        assert_eq!(items, expected);
    }

    /// Iter/stream parity for a mixed DNF with an unanchored term, in both
    /// directions — the universe leaf forces dense bucket coverage while the
    /// anchored term stays sparse.
    #[tokio::test]
    async fn eval_bitmap_query_bucket_iter_matches_stream_for_unanchored_terms() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (test_key(b"a"), vec![(1, vec![7])]),
                (test_key(b"x"), vec![(0, vec![1, 2]), (3, vec![5])]),
            ])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a")]).unwrap(),
            BitmapTerm::new(vec![include_universe(), exclude(b"x")]).unwrap(),
        ])
        .unwrap();

        for direction in [ScanDirection::Ascending, ScanDirection::Descending] {
            for policy in [
                SkipPolicy::DRAIN_ONLY,
                SkipPolicy {
                    drain_probe_rows: std::num::NonZeroU32::new(2),
                },
            ] {
                let stream_out: Vec<_> = eval_bitmap_query_bucket_stream(
                    source.clone(),
                    query.clone(),
                    0..(5 * BUCKET_SIZE),
                    BUCKET_SIZE,
                    direction,
                    BitmapScanBudget::new(1_000_000),
                    policy,
                )
                .collect()
                .await;
                let iter_out: Vec<_> = eval_bitmap_query_bucket_iter(
                    source.clone(),
                    query.clone(),
                    0..(5 * BUCKET_SIZE),
                    BUCKET_SIZE,
                    direction,
                    1_000_000,
                    policy,
                    EVERY_WATERMARK,
                    |_| {},
                )
                .collect();

                assert_eq!(
                    collect_marked(stream_out),
                    collect_marked(iter_out),
                    "iter and stream diverged on unanchored terms for {direction:?}"
                );
            }
        }
    }

    /// Budget exhaustion mid-dense-scan bundles the merged floor in the
    /// terminal, and resuming from that frontier covers every remaining bucket
    /// exactly once without relying on a terminal-round progress beacon.
    #[tokio::test]
    async fn unanchored_budget_exhaustion_resumes_at_terminal_frontier() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::new()),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include_universe(), exclude(b"x")]).unwrap(),
        ])
        .unwrap();

        let first: Vec<_> = eval_bitmap_query_bucket_stream(
            source.clone(),
            query.clone(),
            0..(10 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            BitmapScanBudget::new(3),
            SkipPolicy::DRAIN_ONLY,
        )
        .collect()
        .await;

        let mut covered: Vec<u64> = Vec::new();
        let mut resume_from = None;
        let mut limit_hit = false;
        for item in first {
            match item {
                Ok(Watermarked::Item((bucket, bitmap))) => {
                    assert_eq!(bitmap, full_bucket());
                    covered.push(bucket);
                }
                Ok(Watermarked::Watermark(_)) => {}
                Err(ScanStop::ScanLimit { scan_frontier }) => {
                    resume_from = Some(scan_frontier);
                    limit_hit = true;
                }
                Err(other) => panic!("expected ScanLimit, got {other:?}"),
            }
        }
        assert!(limit_hit, "3-bucket budget cannot cover 10 dense buckets");
        assert_eq!(covered, vec![0, 1, 2]);
        let resume_from = resume_from.expect("ScanLimit must carry a resume frontier");
        assert_eq!(resume_from, 3 * BUCKET_SIZE);

        let resumed: Vec<_> = eval_bitmap_query_bucket_stream(
            source,
            query,
            resume_from..(10 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            BitmapScanBudget::new(1_000_000),
            SkipPolicy::DRAIN_ONLY,
        )
        .collect()
        .await;
        for item in resumed {
            if let Watermarked::Item((bucket, _)) = item.unwrap() {
                covered.push(bucket);
            }
        }
        assert_eq!(covered, (0..10).collect::<Vec<_>>());
    }
    #[tokio::test]
    async fn iter_seeks_lagging_leaf_natively() {
        let source = CountingBucketSource::new(BTreeMap::from([
            (test_key(b"a"), vec![(0, vec![1]), (50, vec![1])]),
            (
                test_key(b"b"),
                (0..=50).map(|bucket| (bucket, vec![1])).collect(),
            ),
        ]));
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b")]).unwrap(),
        ])
        .unwrap();
        let policy = SkipPolicy {
            drain_probe_rows: std::num::NonZeroU32::new(2),
        };

        let stream_out = eval_bitmap_query_bucket_stream(
            source.clone(),
            query.clone(),
            0..(51 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            BitmapScanBudget::new(1_000),
            policy,
        )
        .collect()
        .await;
        let iter_out = eval_bitmap_query_bucket_iter(
            source.clone(),
            query,
            0..(51 * BUCKET_SIZE),
            BUCKET_SIZE,
            ScanDirection::Ascending,
            1_000,
            policy,
            EVERY_WATERMARK,
            |_| {},
        )
        .collect();

        assert_eq!(collect_marked(stream_out), collect_marked(iter_out));
        assert_eq!(source.seek_count(&test_key(b"b")), 1);
    }

    /// One evaluator output in comparable form: an item as `(bucket_id, bits)`,
    /// a watermark, or a scan-limit stop as `Err(scan_frontier)`.
    type ObservedOutput = Result<Watermarked<(u64, Vec<u32>)>, u64>;

    /// Comparable form of an evaluator's output; see [`ObservedOutput`].
    fn observe(outputs: Vec<WatermarkedBucket>) -> Vec<ObservedOutput> {
        outputs
            .into_iter()
            .map(|output| match output {
                Ok(marked) => {
                    Ok(marked.map_item(|(bucket, bitmap)| (bucket, bitmap.iter().collect())))
                }
                Err(ScanStop::ScanLimit { scan_frontier }) => Err(scan_frontier),
                Err(other) => panic!("expected only scan-limit stops, got {other:?}"),
            })
            .collect()
    }

    /// Run both evaluators with every budget in `budgets`, in both directions
    /// and under both skip policies, and require identical items, watermarks
    /// and scan-limit frontiers. Also requires that some runs stop on the
    /// budget and some complete, so the comparison covers exhaustion.
    async fn assert_iter_matches_stream_under_budgets(
        source: TestBucketSource,
        query: BitmapQuery,
        range: Range<u64>,
        budgets: std::ops::RangeInclusive<u64>,
    ) {
        let mut stopped = 0;
        let mut completed = 0;
        for budget in budgets {
            for direction in [ScanDirection::Ascending, ScanDirection::Descending] {
                for policy in [
                    SkipPolicy::DRAIN_ONLY,
                    SkipPolicy {
                        drain_probe_rows: std::num::NonZeroU32::new(2),
                    },
                ] {
                    let stream_out = observe(
                        eval_bitmap_query_bucket_stream(
                            source.clone(),
                            query.clone(),
                            range.clone(),
                            BUCKET_SIZE,
                            direction,
                            BitmapScanBudget::new(budget),
                            policy,
                        )
                        .collect()
                        .await,
                    );
                    let iter_out = observe(
                        eval_bitmap_query_bucket_iter(
                            source.clone(),
                            query.clone(),
                            range.clone(),
                            BUCKET_SIZE,
                            direction,
                            budget,
                            policy,
                            EVERY_WATERMARK,
                            |_| {},
                        )
                        .collect(),
                    );

                    assert_eq!(
                        stream_out, iter_out,
                        "iter and stream diverged with budget {budget}, {direction:?}, {policy:?}"
                    );
                    if matches!(iter_out.last(), Some(Err(_))) {
                        stopped += 1;
                    } else {
                        completed += 1;
                    }
                }
            }
        }
        assert!(stopped > 0, "no budget in the sweep exhausted the scan");
        assert!(completed > 0, "no budget in the sweep completed the scan");
    }

    /// `a AND b` where `b` is dense and `a` jumps from bucket 0 to 50, so
    /// catching `b` up drains (and, with a probe policy, seeks past) dead rows.
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

    fn skewed_and_query() -> BitmapQuery {
        BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"b")]).unwrap(),
        ])
        .unwrap()
    }

    /// Budget exhaustion during gap catch-up (drained or seeked dead rows are
    /// charged) stops both evaluators at the same output and frontier.
    #[tokio::test]
    async fn small_budget_iter_matches_stream_during_gap_catch_up() {
        assert_iter_matches_stream_under_budgets(
            skewed_and_source(),
            skewed_and_query(),
            0..(51 * BUCKET_SIZE),
            1..=60,
        )
        .await;
    }

    /// Budget exhaustion in a DNF over sparse gaps stops both evaluators at
    /// the same output and frontier.
    #[tokio::test]
    async fn small_budget_iter_matches_stream_for_or_terms_over_sparse_gaps() {
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

        assert_iter_matches_stream_under_budgets(source, query, 0..(10 * BUCKET_SIZE), 1..=20)
            .await;
    }

    /// Tightest budgets: `(a AND b) OR c` with three leaves. Every leaf's first
    /// bucket is reserved, so even a budget at or below the leaf count lets
    /// `c` report bucket 40 and `b` pin the frontier at bucket 50 before
    /// catch-up of `a` exhausts the budget.
    #[test]
    fn budget_at_or_below_leaf_count_still_reports_every_first_position() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (
                    test_key(b"a"),
                    (0..=4).map(|bucket| (bucket, vec![1])).collect(),
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

        for budget in 1..=3 {
            let out = observe(
                eval_bitmap_query_bucket_iter(
                    source.clone(),
                    query.clone(),
                    0..(60 * BUCKET_SIZE),
                    BUCKET_SIZE,
                    ScanDirection::Ascending,
                    budget,
                    SkipPolicy::DRAIN_ONLY,
                    EVERY_WATERMARK,
                    |_| {},
                )
                .collect(),
            );

            assert_eq!(
                out,
                vec![
                    Ok(Watermarked::Watermark(40 * BUCKET_SIZE)),
                    Ok(Watermarked::Item((40, vec![7]))),
                    Ok(Watermarked::Watermark(41 * BUCKET_SIZE)),
                    Err(50 * BUCKET_SIZE),
                ],
                "budget {budget}"
            );
        }
    }

    /// Charged, discarded and seek counts match the stream evaluator for a
    /// scan that both drains and seeks across a dead gap.
    #[tokio::test]
    async fn metrics_match_stream_when_crossing_gap() {
        let policy = SkipPolicy {
            drain_probe_rows: std::num::NonZeroU32::new(2),
        };
        for direction in [ScanDirection::Ascending, ScanDirection::Descending] {
            let (stream_tx, stream_rx) = std::sync::mpsc::channel();
            let _: Vec<_> = eval_bitmap_query_stream(
                skewed_and_source(),
                skewed_and_query(),
                0..(51 * BUCKET_SIZE),
                BUCKET_SIZE,
                direction,
                1_000,
                policy,
                EVERY_WATERMARK,
                move |observed| stream_tx.send(observed).unwrap(),
            )
            .collect()
            .await;
            let (iter_tx, iter_rx) = std::sync::mpsc::channel();
            let _: Vec<_> = eval_bitmap_query_bucket_iter(
                skewed_and_source(),
                skewed_and_query(),
                0..(51 * BUCKET_SIZE),
                BUCKET_SIZE,
                direction,
                1_000,
                policy,
                EVERY_WATERMARK,
                move |observed| iter_tx.send(observed).unwrap(),
            )
            .collect();

            let iter_metrics = iter_rx.recv().expect("iter metrics reported");
            assert_eq!(
                iter_metrics,
                stream_rx.recv().expect("stream metrics reported"),
                "{direction:?}"
            );
            assert_eq!(
                iter_metrics,
                BitmapScanMetrics {
                    buckets_evaluated: 7,
                    buckets_discarded: 3,
                    leaf_seeks: 1,
                },
                "{direction:?}"
            );
        }
    }

    /// `on_metrics` fires exactly once, at drop: not while the iterator is
    /// alive, and also when it is dropped before or in the middle of a scan.
    #[test]
    fn metrics_fire_once_when_dropped_mid_scan() {
        fn scan(
            on_metrics: impl FnOnce(BitmapScanMetrics) + 'static,
        ) -> impl Iterator<Item = WatermarkedBucket> {
            eval_bitmap_query_bucket_iter(
                skewed_and_source(),
                skewed_and_query(),
                0..(51 * BUCKET_SIZE),
                BUCKET_SIZE,
                ScanDirection::Ascending,
                1_000,
                SkipPolicy {
                    drain_probe_rows: std::num::NonZeroU32::new(2),
                },
                EVERY_WATERMARK,
                on_metrics,
            )
        }

        let (unpolled_tx, unpolled_rx) = std::sync::mpsc::channel();
        let unpolled = scan(move |observed| unpolled_tx.send(observed).unwrap());
        assert!(unpolled_rx.try_recv().is_err(), "fired before drop");
        drop(unpolled);
        assert_eq!(
            unpolled_rx.try_recv().expect("fired on drop"),
            BitmapScanMetrics {
                buckets_evaluated: 0,
                buckets_discarded: 0,
                leaf_seeks: 0,
            }
        );
        assert!(
            matches!(
                unpolled_rx.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Disconnected)
            ),
            "fired more than once"
        );

        let (partial_tx, partial_rx) = std::sync::mpsc::channel();
        let mut partial = scan(move |observed| partial_tx.send(observed).unwrap());
        // The first round charges both leaves' first buckets and yields the
        // bucket-0 match; the gap after it is never reached.
        assert!(matches!(
            partial.next(),
            Some(Ok(Watermarked::Item((0, _))))
        ));
        assert!(partial_rx.try_recv().is_err(), "fired before drop");
        drop(partial);
        assert_eq!(
            partial_rx.try_recv().expect("fired on drop"),
            BitmapScanMetrics {
                buckets_evaluated: 2,
                buckets_discarded: 0,
                leaf_seeks: 0,
            }
        );
        assert!(
            matches!(
                partial_rx.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Disconnected)
            ),
            "fired more than once"
        );
    }

    /// Absent-dimension semantics: an include whose key has no rows at all annihilates its
    /// conjunction (`∩ ∅ = ∅`). Pinned explicitly because this shape only arises when a queried key
    /// was never written (e.g. a sender with no transactions), which live-cluster tests never
    /// exercise.
    #[test]
    fn absent_include_annihilates_term() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([(test_key(b"a"), vec![(0, vec![1, 2])])])),
        };
        let query =
            BitmapQuery::new(vec![BitmapTerm::new(vec![include(b"ghost")]).unwrap()]).unwrap();

        let out = eval_bitmap_query_bucket_iter(
            source,
            query,
            0..200_000,
            BUCKET_SIZE,
            ScanDirection::Ascending,
            1_000_000,
            SkipPolicy::DRAIN_ONLY,
            EVERY_WATERMARK,
            |_| {},
        )
        .collect::<Vec<_>>();
        let items = items_only(&collect_marked(out));

        assert!(
            items.is_empty(),
            "absent include must annihilate: {items:?}"
        );
    }

    /// A present include cannot rescue a conjunction whose other include is absent — the
    /// intersection is still empty.
    #[test]
    fn absent_include_annihilates_term_despite_present_include() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([(test_key(b"a"), vec![(0, vec![1, 2])])])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), include(b"ghost")]).unwrap(),
        ])
        .unwrap();

        let out = eval_bitmap_query_bucket_iter(
            source,
            query,
            0..200_000,
            BUCKET_SIZE,
            ScanDirection::Ascending,
            1_000_000,
            SkipPolicy::DRAIN_ONLY,
            EVERY_WATERMARK,
            |_| {},
        )
        .collect::<Vec<_>>();
        let items = items_only(&collect_marked(out));

        assert!(
            items.is_empty(),
            "absent include must annihilate: {items:?}"
        );
    }

    /// An exclude whose key has no rows subtracts nothing (`∖ ∅`): the present include's matches
    /// pass through untouched.
    #[test]
    fn absent_exclude_is_noop() {
        let source = TestBucketSource {
            buckets: Arc::new(BTreeMap::from([(test_key(b"a"), vec![(0, vec![1, 2])])])),
        };
        let query = BitmapQuery::new(vec![
            BitmapTerm::new(vec![include(b"a"), exclude(b"ghost")]).unwrap(),
        ])
        .unwrap();

        let out = eval_bitmap_query_bucket_iter(
            source,
            query,
            0..200_000,
            BUCKET_SIZE,
            ScanDirection::Ascending,
            1_000_000,
            SkipPolicy::DRAIN_ONLY,
            EVERY_WATERMARK,
            |_| {},
        )
        .collect::<Vec<_>>();

        assert_eq!(items_only(&collect_marked(out)), vec![(0, vec![1, 2])]);
    }

    /// Run both public evaluators over `source` with the same budget and
    /// pacing, in both directions, and require identical paced output: the
    /// iterator's buckets are flattened exactly as the stream evaluator
    /// flattens its own. Returns how many runs ended on a scan-limit stop.
    async fn assert_paced_iter_matches_paced_stream<S>(source: S) -> usize
    where
        S: BitmapBucketSource + for<'a> BitmapBucketIteratorSource<'a>,
    {
        let range = 0..(10 * BUCKET_SIZE);
        let mut scan_limited = 0;
        for direction in [ScanDirection::Ascending, ScanDirection::Descending] {
            for budget in [7, 1_000_000] {
                for pacing in [
                    charged_buckets(1, false),
                    charged_buckets(3, true),
                    charged_buckets(5, false),
                    charged_buckets(5, true),
                ] {
                    let iter_buckets: Vec<_> = eval_bitmap_query_bucket_iter(
                        source.clone(),
                        parallel_leaves_query(),
                        range.clone(),
                        BUCKET_SIZE,
                        direction,
                        budget,
                        SkipPolicy::DRAIN_ONLY,
                        pacing,
                        |_| {},
                    )
                    .collect();
                    let iter_members: Vec<_> = flatten_watermarked_buckets(
                        futures::stream::iter(iter_buckets),
                        range.clone(),
                        BUCKET_SIZE,
                        direction,
                    )
                    .collect()
                    .await;
                    let stream_members: Vec<_> = eval_bitmap_query_stream(
                        source.clone(),
                        parallel_leaves_query(),
                        range.clone(),
                        BUCKET_SIZE,
                        direction,
                        budget,
                        SkipPolicy::DRAIN_ONLY,
                        pacing,
                        |_| {},
                    )
                    .collect()
                    .await;

                    let iter_members = observe_member_outputs(iter_members);
                    assert_eq!(
                        iter_members,
                        observe_member_outputs(stream_members),
                        "paced iter and stream diverged with budget {budget}, {direction:?}, \
                         {pacing:?}"
                    );
                    if matches!(iter_members.last(), Some(Err(ObservedStop::ScanLimit(_)))) {
                        scan_limited += 1;
                    }
                }
            }
        }
        scan_limited
    }

    /// Paced output of the iterator evaluator matches the stream evaluator's
    /// for item, watermark, scan-limit, fault and clean-end sequences.
    #[tokio::test]
    async fn paced_iter_matches_paced_stream() {
        let scan_limited =
            assert_paced_iter_matches_paced_stream(parallel_leaves_source(10, &[3, 6])).await;
        assert!(scan_limited > 0, "no run exhausted the budget");
        assert_paced_iter_matches_paced_stream(StoppingBucketSource {
            inner: parallel_leaves_source(10, &[3]),
            stop_key: test_key(b"a"),
            rows_before_stop: 5,
            stop: || LeafStop::Fault(anyhow::anyhow!("leaf storage fault")),
        })
        .await;
    }

    /// The two public evaluators.
    #[derive(Clone, Copy, Debug)]
    enum Evaluator {
        Iter,
        Stream,
    }

    /// The scan work [`ScanWorkCountingSource`] had counted when an output
    /// arrived.
    #[derive(Clone, Copy, Debug)]
    struct ScanCounts {
        rows: u64,
        reopens: u64,
    }

    impl ScanCounts {
        /// Scan work as [`WatermarkPacing`] counts it under `policy`, for
        /// scans whose catch-up steps reopen at most one leaf each, so every
        /// reopen is its own step.
        fn work(self, policy: SkipPolicy) -> u64 {
            self.rows + policy.reopen_cost_rows() * self.reopens
        }
    }

    /// One evaluation's member-level outputs, each paired with the scan work
    /// counted when it arrived, and the evaluation's metrics.
    struct ChargedRun {
        outputs: Vec<(ObservedMemberOutput, ScanCounts)>,
        metrics: BitmapScanMetrics,
    }

    impl ChargedRun {
        fn member_outputs(&self) -> Vec<ObservedMemberOutput> {
            self.outputs
                .iter()
                .map(|(output, _)| output.clone())
                .collect()
        }

        fn items(&self) -> Vec<u64> {
            self.outputs
                .iter()
                .filter_map(|(output, _)| match output {
                    Ok(Watermarked::Item(member)) => Some(*member),
                    _ => None,
                })
                .collect()
        }

        fn watermarks(&self) -> Vec<u64> {
            self.outputs
                .iter()
                .filter_map(|(output, _)| match output {
                    Ok(Watermarked::Watermark(position)) => Some(*position),
                    _ => None,
                })
                .collect()
        }
    }

    /// Run `evaluator` over `source` with a budget the scan never exhausts,
    /// pairing every member-level output with the scan work counted when it
    /// arrived. The iterator evaluator's buckets are flattened one output at
    /// a time, exactly as the stream evaluator flattens its own.
    async fn charged_member_run(
        evaluator: Evaluator,
        source: TestBucketSource,
        query: BitmapQuery,
        range: Range<u64>,
        direction: ScanDirection,
        policy: SkipPolicy,
        pacing: WatermarkPacing,
    ) -> ChargedRun {
        let source = ScanWorkCountingSource::new(source);
        let counts = |source: &ScanWorkCountingSource| ScanCounts {
            rows: source.rows_yielded(),
            reopens: source.reopens(),
        };
        let (metrics_tx, metrics_rx) = std::sync::mpsc::channel();
        let on_metrics = move |metrics: BitmapScanMetrics| metrics_tx.send(metrics).unwrap();
        let mut outputs = Vec::new();
        match evaluator {
            Evaluator::Iter => {
                let bucket_outputs = eval_bitmap_query_bucket_iter(
                    source.clone(),
                    query,
                    range.clone(),
                    BUCKET_SIZE,
                    direction,
                    1_000_000,
                    policy,
                    pacing,
                    on_metrics,
                );
                for bucket_output in bucket_outputs {
                    let counted = counts(&source);
                    let members: Vec<_> = flatten_watermarked_buckets(
                        futures::stream::iter([bucket_output]),
                        range.clone(),
                        BUCKET_SIZE,
                        direction,
                    )
                    .collect()
                    .await;
                    outputs.extend(
                        observe_member_outputs(members)
                            .into_iter()
                            .map(|output| (output, counted)),
                    );
                }
            }
            Evaluator::Stream => {
                let mut member_outputs = eval_bitmap_query_stream(
                    source.clone(),
                    query,
                    range,
                    BUCKET_SIZE,
                    direction,
                    1_000_000,
                    policy,
                    pacing,
                    on_metrics,
                );
                while let Some(output) = member_outputs.next().await {
                    let counted = counts(&source);
                    outputs.extend(
                        observe_member_outputs(vec![output])
                            .into_iter()
                            .map(|output| (output, counted)),
                    );
                }
            }
        }
        let metrics = metrics_rx.recv().unwrap();
        assert_eq!(
            metrics.buckets_evaluated,
            source.rows_yielded(),
            "{evaluator:?}: every yielded row must be charged"
        );
        assert_eq!(
            metrics.leaf_seeks,
            source.reopens(),
            "{evaluator:?}: every reopen must be counted"
        );
        ChargedRun { outputs, metrics }
    }

    fn watermark_at_bucket(bucket: u64) -> ObservedMemberOutput {
        Ok(Watermarked::Watermark(bucket * BUCKET_SIZE))
    }

    /// An item of an edge bucket whose matches all lie outside the scan range
    /// trims to nothing, so it is not an emission: the scan's first
    /// member-level output is the first unpaced watermark, emitted at once,
    /// and later watermarks are paced from it. Ascending, bucket 0's only
    /// match is member 1 and the range starts at 2; descending, bucket 9's
    /// only match is `9 * BUCKET_SIZE + 1` and the range ends just before it.
    /// Each bucket charges two rows, so with five rows between emissions every
    /// third watermark is emitted. Counting the trimmed item as an emission
    /// holds the first watermark and starts the output three buckets late.
    #[tokio::test]
    async fn edge_item_without_in_range_members_does_not_delay_watermarks() {
        let cases = [
            (
                ScanDirection::Ascending,
                parallel_leaves_source(10, &[0]),
                2..(10 * BUCKET_SIZE),
                [1, 4, 7, 10],
            ),
            (
                ScanDirection::Descending,
                parallel_leaves_source(10, &[9]),
                0..(9 * BUCKET_SIZE + 1),
                [9, 6, 3, 0],
            ),
        ];
        for evaluator in [Evaluator::Iter, Evaluator::Stream] {
            for (direction, source, range, emitted_buckets) in cases.clone() {
                let context = format!("{evaluator:?}, {direction:?}");
                let expected: Vec<ObservedMemberOutput> = emitted_buckets
                    .into_iter()
                    .map(watermark_at_bucket)
                    .collect();
                let unpaced = charged_member_run(
                    evaluator,
                    source.clone(),
                    parallel_leaves_query(),
                    range.clone(),
                    direction,
                    SkipPolicy::DRAIN_ONLY,
                    EVERY_WATERMARK,
                )
                .await;
                assert_eq!(
                    unpaced.member_outputs().first(),
                    expected.first(),
                    "{context}"
                );
                let paced = charged_member_run(
                    evaluator,
                    source,
                    parallel_leaves_query(),
                    range,
                    direction,
                    SkipPolicy::DRAIN_ONLY,
                    charged_buckets(5, false),
                )
                .await;
                assert_eq!(paced.member_outputs(), expected, "{context}");
            }
        }
    }

    /// An edge-bucket item with a member inside the scan range is an emission
    /// even though it also holds members outside the range: it is the scan's
    /// first output and restarts the count, so the watermark of its own
    /// bucket is held and the next one waits for five more charged rows.
    /// Ascending, bucket 0 matches members 1 and 3 and the range starts at 2;
    /// descending, bucket 9 matches `9 * BUCKET_SIZE + 1` and `+ 3` and the
    /// range ends at `+ 2`.
    #[tokio::test]
    async fn edge_item_with_in_range_member_restarts_count() {
        let cases = [
            (
                ScanDirection::Ascending,
                parallel_leaves_source_with_matches(10, &[(0, &[1, 3][..])]),
                2..(10 * BUCKET_SIZE),
                3,
                [4, 7, 10],
            ),
            (
                ScanDirection::Descending,
                parallel_leaves_source_with_matches(10, &[(9, &[1, 3][..])]),
                0..(9 * BUCKET_SIZE + 2),
                9 * BUCKET_SIZE + 1,
                [6, 3, 0],
            ),
        ];
        for evaluator in [Evaluator::Iter, Evaluator::Stream] {
            for (direction, source, range, in_range_member, emitted_buckets) in cases.clone() {
                let mut expected: Vec<ObservedMemberOutput> =
                    vec![Ok(Watermarked::Item(in_range_member))];
                expected.extend(emitted_buckets.into_iter().map(watermark_at_bucket));
                let paced = charged_member_run(
                    evaluator,
                    source,
                    parallel_leaves_query(),
                    range,
                    direction,
                    SkipPolicy::DRAIN_ONLY,
                    charged_buckets(5, false),
                )
                .await;
                assert_eq!(
                    paced.member_outputs(),
                    expected,
                    "{evaluator:?}, {direction:?}"
                );
            }
        }
    }

    /// `a AND b` where `a` holds member 1 in every tenth bucket of `0..=40`
    /// and `b` holds member 2 in every bucket, plus member 1 in bucket 20:
    /// each `a` row leaves `b` nine dead rows behind, and the query matches
    /// only `20 * BUCKET_SIZE + 1`.
    fn sparse_and_dense_source() -> TestBucketSource {
        TestBucketSource {
            buckets: Arc::new(BTreeMap::from([
                (
                    test_key(b"a"),
                    (0..=40)
                        .step_by(10)
                        .map(|bucket| (bucket, vec![1]))
                        .collect(),
                ),
                (
                    test_key(b"b"),
                    (0..=40)
                        .map(|bucket| {
                            if bucket == 20 {
                                (bucket, vec![1, 2])
                            } else {
                                (bucket, vec![2])
                            }
                        })
                        .collect(),
                ),
            ])),
        }
    }

    /// With gap catch-up draining and seeking past dead rows, pacing counts
    /// all the scan work: each emitted watermark after the scan's first
    /// output comes at least `min_buckets` of work (rows charged plus reopen
    /// steps) after the previous emission and is a watermark of the unpaced
    /// run, in order, while items are exactly the unpaced run's.
    #[tokio::test]
    async fn charged_buckets_paces_by_scan_work_during_gap_catch_up() {
        const MIN_BUCKETS: u64 = 3;
        let range = 0..(41 * BUCKET_SIZE);
        for evaluator in [Evaluator::Iter, Evaluator::Stream] {
            for direction in [ScanDirection::Ascending, ScanDirection::Descending] {
                for policy in [
                    SkipPolicy::DRAIN_ONLY,
                    SkipPolicy {
                        drain_probe_rows: std::num::NonZeroU32::new(2),
                    },
                ] {
                    let context = format!("{evaluator:?}, {direction:?}, {policy:?}");
                    let unpaced = charged_member_run(
                        evaluator,
                        sparse_and_dense_source(),
                        skewed_and_query(),
                        range.clone(),
                        direction,
                        policy,
                        EVERY_WATERMARK,
                    )
                    .await;
                    let paced = charged_member_run(
                        evaluator,
                        sparse_and_dense_source(),
                        skewed_and_query(),
                        range.clone(),
                        direction,
                        policy,
                        charged_buckets(MIN_BUCKETS, false),
                    )
                    .await;

                    // The scan crosses dead gaps, seeking past them when the
                    // policy probes.
                    assert!(paced.metrics.buckets_discarded > 0, "{context}");
                    assert_eq!(
                        paced.metrics.leaf_seeks > 0,
                        policy.drain_probe_rows.is_some(),
                        "{context}"
                    );

                    assert_eq!(unpaced.items(), vec![20 * BUCKET_SIZE + 1], "{context}");
                    assert_eq!(paced.items(), unpaced.items(), "{context}");
                    assert_eq!(
                        paced.outputs.first().map(|(output, _)| output),
                        unpaced.outputs.first().map(|(output, _)| output),
                        "first output must not be held: {context}"
                    );

                    let paced_watermarks = paced.watermarks();
                    let unpaced_watermarks = unpaced.watermarks();
                    let mut unmatched_unpaced = unpaced_watermarks.iter();
                    for position in &paced_watermarks {
                        assert!(
                            unmatched_unpaced.any(|candidate| candidate == position),
                            "paced {paced_watermarks:?} is not a subsequence of unpaced \
                             {unpaced_watermarks:?}: {context}"
                        );
                    }
                    // Some watermarks are held and some emitted after the
                    // first, so the spacing check below is not vacuous.
                    assert!(
                        paced_watermarks.len() < unpaced_watermarks.len(),
                        "no watermark held: {context}"
                    );
                    assert!(
                        paced_watermarks.len() >= 2,
                        "no watermark after the first emitted: {context}"
                    );

                    for pair in paced.outputs.windows(2) {
                        let [(_, previous), (output, current)] = pair else {
                            unreachable!("windows(2) yields pairs");
                        };
                        if matches!(output, Ok(Watermarked::Watermark(_))) {
                            let work = current.work(policy) - previous.work(policy);
                            assert!(
                                work >= MIN_BUCKETS,
                                "{output:?} emitted {work} of scan work after the previous \
                                 emission: {context}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Both evaluators emit a watermark exactly when `min_buckets` of scan
    /// work, rows charged plus `drain_probe_rows` per reopen step, has been
    /// done since the last emission. Over [`sparse_and_dense_source`] with a
    /// two-row probe, `b` drains two dead rows and reopens behind every `a`
    /// row, one leaf per step, so some watermarks are due only because of
    /// the reopens.
    #[tokio::test]
    async fn both_evaluators_pace_by_rows_and_reopen_steps() {
        let range = 0..(41 * BUCKET_SIZE);
        let policy = SkipPolicy {
            drain_probe_rows: std::num::NonZeroU32::new(2),
        };
        for evaluator in [Evaluator::Iter, Evaluator::Stream] {
            for direction in [ScanDirection::Ascending, ScanDirection::Descending] {
                let unpaced = charged_member_run(
                    evaluator,
                    sparse_and_dense_source(),
                    skewed_and_query(),
                    range.clone(),
                    direction,
                    policy,
                    EVERY_WATERMARK,
                )
                .await;
                assert!(
                    unpaced.metrics.leaf_seeks > 0,
                    "{evaluator:?}, {direction:?}"
                );
                let mut due_only_to_reopens = 0;
                for min_buckets in 1..=20 {
                    let context =
                        format!("{evaluator:?}, {direction:?}, min_buckets {min_buckets}");
                    // The pacing rule applied to the unpaced run: items and
                    // the first output are always emitted, and every
                    // emission restarts the count.
                    let mut expected = Vec::new();
                    let mut last_emission: Option<ScanCounts> = None;
                    for (output, counts) in &unpaced.outputs {
                        if matches!(output, Ok(Watermarked::Watermark(_)))
                            && let Some(last) = last_emission
                        {
                            if counts.work(policy) - last.work(policy) < min_buckets {
                                continue;
                            }
                            if counts.rows - last.rows < min_buckets {
                                due_only_to_reopens += 1;
                            }
                        }
                        last_emission = Some(*counts);
                        expected.push(output.clone());
                    }
                    let paced = charged_member_run(
                        evaluator,
                        sparse_and_dense_source(),
                        skewed_and_query(),
                        range.clone(),
                        direction,
                        policy,
                        charged_buckets(min_buckets, false),
                    )
                    .await;
                    assert_eq!(paced.member_outputs(), expected, "{context}");
                }
                assert!(
                    due_only_to_reopens > 0,
                    "no watermark was due only because of reopens: {evaluator:?}, {direction:?}"
                );
            }
        }
    }
}
