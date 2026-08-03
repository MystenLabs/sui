// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! [`Synchronizer`] — coordinates writes from multiple pipelines
//! into a single [`Db`], taking a cross-pipeline snapshot at every
//! checkpoint boundary.
//!
//! The framework's `SequentialStore::transaction` ships each
//! pipeline's `(Watermark, Batch)` pair through a per-pipeline
//! mpsc channel. The synchronizer's per-pipeline task receives
//! these in checkpoint order and commits the batch against the
//! shared database. At each checkpoint boundary (the snapshot
//! frontier), every pipeline that has caught up pauses via
//! a shared `SnapshotCoordinator`; the last one to arrive calls
//! [`Db::take_snapshot`](crate::Db::take_snapshot) and releases the
//! rest.
//!
//! This guarantees that a snapshot at checkpoint `C` captures
//! exactly the state every participating pipeline has up through
//! `C`'s writes — no such pipeline is half-applied when the snapshot
//! is taken.
//!
//! # Dynamic membership and late join
//!
//! The set of pipelines a snapshot waits on is dynamic. A pipeline
//! lagging behind the frontier — for example a
//! history cohort backfilling from a low watermark while a live
//! cohort follows the tip — commits freely without gating snapshots,
//! and joins the snapshot cohort once it climbs to the frontier. A
//! late join is still consistent: like every member, the joiner has
//! committed through `frontier - 1` at the moment it arrives. Until
//! it joins, snapshots reflect its data only up to wherever its
//! backfill has reached, which is why a snapshot is consistent only
//! for the pipelines that were members when it was taken.
//!
//! # Pipelines must commit exactly one checkpoint per batch
//!
//! The synchronizer assumes each `(Watermark, Batch)` it receives
//! corresponds to exactly one checkpoint. A pipeline driven by
//! the indexer-alt framework's `sequential::pipeline` must therefore
//! set `MAX_BATCH_CHECKPOINTS = 1` on its `sequential::Handler`
//! impl so the framework's collector commits each checkpoint as
//! its own batch rather than folding several into one. If a batch
//! spans multiple checkpoints the synchronizer will bail with an
//! out-of-order error on the first such batch.
//!
//! # Lifecycle
//!
//! 1. [`Synchronizer::new`] creates the service with a database and
//!    per-pipeline channel buffer size. The framework schema is read
//!    off `db` on demand.
//! 2. [`register_pipeline`](Synchronizer::register_pipeline) reads
//!    the pipeline's existing watermark (if any) from the
//!    framework schema and records it as that pipeline's resume
//!    point.
//! 3. [`Synchronizer::run`] consumes the synchronizer, spawns one
//!    task per registered pipeline, and returns the per-pipeline
//!    write queues plus a [`tokio::task::JoinSet`] whose tasks
//!    complete when their input channels close.
//!
//! Today the [`Store`](crate::Store) integration is wired via
//! [`Store::install_sync`](crate::Store::install_sync); see that
//! function's documentation for the end-to-end flow.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Context as _;
use anyhow::bail;
use anyhow::ensure;
use sui_futures::future::with_slow_future_monitor;
use tokio::sync::mpsc;
use tokio::sync::watch;

use crate::Batch;
use crate::Db;
use crate::PipelineTaskKey;
use crate::Watermark;
use tokio::task::JoinSet;
use tracing::debug;
use tracing::info;
use tracing::warn;

/// How long a per-pipeline synchronizer task may wait at the snapshot
/// frontier (for peer pipelines to reach it) before it logs a
/// warning. The wait still completes normally; the warning just
/// surfaces a likely operational issue.
const SLOW_SYNC_WARNING_THRESHOLD: Duration = Duration::from_secs(60);

/// Write-side handle to the per-pipeline channels each
/// [`Synchronizer`] task reads from. Held inside the
/// [`Store`](crate::Store)'s `OnceLock` so transactions can route
/// through it after the synchronizer is installed.
///
/// Pipeline identifiers are `&'static str` (typically a
/// pipeline's `Processor::NAME`) so the map key is a single
/// pointer rather than a heap-allocated `String`. The
/// `HashMap<&'static str, _>` still accepts `&str` lookups via
/// the standard `Borrow<str>` blanket impl.
pub(crate) type Queue = HashMap<&'static str, mpsc::Sender<(Watermark, Batch)>>;

/// Builder + runner for the per-pipeline synchronizer tasks.
///
/// Created via [`Synchronizer::new`]; pipelines registered via
/// [`register_pipeline`](Self::register_pipeline); started via
/// [`run`](Self::run) (which consumes `self`) and produces the
/// per-pipeline write queues plus a [`JoinSet`] driving the
/// per-pipeline tasks.
pub struct Synchronizer {
    db: Db,
    last_watermarks: HashMap<&'static str, Option<Watermark>>,
    first_checkpoint: u64,
    buffer_size: usize,
}

impl Synchronizer {
    /// Construct a synchronizer over `db`.
    ///
    /// The framework schema lives on `db` (auto-registered by
    /// [`Db::open`](crate::Db::open)); the
    /// synchronizer reads existing watermarks through it during
    /// [`register_pipeline`](Self::register_pipeline).
    ///
    /// A cross-pipeline snapshot is taken at every checkpoint
    /// boundary: before the write of checkpoint `C`, after every
    /// pipeline has applied `C - 1`.
    ///
    /// `buffer_size` is the capacity of each per-pipeline channel.
    /// Smaller values backpressure faster pipelines so they don't
    /// outpace slower ones.
    ///
    /// `first_checkpoint` is the starting checkpoint for brand-new
    /// pipelines that have no persisted watermark. `None` defaults
    /// to `0`.
    pub fn new(db: Db, buffer_size: usize, first_checkpoint: Option<u64>) -> Self {
        Self {
            db,
            last_watermarks: HashMap::new(),
            first_checkpoint: first_checkpoint.unwrap_or(0),
            buffer_size,
        }
    }

    /// Register a pipeline by its `pipeline_task` identifier.
    ///
    /// `pipeline_task` is `&'static str` because the canonical
    /// source of a pipeline's name is its
    /// [`Processor::NAME`](sui_indexer_alt_framework::pipeline::Processor::NAME)
    /// constant, which is already `&'static str`. Storing it that
    /// way avoids a per-pipeline heap allocation and lets the
    /// synchronizer's per-pipeline state, queue entry, and task
    /// share a single static string slice instead of cloned
    /// `String`s.
    ///
    /// Reads the pipeline's existing committer watermark (if any)
    /// from the framework schema so the synchronizer knows what
    /// checkpoint to expect next.
    ///
    /// Registering a brand-new pipeline (no persisted watermark)
    /// is *not* an error — the synchronizer expects its first
    /// write to be at `first_checkpoint`.
    pub fn register_pipeline(&mut self, pipeline_task: &'static str) -> anyhow::Result<()> {
        let key = PipelineTaskKey::new(pipeline_task);
        let watermark = self
            .db
            .framework()
            .watermarks
            .get(&key)
            .with_context(|| format!("reading initial watermark for {pipeline_task}"))?;
        self.last_watermarks.insert(pipeline_task, watermark);
        Ok(())
    }

    /// Start the synchronizer's per-pipeline tasks.
    ///
    /// Returns the per-pipeline write queues (one
    /// [`mpsc::Sender`] per registered pipeline) plus a
    /// [`JoinSet`] holding the spawned tasks.
    /// Dropping the queue closes every pipeline's channel, which
    /// causes the corresponding task to finish naturally — the
    /// [`JoinSet`] drains cleanly on shutdown.
    pub fn run(self) -> anyhow::Result<(JoinSet<anyhow::Result<()>>, Queue)> {
        ensure!(
            !self.last_watermarks.is_empty(),
            "no pipelines registered with the synchronizer",
        );

        let first_checkpoint = self.first_checkpoint;
        let buffer_size = self.buffer_size;

        // Figure out where the snapshot cadence should start: the
        // checkpoint after the highest already-committed checkpoint
        // across registered pipelines. Fresh pipelines (no watermark)
        // contribute `first_checkpoint`.
        let init_checkpoint = self
            .last_watermarks
            .values()
            .map(|w| w.map_or(first_checkpoint, |w| w.checkpoint_hi_inclusive))
            .max()
            .expect("non-empty by ensure! above");
        let next_snapshot_checkpoint = init_checkpoint + 1;

        // Classify each pipeline as a snapshot-cohort member or a
        // lagging pipeline. A pipeline caught up to the frontier (its
        // watermark is `init_checkpoint`, the checkpoint the next
        // snapshot captures) reaches `next_snapshot_checkpoint` as its
        // first snapshot boundary, so it joins the cohort immediately.
        // A pipeline lagging behind (e.g. a history cohort backfilling
        // from a low watermark) commits freely without gating snapshots
        // and joins the cohort once it climbs to the frontier.
        let is_member = |w: &Option<Watermark>| {
            w.map_or(first_checkpoint, |w| w.checkpoint_hi_inclusive) >= init_checkpoint
        };
        let initial_members = self
            .last_watermarks
            .values()
            .filter(|&w| is_member(w))
            .count();

        let coordinator = Arc::new(SnapshotCoordinator::new(
            self.db.clone(),
            next_snapshot_checkpoint,
            initial_members,
        ));

        let mut queue: Queue = HashMap::new();
        let mut join_set = JoinSet::new();
        for (pipeline_task, last_watermark) in self.last_watermarks {
            let (tx, rx) = mpsc::channel(buffer_size);
            queue.insert(pipeline_task, tx);
            let member = is_member(&last_watermark);
            join_set.spawn(synchronizer_task(
                coordinator.clone(),
                rx,
                coordinator.subscribe(),
                pipeline_task,
                first_checkpoint,
                last_watermark,
                member,
            ));
        }

        Ok((join_set, queue))
    }
}

/// The per-pipeline task body. Receives `(Watermark, Batch)` pairs
/// in checkpoint order, commits each batch, and coordinates with
/// peer tasks at the snapshot frontier via the shared
/// [`SnapshotCoordinator`].
///
/// `is_member` records whether this pipeline starts inside the
/// snapshot cohort (caught up to the frontier) or lagging behind it;
/// a lagging pipeline flips it to `true` when it climbs to the
/// frontier and joins.
async fn synchronizer_task(
    coordinator: Arc<SnapshotCoordinator>,
    mut rx: mpsc::Receiver<(Watermark, Batch)>,
    mut frontier_rx: watch::Receiver<u64>,
    pipeline_task: &'static str,
    first_checkpoint: u64,
    mut current_watermark: Option<Watermark>,
    mut is_member: bool,
) -> anyhow::Result<()> {
    loop {
        let next_checkpoint = current_watermark
            .as_ref()
            .map(|w| w.checkpoint_hi_inclusive + 1)
            .unwrap_or(first_checkpoint);
        let frontier = coordinator.frontier();

        match frontier.cmp(&next_checkpoint) {
            // Next checkpoint is below the snapshot frontier; commit it
            // without coordinating. This is the steady state for
            // lagging pipelines backfilling toward the frontier (and
            // fires once for a fresh pipeline's first checkpoint, before
            // it reaches the opening frontier).
            Ordering::Greater => {}

            // Next checkpoint is past the frontier. A member advances
            // one checkpoint at a time through each frontier, so this
            // only fires if a batch skipped the snapshot point
            // upstream.
            Ordering::Less => {
                bail!("Missed snapshot {frontier} for {pipeline_task}, got {next_checkpoint}");
            }

            // At the frontier: coordinate the cross-pipeline snapshot.
            // A lagging pipeline joins the cohort here.
            Ordering::Equal => {
                let Some(watermark) = current_watermark else {
                    bail!(
                        "{pipeline_task} has no watermark for snapshot at \
                         {next_checkpoint}"
                    );
                };
                coordinator
                    .arrive(
                        pipeline_task,
                        &mut is_member,
                        &mut frontier_rx,
                        next_checkpoint,
                        watermark,
                    )
                    .await;
            }
        }

        let Some((watermark, batch)) = rx.recv().await else {
            info!(pipeline = %pipeline_task, "Synchronizer channel closed");
            coordinator.depart(pipeline_task, is_member);
            break;
        };

        ensure!(
            watermark.checkpoint_hi_inclusive == next_checkpoint,
            "Out-of-order batch for {pipeline_task}: expected {next_checkpoint}, \
             got {watermark:?}. The synchronizer requires exactly one checkpoint \
             per batch; ensure the pipeline's `sequential::Handler` impl sets \
             `MAX_BATCH_CHECKPOINTS = 1`.",
        );

        batch
            .commit()
            .with_context(|| format!("committing batch for {pipeline_task} at {watermark:?}"))?;
        current_watermark = Some(watermark);
    }

    info!(
        pipeline = %pipeline_task,
        frontier = coordinator.frontier(),
        watermark = ?current_watermark,
        "Stopping sync",
    );
    Ok(())
}

/// Coordinates dynamic-membership cross-pipeline snapshots.
///
/// The snapshot *frontier* is the checkpoint at whose boundary the
/// next snapshot is taken (the snapshot captures state through
/// `frontier - 1`). A pipeline that has caught up to the frontier is a
/// *member* of the snapshot cohort; the snapshot fires once every
/// member has committed through `frontier - 1` and arrived at the
/// frontier. Pipelines lagging behind the frontier commit freely
/// without gating the snapshot and join the cohort when they climb to
/// it — a late join is consistent because the joiner, like every
/// member, has committed through `frontier - 1` when it arrives.
///
/// This replaces a fixed-size [`tokio::sync::Barrier`], which would
/// stall the caught-up cohort at the frontier waiting for a lagging
/// pipeline to climb all the way up — exactly the situation a history
/// cohort backfilling from a low watermark creates.
///
/// # Concurrency
///
/// Membership and arrival bookkeeping live behind a
/// [`std::sync::Mutex`], held only for the brief, non-async
/// transition (including the in-memory
/// [`Db::take_snapshot`](crate::Db::take_snapshot)). The frontier
/// itself lives in a [`watch`] channel: it is the single source of
/// truth for the current frontier *and* the wakeup mechanism for
/// members parked at the barrier. Because `watch` retains the latest
/// value, a member that parks *after* the frontier advances still
/// observes the advance — there is no lost-wakeup race.
struct SnapshotCoordinator {
    db: Db,
    /// Single source of truth for the current frontier; advancing it
    /// wakes every parked member.
    frontier: watch::Sender<u64>,
    cohort: Mutex<Cohort>,
}

/// Snapshot-cohort bookkeeping guarded by
/// [`SnapshotCoordinator::cohort`].
struct Cohort {
    /// Pipelines currently participating in the snapshot barrier.
    /// Grows when a lagging pipeline catches up and joins; shrinks
    /// when a member's input channel closes on shutdown.
    members: usize,
    /// Members that have reached the current frontier this round.
    /// Reset to zero each time the frontier advances.
    arrived: usize,
}

impl SnapshotCoordinator {
    fn new(db: Db, frontier: u64, members: usize) -> Self {
        let (frontier, _rx) = watch::channel(frontier);
        Self {
            db,
            frontier,
            cohort: Mutex::new(Cohort {
                members,
                arrived: 0,
            }),
        }
    }

    /// The current snapshot frontier. A best-effort, lock-free read
    /// used by the task loop to decide whether to coordinate;
    /// authoritative decisions re-check it under the cohort lock.
    fn frontier(&self) -> u64 {
        *self.frontier.borrow()
    }

    /// A fresh receiver for awaiting frontier advances.
    fn subscribe(&self) -> watch::Receiver<u64> {
        self.frontier.subscribe()
    }

    /// Advance the frontier to the next checkpoint and reset the
    /// round, releasing every parked member. Caller holds `cohort`.
    fn advance(&self, cohort: &mut Cohort, frontier: u64) {
        cohort.arrived = 0;
        // `send` wakes every parked member; the retained value also
        // lets a member that parks *after* this call observe the
        // advance, so there is no lost-wakeup race. `send` errors only
        // when there are no receivers, which is harmless here.
        let _ = self.frontier.send(frontier + 1);
    }

    /// Arrive at `at_checkpoint` (the caller's view of the current
    /// frontier) and block until the cross-pipeline snapshot at this
    /// frontier has been taken. Joins the cohort first if `*is_member`
    /// is false.
    ///
    /// Returns immediately without taking part if the frontier has
    /// already advanced past `at_checkpoint` — the caller raced a
    /// snapshot the rest of the cohort already took, and should commit
    /// `at_checkpoint` as an ordinary below-frontier checkpoint.
    async fn arrive(
        &self,
        pipeline_task: &str,
        is_member: &mut bool,
        frontier_rx: &mut watch::Receiver<u64>,
        at_checkpoint: u64,
        watermark: Watermark,
    ) {
        {
            let mut cohort = self.cohort.lock().expect("snapshot cohort mutex poisoned");

            // Re-check the frontier under the lock: a lagging pipeline
            // may have raced a snapshot taken by the rest of the cohort
            // while it was climbing. If so, don't join or wait.
            if *self.frontier.borrow() != at_checkpoint {
                return;
            }

            if !*is_member {
                cohort.members += 1;
                *is_member = true;
                debug!(
                    pipeline = %pipeline_task,
                    frontier = at_checkpoint,
                    members = cohort.members,
                    "Pipeline joined snapshot cohort",
                );
            }

            cohort.arrived += 1;
            if cohort.arrived == cohort.members {
                // Last to arrive: every member has committed through
                // `at_checkpoint - 1`, so take the snapshot and release
                // the cohort.
                self.db.take_snapshot(watermark);
                self.advance(&mut cohort, at_checkpoint);
                debug!(
                    pipeline = %pipeline_task,
                    checkpoint = watermark.checkpoint_hi_inclusive,
                    "Took snapshot",
                );
                return;
            }
        }

        // Not the last to arrive: park until the frontier advances past
        // the checkpoint we arrived at (i.e. the snapshot has been
        // taken).
        let _ = with_slow_future_monitor(
            frontier_rx.wait_for(|&f| f > at_checkpoint),
            SLOW_SYNC_WARNING_THRESHOLD,
            || warn!(pipeline = %pipeline_task, "Synchronizer stuck waiting for snapshot"),
        )
        .await;
    }

    /// Remove a member from the cohort when its input channel closes on
    /// shutdown. If the remaining members have all already arrived at
    /// the frontier, release them so they do not deadlock waiting for
    /// the departed pipeline.
    ///
    /// The departing pipeline has not reached the frontier (it was
    /// awaiting its next checkpoint, below the frontier), so no
    /// snapshot is taken here: a snapshot at this frontier would be
    /// inconsistent for the departed pipeline. Departure only happens
    /// on shutdown, so skipping this final snapshot is harmless.
    fn depart(&self, pipeline_task: &str, is_member: bool) {
        if !is_member {
            return;
        }
        let mut cohort = self.cohort.lock().expect("snapshot cohort mutex poisoned");
        cohort.members -= 1;
        debug!(
            pipeline = %pipeline_task,
            members = cohort.members,
            "Pipeline left snapshot cohort",
        );
        if cohort.members > 0 && cohort.arrived == cohort.members {
            let frontier = *self.frontier.borrow();
            self.advance(&mut cohort, frontier);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Db;
    use crate::DbOptions;
    use crate::FrameworkSchema;
    use crate::Schema;
    use crate::error::OpenError;
    use tempfile::TempDir;

    use super::*;

    /// Minimal user schema (no extra CFs); the framework's CFs are
    /// auto-registered by `Db::open`, so all the synchronizer
    /// tests need is a database with the default + framework CFs.
    #[derive(Debug)]
    struct EmptySchema;

    impl Schema for EmptySchema {
        fn cfs(_: &crate::options::CfOptionsResolver) -> Vec<crate::CfDescriptor> {
            vec![]
        }

        fn open(_: &Db) -> Result<Self, OpenError> {
            Ok(Self)
        }
    }

    fn open() -> (TempDir, Db) {
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<EmptySchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db)
    }

    /// Test-only helper: a watermark at `checkpoint`.
    fn wm(checkpoint: u64) -> Watermark {
        Watermark::for_checkpoint(checkpoint)
    }

    /// Poll until a snapshot at `checkpoint` exists, yielding to let
    /// the synchronizer tasks run. Panics after a bounded number of
    /// polls so a regression fails the test instead of hanging.
    async fn wait_for_snapshot(db: &Db, checkpoint: u64) {
        for _ in 0..10_000 {
            if db.at_snapshot(checkpoint).is_some() {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("snapshot at checkpoint {checkpoint} was never taken");
    }

    #[test]
    fn register_pipeline_with_no_watermark_succeeds() {
        let (_dir, db) = open();
        let mut sync = Synchronizer::new(db, 4, None);
        sync.register_pipeline("p").unwrap();
    }

    #[test]
    fn register_pipeline_reads_existing_watermark() {
        let (_dir, db) = open();
        // Persist an existing watermark via the auto-registered
        // framework schema.
        let framework = FrameworkSchema::new(db.clone());
        let mut wb = db.batch();
        let key = PipelineTaskKey::new("p".to_string());
        let w = Watermark {
            checkpoint_hi_inclusive: 42,
            ..Watermark::default()
        };
        wb.put(&framework.watermarks, &key, &w).unwrap();
        wb.commit().unwrap();

        let mut sync = Synchronizer::new(db, 4, None);
        sync.register_pipeline("p").unwrap();
        assert_eq!(
            sync.last_watermarks
                .get("p")
                .and_then(|w| *w)
                .map(|w| w.checkpoint_hi_inclusive),
            Some(42),
        );
    }

    #[test]
    fn run_refuses_no_pipelines() {
        let (_dir, db) = open();
        let sync = Synchronizer::new(db, 4, None);
        let err = sync.run().unwrap_err();
        assert!(format!("{err:#}").contains("no pipelines registered"));
    }

    #[tokio::test]
    async fn run_returns_one_queue_entry_per_pipeline() {
        let (_dir, db) = open();
        let mut sync = Synchronizer::new(db, 4, None);
        sync.register_pipeline("a").unwrap();
        sync.register_pipeline("b").unwrap();
        let (mut joinset, queue) = sync.run().unwrap();
        assert_eq!(queue.len(), 2);
        assert!(queue.contains_key("a"));
        assert!(queue.contains_key("b"));
        // Drop the queue so each task's channel closes and the
        // tasks exit naturally; the JoinSet drains cleanly.
        drop(queue);
        while let Some(joined) = joinset.join_next().await {
            joined.unwrap().unwrap();
        }
    }

    #[tokio::test]
    async fn snapshots_each_checkpoint_from_resume_watermark() {
        // A pipeline resuming at watermark 17 opens its frontier at
        // 18. Feeding 18 and 19 produces a snapshot at each committed
        // checkpoint: 17 (the resume point) when the task first
        // reaches the frontier, then 18 and 19 as they commit.
        let (_dir, db) = open();
        let framework = FrameworkSchema::new(db.clone());
        let mut wb = db.batch();
        wb.put(
            &framework.watermarks,
            &PipelineTaskKey::new("p".to_string()),
            &wm(17),
        )
        .unwrap();
        wb.commit().unwrap();

        let mut sync = Synchronizer::new(db.clone(), 4, None);
        sync.register_pipeline("p").unwrap();
        let (mut joinset, queue) = sync.run().unwrap();

        for cp in 18..=19 {
            queue
                .get("p")
                .unwrap()
                .send((wm(cp), db.batch()))
                .await
                .unwrap();
        }

        // Drop the queue to close the channel — the task exits after
        // processing the buffered batches (snapshotting 19 when it
        // reaches the next frontier).
        drop(queue);
        while let Some(joined) = joinset.join_next().await {
            joined.unwrap().unwrap();
        }

        for cp in 17..=19 {
            assert!(
                db.at_snapshot(cp).is_some(),
                "expected a snapshot at checkpoint {cp}",
            );
        }
    }

    #[tokio::test]
    async fn synchronizer_rejects_out_of_order_batch() {
        let (_dir, db) = open();
        let mut sync = Synchronizer::new(db.clone(), 4, None);
        sync.register_pipeline("p").unwrap();
        let (mut joinset, queue) = sync.run().unwrap();

        // First expected checkpoint is 0 (no prior watermark,
        // `first_checkpoint` defaulted to 0). Sending checkpoint
        // 5 first should be rejected.
        let bad = (
            Watermark {
                checkpoint_hi_inclusive: 5,
                ..Watermark::default()
            },
            db.batch(),
        );
        queue.get("p").unwrap().send(bad).await.unwrap();

        // The synchronizer task ends with an out-of-order error
        // (expected checkpoint 0, got 5).
        let result = joinset.join_next().await.unwrap().unwrap();
        let err = result.unwrap_err();
        assert!(format!("{err:#}").contains("Out-of-order"));
        drop(queue);
    }

    #[tokio::test]
    async fn synchronizer_takes_snapshot_at_each_checkpoint() {
        // Single pipeline → a snapshot after every checkpoint. Send
        // checkpoint 0 and observe a snapshot at 0.
        let (_dir, db) = open();
        let mut sync = Synchronizer::new(db.clone(), 4, None);
        sync.register_pipeline("p").unwrap();
        let (mut joinset, queue) = sync.run().unwrap();

        let batch = db.batch();
        let w = Watermark {
            checkpoint_hi_inclusive: 0,
            ..Watermark::default()
        };
        queue.get("p").unwrap().send((w, batch)).await.unwrap();
        drop(queue);
        while let Some(joined) = joinset.join_next().await {
            joined.unwrap().unwrap();
        }

        // The synchronizer commits checkpoint 0 and then takes a
        // snapshot at 0 when it reaches the next frontier (1).
        let range = db.snapshot_range();
        assert!(
            range.is_some(),
            "expected at least one snapshot to have been taken",
        );
    }

    #[tokio::test]
    async fn late_join_caught_up_cohort_snapshots_without_lagging_pipeline() {
        // Two pipelines: "live" restored to the tip (watermark 8) and
        // "history" lagging far behind (watermark 0). Every checkpoint
        // is a snapshot boundary, so the frontier opens at 9 and only
        // "live" is in the snapshot cohort; "live" takes snapshots at
        // the frontier without "history" ever climbing to it. (A
        // fixed-size barrier would deadlock here, waiting for "history"
        // to reach the frontier.)
        let (_dir, db) = open();
        let framework = FrameworkSchema::new(db.clone());
        let mut wb = db.batch();
        wb.put(
            &framework.watermarks,
            &PipelineTaskKey::new("live".to_string()),
            &wm(8),
        )
        .unwrap();
        wb.put(
            &framework.watermarks,
            &PipelineTaskKey::new("history".to_string()),
            &wm(0),
        )
        .unwrap();
        wb.commit().unwrap();

        let mut sync = Synchronizer::new(db.clone(), 8, None);
        sync.register_pipeline("live").unwrap();
        sync.register_pipeline("history").unwrap();
        let (mut joinset, queue) = sync.run().unwrap();

        // Feed only "live": 9, 10, 11. After committing 11 it reaches
        // the frontier (12) and, as the sole cohort member, snapshots
        // 11.
        for cp in 9..=11 {
            queue
                .get("live")
                .unwrap()
                .send((wm(cp), db.batch()))
                .await
                .unwrap();
        }

        // "history" is never fed, yet the snapshot at 11 must appear.
        wait_for_snapshot(&db, 11).await;

        // Close both channels; both tasks exit cleanly (no deadlock on
        // the lagging pipeline's departure).
        drop(queue);
        while let Some(joined) = joinset.join_next().await {
            joined.unwrap().unwrap();
        }
    }

    #[tokio::test]
    async fn late_join_lagging_pipeline_joins_cohort_on_catch_up() {
        // Drive the SnapshotCoordinator directly to exercise a late
        // join deterministically. Frontier starting at 12, one initial
        // member ("live").
        let (_dir, db) = open();
        let coordinator = Arc::new(SnapshotCoordinator::new(db.clone(), 12, 1));

        // "live" (a member) arrives at the frontier alone and snapshots
        // 11 without waiting; the frontier advances to 13.
        {
            let mut member = true;
            let mut rx = coordinator.subscribe();
            coordinator
                .arrive("live", &mut member, &mut rx, 12, wm(11))
                .await;
        }
        assert!(db.at_snapshot(11).is_some());
        assert_eq!(coordinator.frontier(), 13);

        // "history" (a laggard, not yet a member) climbs to the
        // frontier (13), joins the cohort, and blocks until the quorum
        // completes.
        let c = coordinator.clone();
        let history = tokio::spawn(async move {
            let mut member = false;
            let mut rx = c.subscribe();
            c.arrive("history", &mut member, &mut rx, 13, wm(12)).await;
            member
        });

        // Deterministically wait for "history" to have joined (white-box
        // read of the cohort, shared with this module). On a
        // current-thread runtime the spawned task runs while we yield.
        loop {
            let joined = coordinator.cohort.lock().unwrap().members == 2;
            if joined {
                break;
            }
            tokio::task::yield_now().await;
        }

        // "history" alone has not completed the two-member quorum, so no
        // snapshot at 12 yet.
        assert!(db.at_snapshot(12).is_none());

        // "live" arrives at the frontier (13); the quorum is now
        // complete, so the snapshot at 12 lands and "history" is
        // released.
        {
            let mut member = true;
            let mut rx = coordinator.subscribe();
            coordinator
                .arrive("live", &mut member, &mut rx, 13, wm(12))
                .await;
        }

        assert!(
            history.await.unwrap(),
            "history should have joined the cohort",
        );
        assert!(db.at_snapshot(12).is_some());
        assert_eq!(coordinator.frontier(), 14);
    }
}
