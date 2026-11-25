// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Grouping of pipelines into ingestion cohorts based on how far behind the network tip each
//! pipeline's resume checkpoint is. Pipelines in the same cohort share one ingestion service, so
//! a far-behind pipeline only applies backpressure to similarly-lagging peers, not to pipelines
//! that are already at the tip.

use crate::PendingPipeline;

/// Default for [`IngestionConfig::min_cohort_boundary`]: a cohort's boundary (its maximum member
/// distance from the tip) is at least this many checkpoints, so that pipelines that are roughly
/// caught up are not fragmented into many tiny cohorts, each with its own ingestion service.
///
/// [`IngestionConfig::min_cohort_boundary`]: crate::ingestion::IngestionConfig::min_cohort_boundary
pub(crate) const DEFAULT_MIN_COHORT_BOUNDARY: u64 = 25_000;

/// Group pipelines into cohorts by their distance from the network tip.
///
/// Each pipeline's distance is how far behind `latest_checkpoint` it will resume from. `pipelines`
/// may be in any order; they are sorted (nearest-tip first) internally. The closest unassigned
/// pipeline (distance `d`) seeds a cohort that absorbs every pipeline whose distance is at most
/// `max(2 * d, min_cohort_boundary)`; the next unassigned pipeline seeds the next cohort, and so
/// on.
///
/// Returns the cohorts ordered nearest-tip first, each holding its members in ascending distance
/// order (pipelines at equal distances keep registration order).
pub(crate) fn cohorts(
    mut pipelines: Vec<PendingPipeline>,
    latest_checkpoint: u64,
    min_cohort_boundary: u64,
) -> Vec<Vec<PendingPipeline>> {
    // Stable sort: pipelines at equal distances keep registration order.
    pipelines.sort_by_key(|p| latest_checkpoint.saturating_sub(p.next_checkpoint));

    // The closest unassigned pipeline seeds a cohort that absorbs every peer within
    // `max(2 * its distance, min_cohort_boundary)`; the first pipeline beyond that boundary seeds
    // the next cohort, and so on.
    let mut cohorts: Vec<Vec<PendingPipeline>> = vec![];
    let mut boundary = None;
    for pipeline in pipelines {
        let dist = latest_checkpoint.saturating_sub(pipeline.next_checkpoint);
        if boundary.is_none_or(|b| dist > b) {
            cohorts.push(vec![]);
            boundary = Some(dist.saturating_mul(2).max(min_cohort_boundary));
        }
        cohorts.last_mut().unwrap().push(pipeline);
    }
    cohorts
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use crate::service::Service;

    use super::*;

    /// A pipeline resuming exactly `distance` checkpoints behind the tip. Tests fix the tip at
    /// `u64::MAX` so scenarios can be written directly in distances and the overflow case is
    /// reachable.
    fn at(distance: u64) -> PendingPipeline {
        PendingPipeline {
            name: "test",
            next_checkpoint: u64::MAX - distance,
            tx: mpsc::channel(1).0,
            service: Service::new(),
        }
    }

    /// Group `pipelines` (tip at `u64::MAX`) and read each cohort back out as member distances.
    fn grouped(pipelines: Vec<PendingPipeline>, min_cohort_boundary: u64) -> Vec<Vec<u64>> {
        cohorts(pipelines, u64::MAX, min_cohort_boundary)
            .iter()
            .map(|cohort| {
                cohort
                    .iter()
                    .map(|p| u64::MAX - p.next_checkpoint)
                    .collect()
            })
            .collect()
    }

    #[test]
    fn test_empty() {
        assert_eq!(
            grouped(vec![], DEFAULT_MIN_COHORT_BOUNDARY),
            Vec::<Vec<u64>>::new()
        );
    }

    #[test]
    fn test_singleton() {
        assert_eq!(
            grouped(vec![at(0)], DEFAULT_MIN_COHORT_BOUNDARY),
            vec![vec![0]]
        );
    }

    #[test]
    fn test_all_equal_distances() {
        assert_eq!(
            grouped(vec![at(42), at(42), at(42)], DEFAULT_MIN_COHORT_BOUNDARY),
            vec![vec![42, 42, 42]]
        );
    }

    /// Input may be given in any order; cohorts and their members come back nearest-tip first.
    #[test]
    fn test_sorts_unsorted_input() {
        assert_eq!(
            grouped(
                vec![at(200_000), at(10), at(70_000), at(30_000)],
                DEFAULT_MIN_COHORT_BOUNDARY
            ),
            vec![vec![10], vec![30_000], vec![70_000], vec![200_000]],
        );
    }

    /// The minimum boundary is inclusive: a seed at the tip absorbs pipelines exactly
    /// `min_cohort_boundary` away, but not one checkpoint further.
    #[test]
    fn test_min_boundary_inclusive() {
        assert_eq!(
            grouped(
                vec![
                    at(0),
                    at(DEFAULT_MIN_COHORT_BOUNDARY),
                    at(DEFAULT_MIN_COHORT_BOUNDARY + 1)
                ],
                DEFAULT_MIN_COHORT_BOUNDARY,
            ),
            vec![
                vec![0, DEFAULT_MIN_COHORT_BOUNDARY],
                vec![DEFAULT_MIN_COHORT_BOUNDARY + 1]
            ],
        );
    }

    /// Once the seed is further out than half the minimum boundary, twice its distance takes
    /// over as the cohort boundary.
    #[test]
    fn test_double_seed_distance_boundary() {
        assert_eq!(
            grouped(
                vec![at(20_000), at(40_000), at(40_001)],
                DEFAULT_MIN_COHORT_BOUNDARY
            ),
            vec![vec![20_000, 40_000], vec![40_001]]
        );
    }

    /// Each cohort's boundary is derived from its own seed, not the previous cohort's.
    #[test]
    fn test_chained_reseeding() {
        assert_eq!(
            grouped(
                vec![at(10), at(30_000), at(70_000), at(200_000)],
                DEFAULT_MIN_COHORT_BOUNDARY
            ),
            vec![vec![10], vec![30_000], vec![70_000], vec![200_000]],
        );
    }

    /// Doubling an enormous seed distance saturates instead of overflowing.
    #[test]
    fn test_distance_overflow_saturates() {
        assert_eq!(
            grouped(
                vec![at(25_000), at(u64::MAX - 1), at(u64::MAX)],
                DEFAULT_MIN_COHORT_BOUNDARY
            ),
            vec![vec![25_000], vec![u64::MAX - 1, u64::MAX]],
        );
    }

    /// A custom (non-default) boundary is honored: with a boundary of 100 a seed at the tip
    /// absorbs a peer 100 away but not one at 101 — whereas the default boundary would have put
    /// all three in one cohort.
    #[test]
    fn test_custom_boundary() {
        assert_eq!(
            grouped(vec![at(0), at(100), at(101)], 100),
            vec![vec![0, 100], vec![101]]
        );
        assert_eq!(
            grouped(vec![at(0), at(100), at(101)], DEFAULT_MIN_COHORT_BOUNDARY),
            vec![vec![0, 100, 101]]
        );
    }
}
