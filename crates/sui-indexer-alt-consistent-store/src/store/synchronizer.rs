// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::Ordering, collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, bail, ensure};
use sui_futures::{future::with_slow_future_monitor, service::Service};
use tokio::sync::{Barrier, mpsc};
use tracing::{debug, info, warn};

use crate::db::{Db, Watermark};

/// The synchronizer will emit a message if it has been waiting to synchronize with other tasks for
/// this long without making progress.
const SLOW_SYNC_WARNING_THRESHOLD: Duration = Duration::from_secs(60);

/// A service that coordinates writes to a database from various registered pipelines, with
/// generating snapshots for that database. The synchronizer ensures that all pipelines have made
/// the same amount of progress before taking a database-wide snapshot.
pub(crate) struct Synchronizer {
    db: Arc<Db>,

    /// The last watermark written to the database for each registered pipeline. The value is
    /// `None` if the database has not yet seen a write for that pipeline.
    last_watermarks: HashMap<&'static str, Option<Watermark>>,

    /// The first checkpoint to be fetched across any pipeline.
    first_checkpoint: u64,

    /// A snapshot is taken every `stride` checkpoints, after pipelines have caught up with each
    /// other.
    stride: u64,

    /// The size of queues that feed each synchronizer task.
    buffer_size: usize,
}

/// Write access to each pipeline's synchronizer task.
pub(super) type Queue = HashMap<&'static str, mpsc::Sender<(Watermark, rocksdb::WriteBatch)>>;

impl Synchronizer {
    /// Create a new synchronizer service that coordinates write and snapshots to `db`.
    ///
    /// `stride` controls the number of checkpoints between snapshots, `buffer_size` controls the
    /// size of the channels that feed each synchronizer task.
    ///
    /// `first_checkpoint` is the first checkpoint the service expects to see written to for
    /// completely new pipelines. If `None`, the first checkpoint is assumed to be `0`.
    ///
    /// The service must be started by calling [Self::run], and it will stop if it has been
    /// instructed to write data out-of-order, or if a write fails.
    pub(crate) fn new(
        db: Arc<Db>,
        stride: u64,
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

    /// Register a new pipeline with the synchronizer service. The synchronizer will spin up a task
    /// for each pipeline, and make a channel available to send writes to that task, when it is
    /// started using [`Self::run`].
    ///
    /// Fails if the database fails to return the pipeline's watermark -- registering a brand new
    /// pipeline is not an error.
    pub(crate) fn register_pipeline(&mut self, pipeline: &'static str) -> anyhow::Result<()> {
        let watermark = self
            .db
            .commit_watermark(pipeline)
            .with_context(|| format!("Failed to get {pipeline} initial watermark"))?;

        self.last_watermarks.insert(pipeline, watermark);
        Ok(())
    }

    /// Start the service, accepting writes for registered pipelines. This consumes the service and
    /// returns a `Service` that will complete when all its tasks have completed, and the `queue`
    /// data structure which gives access to the write side of the channels feeding each task.
    pub(super) fn run(self) -> anyhow::Result<(Service, Queue)> {
        let mut queue = Queue::new();

        let pre_snap = Arc::new(Barrier::new(self.last_watermarks.len()));
        let post_snap = Arc::new(Barrier::new(self.last_watermarks.len()));

        // Calculate the first checkpoint we will have data for, across all pipelines.
        let Some(init_checkpoint) = self
            .last_watermarks
            .values()
            .map(|w| w.map_or(self.first_checkpoint, |w| w.checkpoint_hi_inclusive))
            .max()
        else {
            bail!("No pipelines registered with the synchronizer");
        };

        // The next snapshot should be taken at the next stride boundary after that initial
        // checkpoint.
        let next_snapshot_checkpoint = ((init_checkpoint / self.stride) + 1) * self.stride;

        let mut service = Service::new();
        for (pipeline, last_watermark) in self.last_watermarks {
            let (tx, rx) = mpsc::channel(self.buffer_size);

            queue.insert(pipeline, tx);
            service = service.spawn_aborting(synchronizer(
                self.db.clone(),
                rx,
                pipeline,
                self.first_checkpoint,
                self.stride,
                next_snapshot_checkpoint,
                last_watermark,
                pre_snap.clone(),
                post_snap.clone(),
            ));
        }

        Ok((service, queue))
    }
}

/// The synchronizer task is responsible for landing writes to the database for a given `pipeline`.
/// It also coordinates with other synchronizers to take snapshots of the database every `stride`
/// checkpoints, starting from before the write of the `next_snapshot_checkpoint`.
///
/// Data arrives as a batch-per-checkpoint on `rx`, and must arrive in checkpoint sequence number
/// order (the synchronizer will report an error and stop if it detects an out-of-order batch).
///
/// `pre_snap` and `post_snap` are barriers shared among all synchronizers -- synchronizers wait on
/// `pre_snap` before a snapshot is to be taken, and on `post_snap` after the snapshot is taken
/// (and data from future checkpoints can be written).
///
/// The task will stop if it detects an issue during writes (an out-of-order batch, or an error
/// during writes).
async fn synchronizer(
    db: Arc<Db>,
    mut rx: mpsc::Receiver<(Watermark, rocksdb::WriteBatch)>,
    pipeline: &'static str,
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
            // The next checkpoint should be included in the next snapshot, so allow it to be
            // written.
            Ordering::Greater => {}

            // If the next checkpoint is more than one checkpoint ahead of the next snapshot,
            // something has gone wrong.
            Ordering::Less => {
                bail!(
                    "Missed snapshot {next_snapshot_checkpoint} for {pipeline}, got {next_checkpoint}"
                );
            }

            // The next checkpoint does not belong in the next snapshot, so wait for other
            // synchronizers to reach this point, and take the snapshot before proceeding.
            //
            // One arbitrary task (the "leader") is responsible for taking the snapshot, the others
            // just bump their own synchronization point and wait.
            Ordering::Equal => {
                let take_snapshot =
                    with_slow_future_monitor(pre_snap.wait(), SLOW_SYNC_WARNING_THRESHOLD, || {
                        warn!(pipeline, "Synchronizer stuck, pre-snapshot")
                    })
                    .await
                    .is_leader();

                if take_snapshot {
                    if let Some(watermark) = current_watermark {
                        db.take_snapshot(watermark);
                    } else {
                        bail!(
                            "{pipeline} has no watermark available for snapshot at {next_snapshot_checkpoint}"
                        );
                    }
                }

                next_snapshot_checkpoint += stride;
                with_slow_future_monitor(post_snap.wait(), SLOW_SYNC_WARNING_THRESHOLD, || {
                    warn!(pipeline, "Synchronizer stuck, post-snapshot")
                })
                .await;
            }
        }

        let Some((watermark, batch)) = rx.recv().await else {
            info!(pipeline, "Synchronizer channel closed");
            break;
        };

        debug!(pipeline, ?watermark, "Received batch");
        ensure!(
            watermark.checkpoint_hi_inclusive == next_checkpoint,
            "Out-of-order batch for {pipeline}: expected {next_checkpoint}, got {watermark:?}",
        );

        db.write(pipeline, watermark, batch)
            .with_context(|| format!("Failed to write batch for {pipeline} at {watermark:?}"))?;

        current_watermark = Some(watermark);
    }

    info!(
        pipeline,
        next_snapshot_checkpoint, watermark = ?current_watermark, "Stopping sync"
    );

    Ok(())
}
