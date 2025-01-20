// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use sui_field_count::FieldCount;
use sui_pg_db::{self as db, Db};
use sui_types::full_checkpoint_content::CheckpointData;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{metrics::IndexerMetrics, models::watermarks::CommitterWatermark};

use super::{processor::processor, CommitterConfig, Processor, WatermarkPart, PIPELINE_BUFFER};

use self::{
    collector::collector, commit_watermark::commit_watermark, committer::committer, pruner::pruner,
    reader_watermark::reader_watermark,
};

mod collector;
mod commit_watermark;
mod committer;
mod pruner;
mod reader_watermark;

/// The maximum number of watermarks that can show up in a single batch. This limit exists to deal
/// with pipelines that produce no data for a majority of checkpoints -- the size of these
/// pipeline's batches will be dominated by watermark updates.
const MAX_WATERMARK_UPDATES: usize = 10_000;

/// Handlers implement the logic for a given indexing pipeline: How to process checkpoint data (by
/// implementing [Processor]) into rows for their table, and how to write those rows to the database.
///
/// The handler is also responsible for tuning the various parameters of the pipeline (provided as
/// associated values). Reasonable defaults have been chosen to balance concurrency with memory
/// usage, but each handle may choose to override these defaults, e.g.
///
/// - Handlers that produce many small rows may wish to increase their batch/chunk/max-pending
///   sizes).
/// - Handlers that do more work during processing may wish to increase their fanout so more of it
///   can be done concurrently, to preserve throughput.
///
/// Concurrent handlers can only be used in concurrent pipelines, where checkpoint data is
/// processed and committed out-of-order and a watermark table is kept up-to-date with the latest
/// checkpoint below which all data has been committed.
///
/// Back-pressure is handled through the `MAX_PENDING_SIZE` constant -- if more than this many rows
/// build up, the collector will stop accepting new checkpoints, which will eventually propagate
/// back to the ingestion service.
#[async_trait::async_trait]
pub trait Handler: Processor<Value: FieldCount> {
    /// If at least this many rows are pending, the committer will commit them eagerly.
    const MIN_EAGER_ROWS: usize = 50;

    /// If there are more than this many rows pending, the committer applies backpressure.
    const MAX_PENDING_ROWS: usize = 5000;

    /// Whether the pruner requires processed values in order to prune.
    /// This will determine the first checkpoint to process when we start the pipeline.
    /// If this is true, when the pipeline starts, it will process all checkpoints from the
    /// pruner watermark, so that the pruner have access to the processed values for any unpruned
    /// checkpoints.
    /// If this is false, when the pipeline starts, it will process all checkpoints from the
    /// committer watermark.
    // TODO: There are two issues with this:
    // 1. There is no static guarantee that this flag is set correctly when the pruner needs processed values.
    // 2. The name is a bit abstract.
    const PRUNING_REQUIRES_PROCESSED_VALUES: bool = false;

    /// Take a chunk of values and commit them to the database, returning the number of rows
    /// affected.
    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>)
        -> anyhow::Result<usize>;

    /// Clean up data between checkpoints `_from` and `_to_exclusive` (exclusive) in the database, returning
    /// the number of rows affected. This function is optional, and defaults to not pruning at all.
    async fn prune(
        &self,
        _from: u64,
        _to_exclusive: u64,
        _conn: &mut db::Connection<'_>,
    ) -> anyhow::Result<usize> {
        Ok(0)
    }
}

/// Configuration for a concurrent pipeline
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ConcurrentConfig {
    /// Configuration for the writer, that makes forward progress.
    pub committer: CommitterConfig,

    /// Configuration for the pruner, that deletes old data.
    pub pruner: Option<PrunerConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrunerConfig {
    /// How often the pruner should check whether there is any data to prune, in milliseconds.
    pub interval_ms: u64,

    /// How long to wait after the reader low watermark was set, until it is safe to prune up until
    /// this new watermark, in milliseconds.
    pub delay_ms: u64,

    /// How much data to keep, this is measured in checkpoints.
    pub retention: u64,

    /// The maximum range to try and prune in one request, measured in checkpoints.
    pub max_chunk_size: u64,

    /// The max number of tasks to run in parallel for pruning.
    pub prune_concurrency: u64,
}

/// Values ready to be written to the database. This is an internal type used to communicate
/// between the collector and the committer parts of the pipeline.
///
/// Values inside each batch may or may not be from the same checkpoint. Values in the same
/// checkpoint can also be split across multiple batches.
struct BatchedRows<H: Handler> {
    /// The rows to write
    values: Vec<H::Value>,
    /// Proportions of all the watermarks that are represented in this chunk
    watermark: Vec<WatermarkPart>,
}

impl PrunerConfig {
    pub fn interval(&self) -> Duration {
        Duration::from_millis(self.interval_ms)
    }

    pub fn delay(&self) -> Duration {
        Duration::from_millis(self.delay_ms)
    }
}

impl<H: Handler> BatchedRows<H> {
    fn new() -> Self {
        Self {
            values: vec![],
            watermark: vec![],
        }
    }

    /// Number of rows in this batch.
    fn len(&self) -> usize {
        self.values.len()
    }

    /// The batch is full if it has more than enough values to write to the database, or more than
    /// enough watermarks to update.
    fn is_full(&self) -> bool {
        self.values.len() >= max_chunk_rows::<H>() || self.watermark.len() >= MAX_WATERMARK_UPDATES
    }
}

impl Default for PrunerConfig {
    fn default() -> Self {
        Self {
            interval_ms: 300_000,
            delay_ms: 120_000,
            retention: 4_000_000,
            max_chunk_size: 2_000,
            prune_concurrency: 1,
        }
    }
}

/// Start a new concurrent (out-of-order) indexing pipeline served by the handler, `H`. Starting
/// strictly after the `watermark` (or from the beginning if no watermark was provided).
///
/// Each pipeline consists of a processor task which takes checkpoint data and breaks it down into
/// rows, ready for insertion, a collector which batches those rows into an appropriate size for
/// the database, a committer which writes the rows out concurrently, and a watermark task to
/// update the high watermark.
///
/// Committing is performed out-of-order: the pipeline may write out checkpoints out-of-order,
/// either because it received the checkpoints out-of-order or because of variance in processing
/// time.
///
/// The pipeline also maintains a row in the `watermarks` table for the pipeline which tracks the
/// watermark below which all data has been committed (modulo pruning), as long as `skip_watermark`
/// is not true.
///
/// Checkpoint data is fed into the pipeline through the `checkpoint_rx` channel, and internal
/// channels are created to communicate between its various components. The pipeline can be
/// shutdown using its `cancel` token, and will also shutdown if any of its independent tasks
/// reports an issue.
pub(crate) fn pipeline<H: Handler + Send + Sync + 'static>(
    handler: H,
    initial_commit_watermark: Option<CommitterWatermark<'static>>,
    config: ConcurrentConfig,
    skip_watermark: bool,
    db: Db,
    checkpoint_rx: mpsc::Receiver<Arc<CheckpointData>>,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    info!(
        pipeline = H::NAME,
        "Starting pipeline with config: {:?}", config
    );
    let ConcurrentConfig {
        committer: committer_config,
        pruner: pruner_config,
    } = config;

    let (processor_tx, collector_rx) = mpsc::channel(H::FANOUT + PIPELINE_BUFFER);
    let (collector_tx, committer_rx) =
        mpsc::channel(committer_config.write_concurrency + PIPELINE_BUFFER);
    let (committer_tx, watermark_rx) =
        mpsc::channel(committer_config.write_concurrency + PIPELINE_BUFFER);

    // The pruner is not connected to the rest of the tasks by channels, so it needs to be
    // explicitly signalled to shutdown when the other tasks shutdown, in addition to listening to
    // the global cancel signal. We achieve this by creating a child cancel token that we call
    // cancel on once the committer tasks have shutdown.
    let pruner_cancel = cancel.child_token();
    let handler = Arc::new(handler);

    let processor = processor(
        handler.clone(),
        checkpoint_rx,
        processor_tx,
        metrics.clone(),
        cancel.clone(),
    );

    let collector = collector::<H>(
        committer_config.clone(),
        collector_rx,
        collector_tx,
        metrics.clone(),
        cancel.clone(),
    );

    let committer = committer::<H>(
        committer_config.clone(),
        skip_watermark,
        committer_rx,
        committer_tx,
        db.clone(),
        metrics.clone(),
        cancel.clone(),
    );

    let commit_watermark = commit_watermark::<H>(
        initial_commit_watermark,
        committer_config,
        skip_watermark,
        watermark_rx,
        db.clone(),
        metrics.clone(),
        cancel,
    );

    let reader_watermark = reader_watermark::<H>(
        pruner_config.clone(),
        db.clone(),
        metrics.clone(),
        pruner_cancel.clone(),
    );

    let pruner = pruner(handler, pruner_config, db, metrics, pruner_cancel.clone());

    tokio::spawn(async move {
        let (_, _, _, _) = futures::join!(processor, collector, committer, commit_watermark);

        pruner_cancel.cancel();
        let _ = futures::join!(reader_watermark, pruner);
    })
}

const fn max_chunk_rows<H: Handler>() -> usize {
    if H::Value::FIELD_COUNT == 0 {
        i16::MAX as usize
    } else {
        i16::MAX as usize / H::Value::FIELD_COUNT
    }
}
