// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_types::full_checkpoint_content::CheckpointData;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::{
    db::{self, Db},
    metrics::IndexerMetrics,
    models::watermarks::CommitterWatermark,
};

use super::{processor::processor, PipelineConfig, Processor, WatermarkPart, PIPELINE_BUFFER};

use self::{collector::collector, committer::committer, watermark::watermark};

mod collector;
mod committer;
mod watermark;

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
pub trait Handler: Processor {
    /// If at least this many rows are pending, the committer will commit them eagerly.
    const MIN_EAGER_ROWS: usize = 50;

    /// If there are more than this many rows pending, the committer will only commit this many in
    /// one operation.
    const MAX_CHUNK_ROWS: usize = 200;

    /// If there are more than this many rows pending, the committer applies backpressure.
    const MAX_PENDING_ROWS: usize = 1000;

    /// Take a chunk of values and commit them to the database, returning the number of rows
    /// affected.
    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>)
        -> anyhow::Result<usize>;
}

/// Values ready to be written to the database. This is an internal type used to communicate
/// between the collector and the committer parts of the pipeline.
struct Batched<H: Handler> {
    /// The rows to write
    values: Vec<H::Value>,
    /// Proportions of all the watermarks that are represented in this chunk
    watermark: Vec<WatermarkPart>,
}

impl<H: Handler> Batched<H> {
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
        self.values.len() >= H::MAX_CHUNK_ROWS || self.watermark.len() >= MAX_WATERMARK_UPDATES
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
/// watermark below which all data has been committed (modulo pruning).
///
/// Checkpoint data is fed into the pipeline through the `checkpoint_rx` channel, and internal
/// channels are created to communicate between its various components. The pipeline can be
/// shutdown using its `cancel` token, and will also shutdown if any of its independent tasks
/// reports an issue.
pub(crate) fn pipeline<H: Handler + 'static>(
    initial_watermark: Option<CommitterWatermark<'static>>,
    config: PipelineConfig,
    db: Db,
    checkpoint_rx: mpsc::Receiver<Arc<CheckpointData>>,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> (
    JoinHandle<()>,
    JoinHandle<()>,
    JoinHandle<()>,
    JoinHandle<()>,
) {
    let (processor_tx, collector_rx) = mpsc::channel(H::FANOUT + PIPELINE_BUFFER);
    let (collector_tx, committer_rx) = mpsc::channel(config.write_concurrency + PIPELINE_BUFFER);
    let (committer_tx, watermark_rx) = mpsc::channel(config.write_concurrency + PIPELINE_BUFFER);

    let processor = processor::<H>(checkpoint_rx, processor_tx, metrics.clone(), cancel.clone());

    let collector = collector::<H>(
        config.clone(),
        collector_rx,
        collector_tx,
        metrics.clone(),
        cancel.clone(),
    );

    let committer = committer::<H>(
        config.clone(),
        committer_rx,
        committer_tx,
        db.clone(),
        metrics.clone(),
        cancel.clone(),
    );

    let watermark = watermark::<H>(initial_watermark, config, watermark_rx, db, metrics, cancel);

    (processor, collector, committer, watermark)
}
