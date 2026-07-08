// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Grouping of pipelines into ingestion cohorts based on how far behind the network tip each
//! pipeline's resume checkpoint is. Pipelines in the same cohort share one ingestion service, so
//! a far-behind pipeline only applies backpressure to similarly-lagging peers, not to pipelines
//! that are already at the tip.
//!
//! Cohorts also merge back together at runtime: when a trailing cohort's ingestion frontier gets
//! within [`IngestionConfig::cohort_merge_threshold`] checkpoints of the cohort ahead of it, the
//! trailing cohort hands its subscribers off to the ahead cohort (exactly once -- no gaps, no
//! duplicate deliveries) and winds down. The protocol lives entirely in the cohort table, under
//! its one mutex: alongside its frontier, each cohort advertises the exclusive end of the range
//! its broadcaster currently has in flight ([`CohortSlot::in_flight`]) -- nothing at or above
//! that point is broadcast before the next range is picked up. A merging cohort atomically
//! registers its subscribers in the target's [`CohortSlot::pending`] at exactly that point, the
//! handoff checkpoint: the target absorbs them into its subscriber list when it commits the
//! range that starts there, while the mergee keeps delivering everything below it before winding
//! down. The target never has to coordinate -- registration is a push into the table, so a
//! trailing cohort can merge into a cohort whose broadcaster is parked on backpressure.
//!
//! [`IngestionConfig::cohort_merge_threshold`]: crate::ingestion::IngestionConfig::cohort_merge_threshold

use std::ops::Range;
use std::sync::Arc;
use std::sync::Mutex;

use tokio::sync::mpsc;
use tracing::debug;

use crate::PendingPipeline;
use crate::ingestion::ingestion_client::CheckpointEnvelope;

/// Default for [`IngestionConfig::min_cohort_boundary`]: a cohort's boundary (its maximum member
/// distance from the tip) is at least this many checkpoints, so that pipelines that are roughly
/// caught up are not fragmented into many tiny cohorts, each with its own ingestion service.
///
/// [`IngestionConfig::min_cohort_boundary`]: crate::ingestion::IngestionConfig::min_cohort_boundary
pub(crate) const DEFAULT_MIN_COHORT_BOUNDARY: u64 = 25_000;

/// Default for [`IngestionConfig::cohort_merge_threshold`]: a trailing cohort merges into the
/// cohort ahead of it once its ingestion frontier is within this many checkpoints of that
/// cohort's frontier.
///
/// [`IngestionConfig::cohort_merge_threshold`]: crate::ingestion::IngestionConfig::cohort_merge_threshold
pub(crate) const DEFAULT_COHORT_MERGE_THRESHOLD: u64 = 1_000;

/// Upper bound on the size of the ingestion chunks a merge-enabled broadcaster splits its legs
/// into. Chunk boundaries are where a broadcaster looks for merge opportunities and absorbs
/// adopted subscribers, so the chunk size tracks the merge threshold (a trigger should fire
/// before frontiers drift by more than a threshold), but is capped so a large threshold does not
/// starve triggers.
const MAX_MERGE_CHUNK: u64 = 1_000;

/// Where a cohort is in its lifecycle, from its peers' point of view.
#[derive(Default)]
pub(crate) enum CohortState {
    /// Running; may merge away or be merged into.
    #[default]
    Active,

    /// Committed to merging into another cohort; delivering its final leg.
    Merging,

    /// Its broadcaster has finished.
    Gone,
}

/// One cohort's entry in the table its peers consult to find merge targets and to hand their
/// subscribers over.
#[derive(Default)]
pub(crate) struct CohortSlot {
    pub(crate) state: CohortState,

    /// The cohort's ingestion frontier: everything below this checkpoint has been delivered to
    /// its subscribers. `None` until the cohort's broadcaster starts. May lag the true frontier
    /// by up to one ingestion chunk; merge decisions only need it as a hint, because the handoff
    /// checkpoint comes from `in_flight`, which is exact.
    pub(crate) frontier: Option<u64>,

    /// The exclusive end of the range this cohort's broadcaster is currently committed to:
    /// nothing at or above this checkpoint is broadcast before the broadcaster commits its next
    /// range (absorbing `pending` first), so it is the exact point at which new subscribers can
    /// still join in full. `None` while no range is committed (before the broadcaster starts,
    /// and through each leg's setup window), during which adoptions are refused.
    pub(crate) in_flight: Option<u64>,

    /// Subscribers handed over by merging cohorts, each with the first checkpoint it is owed;
    /// this cohort's broadcaster absorbs them into its subscriber list at the commit of the
    /// range their checkpoint starts.
    pub(crate) pending: Vec<(Subscriber, u64)>,
}

/// A broadcaster's view of the cohort table, used to publish its frontier and find (or become) a
/// merge target. Present only when the indexer runs multiple cohorts with merging enabled.
#[derive(Clone)]
pub(crate) struct MergeContext {
    pub(crate) table: Arc<Mutex<Vec<CohortSlot>>>,

    /// This cohort's index into the table.
    pub(crate) cohort: usize,

    /// Merge when the cohort ahead is within this many checkpoints.
    pub(crate) threshold: u64,
}

/// Marks a cohort [`CohortState::Gone`] when dropped. Held by the cohort's broadcaster so that
/// every exit path -- completion, error, panic, and abort alike -- retires the slot: peers stop
/// targeting it, and dropping its unabsorbed `pending` closes those subscribers' channels, the
/// same wind-down they would see from a running service shutting down.
pub(crate) struct CohortGuard(MergeContext);

/// A subscription to the ingestion service: the channel checkpoints are delivered on, and the
/// first checkpoint the subscriber still needs. The broadcaster does not deliver checkpoints
/// below `next_checkpoint` -- the subscriber has already processed them.
#[derive(Clone)]
pub(crate) struct Subscriber {
    pub(crate) tx: mpsc::Sender<Arc<CheckpointEnvelope>>,

    /// The subscriber's resume point.
    pub(crate) next_checkpoint: u64,
}

impl Subscriber {
    /// Whether this subscriber still needs `sequence_number`.
    pub(crate) fn needs(&self, sequence_number: u64) -> bool {
        self.next_checkpoint <= sequence_number
    }
}

impl MergeContext {
    /// The number of checkpoints a broadcaster should ingest between merge trigger points.
    pub(crate) fn chunk(&self) -> u64 {
        self.threshold.clamp(1, MAX_MERGE_CHUNK)
    }

    /// Called by this cohort's broadcaster at a clean state -- everything below `frontier`
    /// delivered to `subscribers`, nothing in flight. Absorbs subscribers owed from `frontier`,
    /// publishes it, and withdraws the cohort's join point (nothing is in flight, so there is no
    /// exact point to join at until the next range is committed).
    ///
    /// Then, if a cohort ahead is within the merge threshold, commits this cohort to merging
    /// into it: every subscriber (and every not-yet-absorbed adoptee) is registered with the
    /// target at the handoff checkpoint -- the exclusive end of the target's in-flight range --
    /// and the target's index and the handoff are returned. This cohort should truncate its own
    /// range to end at the handoff, so between the two cohorts every subscriber sees every
    /// checkpoint exactly once.
    pub(crate) fn at_clean_state(
        &self,
        frontier: u64,
        subscribers: &mut Vec<Subscriber>,
    ) -> Option<(usize, u64)> {
        let mut table = self.table.lock().unwrap();

        let slot = &mut table[self.cohort];
        absorb(&mut slot.pending, frontier, subscribers);
        slot.frontier = Some(frontier);
        slot.in_flight = None;

        if !matches!(slot.state, CohortState::Active) {
            return None;
        }

        let (target, handoff) = self.target_of(&table, frontier)?;
        debug_assert!(frontier <= handoff, "handoff below the mergee's frontier");

        // Register this cohort's subscribers with the target from the handoff. Adoptees still
        // pending here (owed from beyond `frontier`) are registered as well, but also retained:
        // this cohort's final leg still owes them everything below the handoff.
        let handed: Vec<_> = subscribers
            .iter()
            .map(|s| (s.clone(), handoff))
            .chain(
                table[self.cohort]
                    .pending
                    .iter()
                    .map(|(s, from)| (s.clone(), (*from).max(handoff))),
            )
            .collect();
        table[target].pending.extend(handed);
        table[self.cohort].state = CohortState::Merging;

        Some((target, handoff))
    }

    /// Called by this cohort's broadcaster when it commits to ingesting `range`: absorbs
    /// subscribers owed from its start into `subscribers` (before the range's snapshot is
    /// taken), and advertises its end as the point where new adoptions can join.
    pub(crate) fn commit_range(&self, range: Range<u64>, subscribers: &mut Vec<Subscriber>) {
        let mut table = self.table.lock().unwrap();
        let slot = &mut table[self.cohort];
        absorb(&mut slot.pending, range.start, subscribers);
        debug_assert!(
            slot.pending.iter().all(|(_, from)| *from >= range.end),
            "pending adoption strictly inside a committed range"
        );
        slot.frontier = Some(range.start);
        slot.in_flight = Some(range.end);
    }

    /// Called by this cohort's broadcaster before streaming checkpoint `lo` to `subscribers`
    /// (streaming is in order, so `lo` is the exact frontier): absorbs subscribers owed from
    /// `lo`, publishes the frontier, and advertises `lo + 1` as the join point.
    ///
    /// Returns whether a cohort ahead has come within merge range, in which case nothing is
    /// advertised and the caller should break out *without* sending `lo`, so the merge can be
    /// arranged at a clean state with frontier `lo`.
    pub(crate) fn commit_streamed(&self, lo: u64, subscribers: &mut Vec<Subscriber>) -> bool {
        let mut table = self.table.lock().unwrap();
        let slot = &mut table[self.cohort];
        absorb(&mut slot.pending, lo, subscribers);
        slot.frontier = Some(lo);
        let active = matches!(slot.state, CohortState::Active);

        if active && self.target_of(&table, lo).is_some() {
            table[self.cohort].in_flight = None;
            true
        } else {
            table[self.cohort].in_flight = Some(lo.saturating_add(1));
            false
        }
    }

    /// The guard this cohort's broadcaster holds so that its slot is retired however it exits.
    pub(crate) fn guard(&self) -> CohortGuard {
        CohortGuard(self.clone())
    }

    /// The nearest active cohort whose frontier is at or ahead of `frontier` by at most the
    /// merge threshold and whose join point is advertised, along with that join point (the
    /// handoff checkpoint for a merge committed now).
    fn target_of(&self, table: &[CohortSlot], frontier: u64) -> Option<(usize, u64)> {
        let mut best: Option<(u64, usize, u64)> = None;
        for (index, slot) in table.iter().enumerate() {
            if index == self.cohort || !matches!(slot.state, CohortState::Active) {
                continue;
            }

            let (Some(ahead), Some(in_flight)) = (slot.frontier, slot.in_flight) else {
                continue;
            };

            if ahead < frontier || ahead - frontier > self.threshold {
                continue;
            }

            if best.is_none_or(|(f, _, _)| ahead < f) {
                best = Some((ahead, index, in_flight));
            }
        }
        best.map(|(_, index, in_flight)| (index, in_flight))
    }
}

impl Drop for CohortGuard {
    fn drop(&mut self) {
        // Retire the slot even if the mutex was poisoned by a panicking peer.
        let mut table = self
            .0
            .table
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let slot = &mut table[self.0.cohort];
        slot.state = CohortState::Gone;
        slot.in_flight = None;
        slot.pending.clear();
    }
}

/// Move subscribers owed from at or below `frontier` out of `pending` and into `subscribers`.
/// Registrations always land exactly on the next commit point, so an absorbed subscriber is owed
/// precisely the range being committed.
fn absorb(pending: &mut Vec<(Subscriber, u64)>, frontier: u64, subscribers: &mut Vec<Subscriber>) {
    pending.retain(|(subscriber, from)| {
        if *from > frontier {
            return true;
        }

        debug_assert_eq!(*from, frontier, "absorbed a subscriber past its handoff");
        debug!(from = *from, "Absorbing adopted subscriber");
        subscribers.push(subscriber.clone());
        false
    });
}

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
    use super::*;

    /// A pipeline resuming exactly `distance` checkpoints behind the tip. Tests fix the tip at
    /// `u64::MAX` so scenarios can be written directly in distances and the overflow case is
    /// reachable.
    fn at(distance: u64) -> PendingPipeline {
        PendingPipeline {
            name: "test",
            next_checkpoint: u64::MAX - distance,
            build: Box::new(|_| unreachable!("cohort grouping never builds pipelines")),
        }
    }

    fn slot(state: CohortState, frontier: Option<u64>, in_flight: Option<u64>) -> CohortSlot {
        CohortSlot {
            state,
            frontier,
            in_flight,
            pending: vec![],
        }
    }

    fn context(slots: Vec<CohortSlot>, cohort: usize, threshold: u64) -> MergeContext {
        MergeContext {
            table: Arc::new(Mutex::new(slots)),
            cohort,
            threshold,
        }
    }

    /// A subscriber resuming at `next_checkpoint`, with its receiver.
    fn subscriber(next_checkpoint: u64) -> (Subscriber, mpsc::Receiver<Arc<CheckpointEnvelope>>) {
        let (tx, rx) = mpsc::channel(16);
        (
            Subscriber {
                tx,
                next_checkpoint,
            },
            rx,
        )
    }

    /// Register `subscriber` in `cohort`'s slot at its advertised join point, as a merging peer
    /// would, and return that handoff checkpoint.
    fn register(ctx: &MergeContext, cohort: usize, subscriber: Subscriber) -> u64 {
        let mut table = ctx.table.lock().unwrap();
        let slot = &mut table[cohort];
        let handoff = slot.in_flight.expect("no join point advertised");
        slot.pending.push((subscriber, handoff));
        handoff
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

    /// Committing a range absorbs subscribers registered at its start into the subscriber list,
    /// and advertises the range's end as the next join point.
    #[test]
    fn test_commit_range_absorbs_and_advertises() {
        let ctx = context(vec![slot(CohortState::Active, Some(10), Some(10))], 0, 50);
        let (sub, _rx) = subscriber(99);
        assert_eq!(register(&ctx, 0, sub), 10);

        let mut subscribers = vec![];
        ctx.commit_range(10..20, &mut subscribers);

        // Absorption preserves the subscriber's own resume point; only the pair's absorb point
        // is protocol state.
        assert_eq!(subscribers.len(), 1);
        assert_eq!(subscribers[0].next_checkpoint, 99);
        let table = ctx.table.lock().unwrap();
        assert!(table[0].pending.is_empty());
        assert_eq!(table[0].in_flight, Some(20));
        assert_eq!(table[0].frontier, Some(10));
    }

    /// A subscriber registered mid-range (at the range's end) is not absorbed until the commit
    /// of the range that starts there.
    #[test]
    fn test_commit_range_leaves_future_adoptions_pending() {
        let ctx = context(vec![slot(CohortState::Active, Some(10), Some(20))], 0, 50);
        let (sub, _rx) = subscriber(0);
        assert_eq!(register(&ctx, 0, sub), 20);

        let mut subscribers = vec![];
        ctx.commit_range(20..30, &mut subscribers);
        assert_eq!(subscribers.len(), 1);
    }

    /// A clean state absorbs subscribers owed from the frontier, publishes it, and withdraws the
    /// join point.
    #[test]
    fn test_clean_state_absorbs_and_withdraws() {
        let ctx = context(vec![slot(CohortState::Active, Some(10), Some(15))], 0, 50);
        let (sub, _rx) = subscriber(999);
        assert_eq!(register(&ctx, 0, sub), 15);

        let mut subscribers = vec![];
        assert!(ctx.at_clean_state(15, &mut subscribers).is_none());

        // Absorbed at the join point, with a resume point beyond it left intact: delivery from
        // here on is still filtered by the subscriber's own `next_checkpoint`.
        assert_eq!(subscribers.len(), 1);
        assert_eq!(subscribers[0].next_checkpoint, 999);
        let table = ctx.table.lock().unwrap();
        assert!(table[0].pending.is_empty());
        assert_eq!(table[0].in_flight, None);
        assert_eq!(table[0].frontier, Some(15));
    }

    /// The nearest active cohort ahead within the threshold is chosen, the handoff is its
    /// advertised join point, and committing to the merge takes this cohort out of the running.
    #[test]
    fn test_merge_target_nearest_ahead() {
        let ctx = context(
            vec![
                slot(CohortState::Active, Some(100), None),
                slot(CohortState::Active, Some(150), Some(170)),
                slot(CohortState::Active, Some(120), Some(140)),
            ],
            0,
            1_000,
        );

        let (sub, _rx) = subscriber(999);
        let mut subscribers = vec![sub];
        assert_eq!(
            ctx.at_clean_state(100, &mut subscribers),
            Some((2, 140)),
            "nearest cohort ahead wins, handing off at its join point"
        );

        {
            let table = ctx.table.lock().unwrap();
            assert!(matches!(table[0].state, CohortState::Merging));
            assert_eq!(table[2].pending.len(), 1);
            // Registration stamps the handoff on the pair, but the subscriber's own resume
            // point (here, beyond the handoff) rides along unchanged.
            assert_eq!(table[2].pending[0].1, 140);
            assert_eq!(table[2].pending[0].0.next_checkpoint, 999);
        }

        // Committed: the mergee no longer looks for targets.
        assert!(ctx.at_clean_state(100, &mut subscribers).is_none());
    }

    /// Cohorts that are behind, out of threshold, not yet started, without an advertised join
    /// point, or no longer active are all skipped.
    #[test]
    fn test_merge_target_filters() {
        let ctx = context(
            vec![
                slot(CohortState::Active, Some(100), Some(120)),
                slot(CohortState::Active, Some(90), Some(120)),
                slot(CohortState::Active, Some(151), Some(170)),
                slot(CohortState::Active, None, None),
                slot(CohortState::Active, Some(110), None),
                slot(CohortState::Merging, Some(110), Some(130)),
                slot(CohortState::Gone, Some(110), Some(130)),
            ],
            0,
            50,
        );
        let mut subscribers = vec![];
        assert!(ctx.at_clean_state(100, &mut subscribers).is_none());
    }

    /// The merge threshold is inclusive.
    #[test]
    fn test_merge_threshold_inclusive() {
        let ctx = context(
            vec![
                slot(CohortState::Active, Some(0), Some(10)),
                slot(CohortState::Active, Some(50), Some(60)),
            ],
            0,
            50,
        );
        let mut subscribers = vec![];
        assert_eq!(ctx.at_clean_state(0, &mut subscribers), Some((1, 60)));
    }

    /// A zero threshold does not disable merging: cohorts merge once their frontiers meet
    /// exactly, but not a checkpoint sooner.
    #[test]
    fn test_merge_threshold_zero() {
        let ctx = context(
            vec![
                slot(CohortState::Active, Some(100), Some(120)),
                slot(CohortState::Active, Some(100), Some(120)),
            ],
            0,
            0,
        );
        let mut subscribers = vec![];
        assert!(ctx.at_clean_state(99, &mut subscribers).is_none());

        // Rolling the frontier back to re-check is artificial, but exercises the boundary.
        ctx.table.lock().unwrap()[0].frontier = None;
        assert_eq!(ctx.at_clean_state(100, &mut subscribers), Some((1, 120)));
    }

    /// A merging cohort passes its own not-yet-absorbed adoptees along to the target (at the
    /// handoff, or their own owed checkpoint if that is later), while retaining them: its final
    /// leg still owes them everything below the handoff.
    #[test]
    fn test_merge_passes_pending_adoptees_along() {
        let ctx = context(
            vec![
                slot(CohortState::Active, Some(100), Some(110)),
                slot(CohortState::Active, Some(105), Some(120)),
            ],
            0,
            50,
        );
        let (sub, _rx) = subscriber(115);
        assert_eq!(register(&ctx, 0, sub), 110);

        let (own, _own_rx) = subscriber(7);
        let mut subscribers = vec![own];
        assert_eq!(ctx.at_clean_state(100, &mut subscribers), Some((1, 120)));

        let table = ctx.table.lock().unwrap();
        // The adoptee (owed from 110 < 120) was not absorbed at frontier 100, so it is both
        // retained locally and registered with the target at the handoff. Both handed entries
        // keep their own resume points.
        assert_eq!(table[0].pending.len(), 1);
        assert_eq!(table[0].pending[0].1, 110);
        let handed: Vec<(u64, u64)> = table[1]
            .pending
            .iter()
            .map(|(s, from)| (*from, s.next_checkpoint))
            .collect();
        assert_eq!(handed, vec![(120, 7), (120, 115)]);
    }

    /// Committing a streamed checkpoint absorbs subscribers owed from it (before it is sent) and
    /// advertises the next checkpoint as the join point.
    #[test]
    fn test_commit_streamed_absorbs_and_advertises() {
        let ctx = context(vec![slot(CohortState::Active, Some(6), Some(7))], 0, 50);
        let (sub, _rx) = subscriber(0);
        assert_eq!(register(&ctx, 0, sub), 7);

        let mut subscribers = vec![];
        assert!(!ctx.commit_streamed(7, &mut subscribers));

        assert_eq!(subscribers.len(), 1);
        let table = ctx.table.lock().unwrap();
        assert!(table[0].pending.is_empty());
        assert_eq!(table[0].in_flight, Some(8));
        assert_eq!(table[0].frontier, Some(7));
    }

    /// When a cohort ahead comes within merge range of the streamed frontier, the commit is
    /// refused (nothing advertised) so the caller breaks out to arrange the merge.
    #[test]
    fn test_commit_streamed_breaks_for_merge() {
        let ctx = context(
            vec![
                slot(CohortState::Active, Some(90), Some(91)),
                slot(CohortState::Active, Some(105), Some(120)),
            ],
            0,
            10,
        );

        let mut subscribers = vec![];
        assert!(!ctx.commit_streamed(94, &mut subscribers));
        assert!(ctx.commit_streamed(95, &mut subscribers));

        let table = ctx.table.lock().unwrap();
        assert_eq!(table[0].in_flight, None);
        assert_eq!(table[0].frontier, Some(95));
    }

    /// Dropping a cohort's guard retires its slot: peers stop finding it, and pending adoptees'
    /// channels close, winding them down like subscribers of a stopping service.
    #[test]
    fn test_guard_retires_slot_and_drops_pending() {
        let ctx = context(vec![slot(CohortState::Active, Some(10), Some(20))], 0, 50);
        let (sub, mut rx) = subscriber(0);
        assert_eq!(register(&ctx, 0, sub), 20);

        drop(ctx.guard());

        {
            let table = ctx.table.lock().unwrap();
            assert!(matches!(table[0].state, CohortState::Gone));
            assert_eq!(table[0].in_flight, None);
            assert!(table[0].pending.is_empty());
        }

        // The adoptee's channel has no senders left.
        assert!(rx.blocking_recv().is_none());
    }

    /// The guard retires the slot even when a panicking peer has poisoned the table mutex, so
    /// pending adoptees' channels still close and peers stop targeting the cohort.
    #[test]
    fn test_guard_retires_slot_despite_poisoned_table() {
        let ctx = context(vec![slot(CohortState::Active, Some(10), Some(20))], 0, 50);
        let (sub, mut rx) = subscriber(0);
        assert_eq!(register(&ctx, 0, sub), 20);

        // Poison the table, as a peer panicking while holding the lock would.
        let table = ctx.table.clone();
        std::thread::spawn(move || {
            let _lock = table.lock().unwrap();
            panic!("peer panic");
        })
        .join()
        .unwrap_err();

        drop(ctx.guard());

        {
            // Poison persists after `into_inner`, so the assertions must also recover from it.
            let table = ctx
                .table
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            assert!(matches!(table[0].state, CohortState::Gone));
            assert_eq!(table[0].in_flight, None);
            assert!(table[0].pending.is_empty());
        }

        // The adoptee's channel has no senders left.
        assert!(rx.blocking_recv().is_none());
    }
}
