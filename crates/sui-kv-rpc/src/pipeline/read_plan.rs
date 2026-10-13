// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The one rule that cuts a burst of ready work into reads.
//!
//! A pipeline stage collects a burst (everything ready before upstream returns `Pending`) and
//! cuts it into consecutive reads with [`plan_reads`], once, against the drainer slots free at
//! that moment. The burst is a list of indivisible [`Segment`]s in burst order; each read is a
//! run of consecutive segments, so reads consumed in order keep the output order.
//!
//! A read costs about one BigTable seek per scattered key, or per stretch of adjacent rows (see
//! `SeekUnit`). Three limits shape the cut, each a stage setting:
//!
//! - `max-seeks-per-read` ([`ReadLimits::max_seeks_per_read`]) caps the seeks of a read.
//! - `max-keys-per-read` ([`ReadLimits::max_keys_per_read`]) caps the keys of a read, on the
//!   stages where one seek covers many keys. Elsewhere every key is its own seek, and there is no
//!   separate key cap.
//! - `min-seeks-per-split-read` ([`ReadLimits::min_seeks_per_split_read`]), when set, lets a
//!   burst be split over the free slots, up to one read per free slot, as long as every read
//!   still costs at least that many seeks. Ready work split over idle slots is read in parallel,
//!   so a small burst finishes sooner than it would as one large read; the floor keeps reads from
//!   shrinking to sizes where per-read overhead dominates.
//!
//! A burst goes out as at least the fewest reads that respect both caps. Reads of one plan aim
//! at near-equal shares of the burst's seeks, with the larger reads last. When the key cap ends
//! reads before their share, a burst can take more reads than it aimed for, some of them smaller
//! than `min-seeks-per-split-read`.

use std::ops::Range;

/// An indivisible piece of a burst, in burst order. A segment that adds no seek is a rider: a
/// keyed item whose keys earlier segments read, which rides on the read open at its position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Segment {
    /// Whether the piece adds one seek to a read: an item, or a run or run piece, for
    /// `pipelined_chunks`; a key still to read for keyed batches. False for a rider.
    pub(super) adds_seek: bool,
    /// Keys the piece reads, counted against [`ReadLimits::max_keys_per_read`].
    pub(super) keys: usize,
}

/// A stage's read limits. Every limit is at least 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ReadLimits {
    /// Most seeks one read costs: the stage's `max-seeks-per-read`.
    pub(super) max_seeks_per_read: usize,
    /// Most keys one read carries: the stage's `max-keys-per-read`, on the stages whose seeks
    /// are buckets. `None` where every key is its own seek, so `max_seeks_per_read` already caps
    /// keys.
    pub(super) max_keys_per_read: Option<usize>,
    /// Least seeks a read costs when a burst is split over free slots: the stage's
    /// `min-seeks-per-split-read`. `None`: never split.
    pub(super) min_seeks_per_split_read: Option<usize>,
}

/// Cut `segments` (one burst) into consecutive reads. Returns each read's range of segment
/// indices, in order, covering every segment exactly once; empty iff `segments` is empty.
///
/// The rule:
///
/// 1. `burst_seeks` is the seeks the whole burst costs: the number of segments that add a
///    seek. `fewest_reads` is the number of reads a greedy left-to-right packing needs when
///    every read stays within `max_seeks_per_read` and `max_keys_per_read`.
/// 2. `max_split_reads` is `min(free_slots, burst_seeks / min_seeks_per_split_read)` when that
///    floor is set, and 0 otherwise: the most reads the burst can be split into with every
///    read costing at least that many seeks.
/// 3. `target_reads` is `max(fewest_reads, max_split_reads, 1)`.
/// 4. Segments are cut in order. Read `r` (0-based) aims for `seek_share = remaining_seeks /
///    (target_reads - r)` seeks, rounded down so the larger reads come last, or all remaining
///    seeks once `r >= target_reads`. A read takes the next segment unless that would carry
///    it past `max_seeks_per_read` or `max_keys_per_read`, or the segment adds a seek and the
///    read already holds its `seek_share`; a rider joins the open read. Cutting continues
///    until every segment is taken; the key cap can end reads early, and so produce more
///    reads than `target_reads`.
///
/// Panics unless every limit is at least 1, every segment's keys fit a read on their own, and
/// riders come only without a key cap. Stage config validation and the two segment builders
/// (`SeekUnit::segments`, `plan_keyed_reads`) guarantee all three, and under them every read
/// takes at least one segment.
pub(super) fn plan_reads(
    segments: &[Segment],
    limits: ReadLimits,
    free_slots: usize,
) -> Vec<Range<usize>> {
    assert!(
        limits.max_seeks_per_read > 0
            && limits.max_keys_per_read != Some(0)
            && limits.min_seeks_per_split_read != Some(0),
        "plan_reads: every read limit must be at least 1: {limits:?}"
    );
    if segments.is_empty() {
        return Vec::new();
    }

    let (burst_seeks, fewest_reads) = burst_seeks_and_fewest_reads(segments, limits);
    let max_split_reads = match limits.min_seeks_per_split_read {
        Some(min_seeks) => free_slots.min(burst_seeks / min_seeks),
        None => 0,
    };
    let target_reads = fewest_reads.max(max_split_reads).max(1);

    let mut reads = Vec::with_capacity(target_reads);
    let mut remaining_seeks = burst_seeks;
    let mut start = 0;
    while start < segments.len() {
        let read_index = reads.len();
        let seek_share = if read_index < target_reads {
            remaining_seeks / (target_reads - read_index)
        } else {
            remaining_seeks
        };

        let mut read_seeks = 0;
        let mut read_keys = 0;
        let mut end = start;
        while let Some(segment) = segments.get(end) {
            if !fits(read_seeks, read_keys, segment, limits)
                || (segment.adds_seek && read_seeks >= seek_share)
            {
                break;
            }
            read_seeks += usize::from(segment.adds_seek);
            read_keys += segment.keys;
            end += 1;
        }
        // The preconditions guarantee a seek share of at least 1 whenever a seek opens a
        // read, so every read takes a segment; one that took none would be an empty read.
        assert!(end > start, "plan_reads: read {read_index} took no segment");

        remaining_seeks -= read_seeks;
        reads.push(start..end);
        start = end;
    }
    reads
}

/// The seeks the whole burst costs, and the number of reads a greedy left-to-right packing
/// within `limits` needs. Checks each segment against [`plan_reads`]'s preconditions.
fn burst_seeks_and_fewest_reads(segments: &[Segment], limits: ReadLimits) -> (usize, usize) {
    let mut burst_seeks = 0;
    let mut fewest_reads = 0;
    let mut read_seeks = 0;
    let mut read_keys = 0;
    for (index, segment) in segments.iter().enumerate() {
        if let Some(max_keys) = limits.max_keys_per_read {
            assert!(
                segment.keys <= max_keys,
                "plan_reads: segment {index} reads {} keys, more than a read's {max_keys}",
                segment.keys,
            );
        }
        assert!(
            segment.adds_seek || limits.max_keys_per_read.is_none(),
            "plan_reads: segment {index} is a rider under a key cap"
        );
        burst_seeks += usize::from(segment.adds_seek);
        if index == 0 || !fits(read_seeks, read_keys, segment, limits) {
            fewest_reads += 1;
            read_seeks = 0;
            read_keys = 0;
        }
        read_seeks += usize::from(segment.adds_seek);
        read_keys += segment.keys;
    }
    (burst_seeks, fewest_reads)
}

/// Whether a read already costing `read_seeks` and carrying `read_keys` stays within `limits`
/// after taking `segment`.
fn fits(read_seeks: usize, read_keys: usize, segment: &Segment, limits: ReadLimits) -> bool {
    read_seeks + usize::from(segment.adds_seek) <= limits.max_seeks_per_read
        && limits
            .max_keys_per_read
            .is_none_or(|max_keys| read_keys + segment.keys <= max_keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A segment that adds a seek and reads one key.
    const SEEK: Segment = Segment {
        adds_seek: true,
        keys: 1,
    };

    /// A rider: a keyed item whose keys earlier segments read.
    const RIDER: Segment = Segment {
        adds_seek: false,
        keys: 0,
    };

    fn seeks(count: usize) -> Vec<Segment> {
        vec![SEEK; count]
    }

    /// Limits without a key cap.
    fn limits(max_seeks_per_read: usize, min_seeks_per_split_read: Option<usize>) -> ReadLimits {
        ReadLimits {
            max_seeks_per_read,
            max_keys_per_read: None,
            min_seeks_per_split_read,
        }
    }

    fn read_lengths(reads: &[Range<usize>]) -> Vec<usize> {
        reads.iter().map(|read| read.len()).collect()
    }

    /// Deterministic linear congruential generator for the property loops.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }

        /// A value in `low..=high`.
        fn between(&mut self, low: usize, high: usize) -> usize {
            low + (self.next() % (high - low + 1) as u64) as usize
        }
    }

    /// A random burst shaped like one of the three segment builders produces, with limits that
    /// pass stage config validation: an item stage (one seek and key per segment, no key cap),
    /// a bucket stage (one seek per run of up to `max-keys-per-read` keys), or a keyed stage
    /// (one seek per key still to read, riders for items that claim none; no key cap).
    fn production_burst(rng: &mut Lcg) -> (Vec<Segment>, ReadLimits, usize) {
        let max_seeks_per_read = rng.between(1, 12);
        let min_seeks_per_split_read = if rng.between(0, 1) == 0 {
            None
        } else {
            Some(rng.between(1, 5))
        };
        let free_slots = rng.between(0, 12);
        let count = rng.between(0, 30);
        let (segments, max_keys_per_read) = match rng.between(0, 2) {
            0 => (seeks(count), None),
            1 => {
                let max_keys = rng.between(1, 8);
                let runs = (0..count)
                    .map(|_| Segment {
                        adds_seek: true,
                        keys: rng.between(1, max_keys),
                    })
                    .collect();
                (runs, Some(max_keys))
            }
            _ => {
                let mut segments = Vec::new();
                for _ in 0..count {
                    match rng.between(0, 3) {
                        0 => segments.push(RIDER),
                        claimed => segments.extend(std::iter::repeat_n(SEEK, claimed)),
                    }
                }
                (segments, None)
            }
        };
        let limits = ReadLimits {
            max_seeks_per_read,
            max_keys_per_read,
            min_seeks_per_split_read,
        };
        (segments, limits, free_slots)
    }

    /// Greedy left-to-right packing count, written independently of the planner.
    fn reference_fewest(segments: &[Segment], limits: ReadLimits) -> usize {
        let max_keys = limits.max_keys_per_read.unwrap_or(usize::MAX);
        let mut reads: Vec<(usize, usize)> = Vec::new();
        for segment in segments {
            let segment_seeks = usize::from(segment.adds_seek);
            match reads.last_mut() {
                Some((seeks, keys))
                    if *seeks + segment_seeks <= limits.max_seeks_per_read
                        && *keys + segment.keys <= max_keys =>
                {
                    *seeks += segment_seeks;
                    *keys += segment.keys;
                }
                _ => reads.push((segment_seeks, segment.keys)),
            }
        }
        reads.len()
    }

    #[test]
    fn empty_burst_plans_no_reads() {
        assert!(plan_reads(&[], limits(10, Some(1)), 8).is_empty());
    }

    #[test]
    fn spread_over_free_slots_puts_larger_reads_last() {
        let reads = plan_reads(&seeks(9), limits(100, Some(2)), 4);
        assert_eq!(reads, vec![0..2, 2..4, 4..6, 6..9]);
    }

    #[test]
    fn spread_beyond_fewest_reads() {
        let reads = plan_reads(&seeks(13), limits(10, Some(1)), 8);
        assert_eq!(
            reads,
            vec![0..1, 1..2, 2..3, 3..5, 5..7, 7..9, 9..11, 11..13]
        );
    }

    #[test]
    fn without_split_floor_burst_goes_out_as_fewest_reads() {
        assert_eq!(plan_reads(&seeks(20), limits(25, None), 4), vec![0..20]);
        assert_eq!(
            plan_reads(&seeks(60), limits(25, None), 4),
            vec![0..20, 20..40, 40..60]
        );
    }

    #[test]
    fn split_floor_bounds_split_reads() {
        assert_eq!(
            plan_reads(&seeks(20), limits(100, Some(4)), 20),
            vec![0..4, 4..8, 8..12, 12..16, 16..20]
        );
        assert_eq!(
            plan_reads(&seeks(9), limits(100, Some(4)), 20),
            vec![0..4, 4..9]
        );
        assert_eq!(plan_reads(&seeks(3), limits(100, Some(4)), 20), vec![0..3]);
    }

    #[test]
    fn no_free_slots_plans_fewest_reads() {
        assert_eq!(
            plan_reads(&seeks(30), limits(10, Some(1)), 0),
            vec![0..10, 10..20, 20..30]
        );
    }

    #[test]
    fn riders_join_the_open_read() {
        // 6 seeks, min split 3, 2 free slots: 2 target reads, seek share 3. The rider after
        // the third seek joins the first read, which is open at its position.
        let segments = [RIDER, SEEK, SEEK, SEEK, RIDER, SEEK, SEEK, SEEK];
        let reads = plan_reads(&segments, limits(100, Some(3)), 2);
        assert_eq!(reads, vec![0..5, 5..8]);
    }

    #[test]
    fn burst_of_riders_forms_one_read() {
        assert_eq!(plan_reads(&[RIDER; 5], limits(100, Some(1)), 8), vec![0..5]);
    }

    #[test]
    fn key_cap_sets_fewest_and_reads_balance_by_seeks() {
        let segments: Vec<Segment> = [2, 1, 1, 1]
            .iter()
            .map(|&keys| Segment {
                adds_seek: true,
                keys,
            })
            .collect();
        let capped = ReadLimits {
            max_seeks_per_read: 100,
            max_keys_per_read: Some(4),
            min_seeks_per_split_read: None,
        };
        assert_eq!(plan_reads(&segments, capped, 8), vec![0..2, 2..4]);

        let split = ReadLimits {
            min_seeks_per_split_read: Some(1),
            ..capped
        };
        assert_eq!(
            plan_reads(&segments, split, 8),
            vec![0..1, 1..2, 2..3, 3..4]
        );
    }

    #[test]
    fn reads_cover_burst_within_bounds_and_at_least_fewest() {
        let mut rng = Lcg(0x5eed);
        for _ in 0..20_000 {
            let (segments, limits, free_slots) = production_burst(&mut rng);

            let reads = plan_reads(&segments, limits, free_slots);
            let case = format!("{segments:?} {limits:?} free {free_slots}: {reads:?}");

            assert_eq!(reads.is_empty(), segments.is_empty(), "{case}");
            let mut next = 0;
            for read in &reads {
                assert_eq!(read.start, next, "{case}");
                assert!(read.end > read.start, "{case}");
                next = read.end;
                let read_segments = &segments[read.clone()];
                let seeks = read_segments.iter().filter(|s| s.adds_seek).count();
                let keys: usize = read_segments.iter().map(|s| s.keys).sum();
                assert!(seeks <= limits.max_seeks_per_read, "{case}");
                assert!(
                    limits.max_keys_per_read.is_none_or(|max| keys <= max),
                    "{case}"
                );
            }
            assert_eq!(next, segments.len(), "{case}");
            assert!(reads.len() >= reference_fewest(&segments, limits), "{case}");
        }
    }

    #[test]
    fn seeks_split_into_balanced_reads_of_at_least_the_floor() {
        let mut rng = Lcg(0xb0257);
        let mut split_cases = 0;
        for _ in 0..20_000 {
            let burst_seeks = rng.between(1, 60);
            let max_seeks = rng.between(1, 20);
            let min_seeks = rng.between(1, 8);
            let free_slots = rng.between(0, 12);
            let fewest_reads = burst_seeks.div_ceil(max_seeks);
            let max_split_reads = free_slots.min(burst_seeks / min_seeks);
            if max_split_reads < fewest_reads {
                continue;
            }
            split_cases += 1;

            let reads = plan_reads(
                &seeks(burst_seeks),
                limits(max_seeks, Some(min_seeks)),
                free_slots,
            );
            let lengths = read_lengths(&reads);
            let case = format!(
                "seeks {burst_seeks} max {max_seeks} min {min_seeks} free {free_slots}: \
                 {lengths:?}"
            );

            assert_eq!(lengths.len(), max_split_reads, "{case}");
            assert!(lengths.iter().all(|&length| length >= min_seeks), "{case}");
            assert!(lengths.windows(2).all(|pair| pair[0] <= pair[1]), "{case}");
            let smallest = lengths.first().copied().unwrap_or_default();
            let largest = lengths.last().copied().unwrap_or_default();
            assert!(largest - smallest <= 1, "{case}");
        }
        assert!(split_cases > 1_000, "only {split_cases} split cases");
    }
}
