// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::Ordering, collections::HashMap, sync::Arc, time::Duration};

use anyhow::Context;
use futures::future;
use sui_indexer_alt_framework::task::with_slow_future_monitor;
use tokio::{
    sync::{mpsc, Barrier},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::db::{Db, Watermark};

/// The synchronizer will emit a message if it has been waiting to synchronize with other tasks for
/// this long without making progress.
const SLOW_SYNC_WARNING: Duration = Duration::from_secs(60);

/// A service that coordinates writes to a database from various registered pipelines, with
/// generating snapshots for that database. The synchronizer ensures that all pipelines have made
/// the same amount of progress before taking a database-wide snapshot.
pub(crate) struct Synchronizer {
    db: Arc<Db>,

    /// The last checkpoint written to the database for each registered pipeline. The value is
    /// `None` if the database has not yet seen a write for that pipeline.
    last_checkpoints: HashMap<&'static str, Option<u64>>,

    /// The first checkpoint to be fetched across any pipeline.
    first_checkpoint: u64,

    /// A snapshot is taken every `stride` checkpoints, after pipelines have caught up with each
    /// other.
    stride: u64,

    /// The size of queues that feed each synchronizer task.
    buffer_size: usize,

    // Cancellation token shared among all synchronizer tasks.
    cancel: CancellationToken,
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
    /// The service must be started by calling [Self::run], and it will stop if signalled on
    /// `cancel`, if it has been instructed to write data out-of-order, or if a write fails.
    pub(crate) fn new(
        db: Arc<Db>,
        stride: u64,
        buffer_size: usize,
        first_checkpoint: Option<u64>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            db,
            last_checkpoints: HashMap::new(),
            first_checkpoint: first_checkpoint.unwrap_or(0),
            stride,
            buffer_size,
            cancel,
        }
    }

    /// Register a new pipeline with the synchronizer service. Fails if the database fails to
    /// return the pipeline's watermark -- registering a brand new pipeline is not an error.
    pub(crate) fn add_pipeline(&mut self, pipeline: &'static str) -> anyhow::Result<()> {
        let watermark = self
            .db
            .watermark(pipeline)
            .with_context(|| format!("Failed to get {pipeline} initial watermark"))?
            .map(|w| w.checkpoint_hi_inclusive);

        self.last_checkpoints.insert(pipeline, watermark);
        Ok(())
    }

    /// Start the service, accepting writes for registered pipelines. This consumes the service and
    /// returns a `JoinHandle<()>` that will complete when all its tasks have completed, and the
    /// `queue` data structure which gives access to the write side of the channels feeding each
    /// task.
    pub(super) fn run(self) -> anyhow::Result<(JoinHandle<()>, Queue)> {
        let mut queue = Queue::new();
        let mut tasks = Vec::new();

        let pre_snap = Arc::new(Barrier::new(self.last_checkpoints.len()));
        let post_snap = Arc::new(Barrier::new(self.last_checkpoints.len()));

        // All tasks will arrange to take a snapshot before writing the next checkpoint that is new
        // to all pipelines (allowing all pipelines to catch up with each other).
        let mut next_snapshot = 0;
        for (pipeline, last_checkpoint) in self.last_checkpoints {
            let (tx, rx) = mpsc::channel(self.buffer_size);
            let next_checkpoint = last_checkpoint
                .map(|v| v + 1)
                .unwrap_or(self.first_checkpoint);

            next_snapshot = next_snapshot.max(next_checkpoint);

            queue.insert(pipeline, tx);
            tasks.push(synchronizer(
                self.db.clone(),
                rx,
                pipeline,
                self.stride,
                next_snapshot,
                next_checkpoint,
                pre_snap.clone(),
                post_snap.clone(),
                self.cancel.child_token(),
            ));
        }

        let handle = tokio::spawn(async move {
            future::join_all(tasks).await;
            info!("Synchronizer gracefully shut down");
        });

        Ok((handle, queue))
    }
}

/// The synchronizer task is responsible for landing writes to the database for a given `pipeline`.
/// It also coordinates with other synchronizers to take snapshots of the database every `stride`
/// checkpoints, starting from before the write of the `next_snapshot` checkpoint.
///
/// Data arrives as a batch-per-checkpoint on `rx`, and must arrive in checkpoint sequence number
/// order (the synchronizer will report an error and stop if it detects an out-of-order batch).
/// starting with `next_checkpoint`.
///
/// `pre_snap` and `post_snap` are barriers shared among all synchronizers -- synchronizers wait on
/// `pre_snap` before a snapshot is to be taken, and on `post_snap` after the snapshot is taken
/// (and data from future checkpoints can be written).
///
/// The task will stop when it receives a shutdown signal on `cancel`, or if it detects an issue
/// during writes (an out-of-order batch, or an error during writes).
fn synchronizer(
    db: Arc<Db>,
    mut rx: mpsc::Receiver<(Watermark, rocksdb::WriteBatch)>,
    pipeline: &'static str,
    stride: u64,
    mut next_snapshot: u64,
    mut next_checkpoint: u64,
    pre_snap: Arc<Barrier>,
    post_snap: Arc<Barrier>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    if next_snapshot == 0 {
        // Ignore a snapshot before checkpoint zero.
        next_snapshot += stride;
    }

    tokio::spawn(async move {
        loop {
            match next_snapshot.cmp(&next_checkpoint) {
                // The next checkpoint should be included in the next snapshot, so allow it to be
                // written.
                Ordering::Greater => {}

                // If the next checkpoint is more than one checkpoint ahead of the next snapshot,
                // something has gone wrong.
                Ordering::Less => {
                    error!(pipeline, next_snapshot, next_checkpoint, "Missed snapshot");
                    break;
                }

                // The next checkpoint does not belong in the next snapshot, so wait for other
                // synchronizers to reach this point, and take the snapshot before proceeding.
                //
                // One arbitrary task (the "leader") is responsible for taking the snapshot, the others
                // just bump their own synchronization point and wait.
                Ordering::Equal => {
                    tokio::select! {
                        w = with_slow_future_monitor(pre_snap.wait(), SLOW_SYNC_WARNING, || {
                            warn!(pipeline, "Synchronizer stuck, pre-snapshot")
                        }) => if w.is_leader() {
                            db.snapshot(next_snapshot - 1);
                        },

                        _ = cancel.cancelled() => {
                            info!(pipeline, "Shutdown received before pre-snapshot barrier");
                            break;
                        }
                    }

                    next_snapshot += stride;
                    tokio::select! {
                        _ = with_slow_future_monitor(post_snap.wait(), SLOW_SYNC_WARNING, || {
                            warn!(pipeline, "Synchronizer stuck, post-snapshot")
                        }) => {}
                        _ = cancel.cancelled() => {
                            info!(pipeline, "Shutdown received before post-snapshot barrier");
                            break;
                        }
                    }
                }
            }

            tokio::select! {
                _ = cancel.cancelled() => {
                    info!(pipeline, "Shutdown received waiting for batch");
                    break;
                }

                Some((watermark, batch)) = rx.recv() => {
                    debug!(pipeline, ?watermark, "Received batch");
                    if watermark.checkpoint_hi_inclusive != next_checkpoint {
                        error!(pipeline, ?watermark, next_checkpoint, "Out-of-order batch");
                        break;
                    }

                    if let Err(e) = db.write(pipeline, watermark, batch) {
                        error!(pipeline, ?e, "Failed to write batch");
                        break;
                    }

                    next_checkpoint += 1;
                }
            }
        }

        info!(pipeline, next_snapshot, next_checkpoint, "Stopping sync");
    })
}
