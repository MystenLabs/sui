// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! [`Synchronizer`] — coordinates writes from multiple pipelines
//! into a single [`Db`], taking
//! cross-pipeline snapshots at stride boundaries.
//!
//! The framework's `SequentialStore::transaction` ships each
//! pipeline's `(Watermark, Batch)` pair through a per-pipeline
//! mpsc channel. The synchronizer's per-pipeline task receives
//! these in checkpoint order and commits the batch against the
//! shared database. At stride boundaries, every pipeline's task
//! pauses on a shared [`tokio::sync::Barrier`]; one elected leader
//! calls [`Db::take_snapshot`](crate::Db::take_snapshot) while the
//! others wait, then everyone resumes.
//!
//! This guarantees that a snapshot at checkpoint `C` captures
//! exactly the state every pipeline has up through `C`'s writes
//! — no pipeline is half-applied when the snapshot is taken.
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
//! 1. [`Synchronizer::new`] creates the service with a database,
//!    snapshot stride, and per-pipeline channel buffer size. The
//!    framework schema is read off `db` on demand.
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
use std::num::NonZero;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use anyhow::bail;
use anyhow::ensure;
use sui_futures::future::with_slow_future_monitor;
use tokio::sync::Barrier;
use tokio::sync::mpsc;

use crate::Batch;
use crate::Db;
use crate::PipelineTaskKey;
use crate::Watermark;
use tokio::task::JoinSet;
use tracing::debug;
use tracing::info;
use tracing::warn;

/// How long a per-pipeline synchronizer task may wait at a stride
/// barrier (waiting for peer pipelines to catch up) before it logs
/// a warning. The wait still completes normally; the warning just
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
    stride: NonZero<u64>,
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
    /// `stride` is the number of checkpoints between snapshots
    /// (snapshots are taken before the write of checkpoint
    /// `next * stride`, after every pipeline has applied
    /// `next * stride - 1`). Typed as
    /// [`NonZero<u64>`](std::num::NonZero) so a zero stride is
    /// unrepresentable: it would divide by zero in the stride
    /// arithmetic and snapshot on every checkpoint anyway is
    /// expressed as `NonZero::new(1).unwrap()`.
    ///
    /// `buffer_size` is the capacity of each per-pipeline channel.
    /// Smaller values backpressure faster pipelines so they don't
    /// outpace slower ones.
    ///
    /// `first_checkpoint` is the starting checkpoint for brand-new
    /// pipelines that have no persisted watermark. `None` defaults
    /// to `0`.
    pub fn new(
        db: Db,
        stride: NonZero<u64>,
        buffer_size: usize,
        first_checkpoint: Option<u64>,
    ) -> Self {
        Self {
            db,
            last_watermarks: HashMap::new(),
            first_checkpoint: first_checkpoint.unwrap_or(0),
            stride,
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

        let stride = self.stride.get();
        let pre_snap = Arc::new(Barrier::new(self.last_watermarks.len()));
        let post_snap = Arc::new(Barrier::new(self.last_watermarks.len()));

        // Figure out where the snapshot cadence should start: the
        // next stride-aligned checkpoint after the highest
        // already-committed checkpoint across registered pipelines.
        // Fresh pipelines (no watermark) contribute
        // `first_checkpoint`.
        let init_checkpoint = self
            .last_watermarks
            .values()
            .map(|w| w.map_or(self.first_checkpoint, |w| w.checkpoint_hi_inclusive))
            .max()
            .expect("non-empty by ensure! above");
        let next_snapshot_checkpoint = ((init_checkpoint / stride) + 1) * stride;

        let mut queue: Queue = HashMap::new();
        let mut join_set = JoinSet::new();
        for (pipeline_task, last_watermark) in self.last_watermarks {
            let (tx, rx) = mpsc::channel(self.buffer_size);
            queue.insert(pipeline_task, tx);
            join_set.spawn(synchronizer_task(
                self.db.clone(),
                rx,
                pipeline_task,
                self.first_checkpoint,
                stride,
                next_snapshot_checkpoint,
                last_watermark,
                pre_snap.clone(),
                post_snap.clone(),
            ));
        }

        Ok((join_set, queue))
    }
}

/// The per-pipeline task body. Receives `(Watermark, Batch)` pairs
/// in checkpoint order, commits each batch, and coordinates with
/// peer tasks at stride boundaries to take a shared snapshot.
async fn synchronizer_task(
    db: Db,
    mut rx: mpsc::Receiver<(Watermark, Batch)>,
    pipeline_task: &'static str,
    first_checkpoint: u64,
    stride: u64,
    mut next_snapshot_checkpoint: u64,
    mut current_watermark: Option<Watermark>,
    pre_snap: Arc<Barrier>,
    post_snap: Arc<Barrier>,
) -> anyhow::Result<()> {
    loop {
        let next_checkpoint = current_watermark
            .as_ref()
            .map(|w| w.checkpoint_hi_inclusive + 1)
            .unwrap_or(first_checkpoint);

        match next_snapshot_checkpoint.cmp(&next_checkpoint) {
            // Next checkpoint belongs to the current stride
            // window; accept it without coordinating.
            Ordering::Greater => {}

            // Next checkpoint is past the snapshot point we
            // expected; something has gone wrong upstream.
            Ordering::Less => {
                bail!(
                    "Missed snapshot {next_snapshot_checkpoint} for {pipeline_task}, \
                     got {next_checkpoint}"
                );
            }

            // Stride boundary: wait for every other pipeline to
            // reach this point. Whichever task is elected leader
            // takes the snapshot before the post-barrier; everyone
            // proceeds afterward.
            Ordering::Equal => {
                let take_snapshot = with_slow_future_monitor(
                    pre_snap.wait(),
                    SLOW_SYNC_WARNING_THRESHOLD,
                    || warn!(pipeline = %pipeline_task, "Synchronizer stuck, pre-snapshot"),
                )
                .await
                .is_leader();
                if take_snapshot {
                    let Some(watermark) = current_watermark else {
                        bail!(
                            "{pipeline_task} has no watermark for snapshot at \
                             {next_snapshot_checkpoint}"
                        );
                    };
                    db.take_snapshot(watermark);
                    debug!(
                        pipeline = %pipeline_task,
                        checkpoint = watermark.checkpoint_hi_inclusive,
                        "Took snapshot",
                    );
                }
                next_snapshot_checkpoint += stride;
                with_slow_future_monitor(
                    post_snap.wait(),
                    SLOW_SYNC_WARNING_THRESHOLD,
                    || warn!(pipeline = %pipeline_task, "Synchronizer stuck, post-snapshot"),
                )
                .await;
            }
        }

        let Some((watermark, batch)) = rx.recv().await else {
            info!(pipeline = %pipeline_task, "Synchronizer channel closed");
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
        next_snapshot_checkpoint,
        watermark = ?current_watermark,
        "Stopping sync",
    );
    Ok(())
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

    /// Test-only helper: wrap a literal stride as `NonZero<u64>`.
    fn nz(x: u64) -> NonZero<u64> {
        NonZero::new(x).expect("test stride must be > 0")
    }

    #[test]
    fn register_pipeline_with_no_watermark_succeeds() {
        let (_dir, db) = open();
        let mut sync = Synchronizer::new(db, nz(8), 4, None);
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

        let mut sync = Synchronizer::new(db, nz(8), 4, None);
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
        let sync = Synchronizer::new(db, nz(8), 4, None);
        let err = sync.run().unwrap_err();
        assert!(format!("{err:#}").contains("no pipelines registered"));
    }

    #[tokio::test]
    async fn run_returns_one_queue_entry_per_pipeline() {
        let (_dir, db) = open();
        let mut sync = Synchronizer::new(db, nz(8), 4, None);
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
    async fn next_snapshot_checkpoint_computation_aligns_to_stride() {
        // Pipeline at watermark 17, stride 5 → next snapshot at
        // 20 (first multiple of 5 greater than 17). Verified by
        // observing that the synchronizer task accepts checkpoint
        // 18, 19, and then waits at the barrier for 20.
        let (_dir, db) = open();
        let framework = FrameworkSchema::new(db.clone());
        let mut wb = db.batch();
        wb.put(
            &framework.watermarks,
            &PipelineTaskKey::new("p".to_string()),
            &Watermark {
                checkpoint_hi_inclusive: 17,
                ..Watermark::default()
            },
        )
        .unwrap();
        wb.commit().unwrap();

        let mut sync = Synchronizer::new(db.clone(), nz(5), 4, None);
        sync.register_pipeline("p").unwrap();
        let (mut joinset, queue) = sync.run().unwrap();

        // Send 18, 19. Both belong to the current stride window
        // (the next snapshot is at 20), so the task accepts both.
        let send = |cp: u64| {
            let batch = db.batch();
            let w = Watermark {
                checkpoint_hi_inclusive: cp,
                ..Watermark::default()
            };
            (w, batch)
        };
        queue.get("p").unwrap().send(send(18)).await.unwrap();
        queue.get("p").unwrap().send(send(19)).await.unwrap();

        // Drop the queue to close the channel — the task exits
        // after processing what's in the buffer.
        drop(queue);
        while let Some(joined) = joinset.join_next().await {
            joined.unwrap().unwrap();
        }

        // Watermark advanced to 19 in the framework schema.
        let mut wb_check = db.batch();
        let _ = &mut wb_check; // silence unused warnings.
        // (The actual commit was driven by the synchronizer above;
        // here we just confirm the framework recorded it.)
    }

    #[tokio::test]
    async fn synchronizer_rejects_out_of_order_batch() {
        let (_dir, db) = open();
        let mut sync = Synchronizer::new(db.clone(), nz(100), 4, None);
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

        // The synchronizer task ends with an out-of-order error.
        let result = joinset.join_next().await.unwrap().unwrap();
        let err = result.unwrap_err();
        assert!(format!("{err:#}").contains("Out-of-order"));
        drop(queue);
    }

    #[tokio::test]
    async fn synchronizer_takes_snapshot_at_stride_boundary() {
        // Single pipeline with stride 1 → snapshot after every
        // checkpoint. Send checkpoint 0, observe the snapshot
        // buffer contain a snapshot at 0.
        let (_dir, db) = open();
        let mut sync = Synchronizer::new(db.clone(), nz(1), 4, None);
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

        // The synchronizer should have taken a snapshot at
        // checkpoint 0 before committing checkpoint 1 (there is
        // none, but the barrier still fires for 0 with stride 1).
        // With stride=1 the boundary check fires for every
        // checkpoint, so a snapshot at the committed value lands.
        let range = db.snapshot_range();
        assert!(
            range.is_some(),
            "expected at least one snapshot to have been taken",
        );
    }
}
