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
//! Budget accounting lives in the request layer for the iterator path (each
//! backend leaf iterator charges its own per-request budget and yields an error
//! on exhaustion), so this evaluator only propagates those errors.

use std::collections::VecDeque;
use std::iter::Peekable as IterPeekable;
use std::ops::Range;

use roaring::RoaringBitmap;

use super::BitmapBucketIteratorSource;
use super::BitmapQuery;
use super::DedupedQuery;
use super::LeafHead;
use super::LeafStop;
use super::ScanDirection;
use super::ScanStop;
use super::Watermarked;
use super::WatermarkedBucket;
use super::bound_in_direction;
use super::bucket_edges;
use super::build_term_specs;
use super::collapse;
use super::count_on_floor_refs;
use super::eval_term_at_bucket;
use super::frontier_advanced;
use super::recompute_unreferenced;
use super::take_snapshot_bitmap;

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
    // One peekable leaf per *unique* dimension key — terms reference them by
    // index. Identical keys across literals share a single backend scan; see
    // [`build_term_specs`]. Each leaf iterator borrows the backend's `'a`
    // store, not `source`, so the thin `source` handle is dropped once the
    // leaves are built.
    let DedupedQuery {
        keys: unique_keys,
        mut terms,
    } = build_term_specs(query.terms);
    let mut leaves: Vec<IterPeekable<S::Iter>> = Vec::with_capacity(unique_keys.len());
    for key in unique_keys {
        leaves.push(
            source
                .scan_bucket_iter(key, range.clone(), direction)
                .peekable(),
        );
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
    // `unreferenced[i]`: leaf is retired — either no satisfiable term still
    // points at it, or its bucket stream is at EOF (a spent exclude).
    let mut unreferenced = vec![false; leaf_count];
    // `front[i]`: clamped position each leaf has provably scanned to. Bounds the
    // resume cursor when a leaf errors before it can advance.
    let mut front = vec![request_floor; leaf_count];
    // The request floor is the baseline, not progress earned by scanning.
    let mut progress_frontier = Some(request_floor);
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
                if unreferenced[i] {
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

            // An include at EOF makes its term unsatisfiable (the intersection
            // is permanently empty). With dedup, an EOF'd leaf may be an
            // include for several terms; all of them become unsatisfiable.
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
            // Recompute leaf liveness from current term state. A leaf may be
            // shared across terms (include for one, exclude for another), so it
            // can only be retired when no satisfiable term still references it.
            recompute_unreferenced(&terms, &class, &mut unreferenced);

            // Consume any stop frame so the error surfaces. The collapse below
            // bundles this round's floor into a ScanLimit terminal; Fault and
            // Cancelled stops still follow the floor beacon.
            let mut errors: Vec<LeafStop> = Vec::new();
            for i in 0..leaf_count {
                if !unreferenced[i] && matches!(class[i], Some(LeafHead::Error)) {
                    match leaves[i].next() {
                        Some(Err(e)) => errors.push(e),
                        _ => unreachable!("peek classified Error"),
                    }
                }
            }

            let active: Vec<usize> = (0..leaf_count).filter(|&i| !unreferenced[i]).collect();
            if active.is_empty() {
                // Natural completion is represented by the caller's terminal
                // boundary; only progress earned while scanning is emitted here.
                done = true;
                return None;
            }

            // The floor is the slowest active leaf's scanned-to position; it is
            // the merged "every source has scanned past here" watermark.
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

            // Snapshot the bitmaps of leaves sitting on `floor_bucket` —
            // each unique leaf consumed exactly once — then distribute to
            // every referencing term. See [`build_term_specs`] for why
            // dedup matters: a leaf shared across terms can only be
            // pulled once before its iterator moves on.
            let mut snapshot: Vec<Option<RoaringBitmap>> = (0..leaf_count).map(|_| None).collect();
            let mut on_floor = vec![false; leaf_count];
            for i in 0..leaf_count {
                if !unreferenced[i]
                    && matches!(class[i], Some(LeafHead::Bucket(b)) if b == floor_bucket)
                {
                    on_floor[i] = true;
                    front[i] = post;
                    snapshot[i] = match leaves[i].next() {
                        Some(Ok((_, bitmap))) => Some(bitmap),
                        _ => None,
                    };
                }
            }
            let mut remaining_refs = count_on_floor_refs(&terms, &on_floor);

            let mut result: Option<RoaringBitmap> = None;
            for term in &terms {
                if term.unsatisfiable {
                    continue;
                }
                let includes: Vec<Option<RoaringBitmap>> = term
                    .includes
                    .iter()
                    .map(|&i| {
                        take_snapshot_bitmap(&mut snapshot, &mut remaining_refs, &on_floor, i)
                    })
                    .collect();
                let excludes: Vec<Option<RoaringBitmap>> = term
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
                pending.push_back(Ok(Watermarked::Item((floor_bucket, bitmap))));
            }
            if frontier_advanced(progress_frontier, post, direction) {
                pending.push_back(Ok(Watermarked::Watermark(post)));
                progress_frontier = Some(post);
            }
        }
    })
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
            let stream_out: Vec<_> = eval_bitmap_query_bucket_stream(
                source.clone(),
                query.clone(),
                0..(5 * BUCKET_SIZE),
                BUCKET_SIZE,
                direction,
                BitmapScanBudget::new(1_000_000),
            )
            .collect()
            .await;
            let iter_out: Vec<_> = eval_bitmap_query_bucket_iter(
                source.clone(),
                query.clone(),
                0..(5 * BUCKET_SIZE),
                BUCKET_SIZE,
                direction,
            )
            .collect();

            assert_eq!(
                collect_marked(stream_out),
                collect_marked(iter_out),
                "iter and stream diverged on unanchored terms for {direction:?}"
            );
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
}
