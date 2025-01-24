// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sui_pg_db::{self as db, Db};
use sui_types::full_checkpoint_content::CheckpointData;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use super::{processor::processor, CommitterConfig, Processor, PIPELINE_BUFFER};

use crate::{metrics::IndexerMetrics, models::watermarks::CommitterWatermark};

use self::committer::committer;

mod committer;

/// Handlers implement the logic for a given indexing pipeline: How to process checkpoint data (by
/// implementing [Processor]) into rows for their table, how to combine multiple rows into a single
/// DB operation, and then how to write those rows atomically to the database.
///
/// The handler is also responsible for tuning the various parameters of the pipeline (provided as
/// associated values).
///
/// Sequential handlers can only be used in sequential pipelines, where checkpoint data is
/// processed out-of-order, but then gathered and written in order. If multiple checkpoints are
/// available, the pipeline will attempt to combine their writes taking advantage of batching to
/// avoid emitting redundant writes.
///
/// Back-pressure is handled by setting a high watermark on the ingestion service: The pipeline
/// notifies the ingestion service of the checkpoint it last successfully wrote to the database
/// for, and in turn the ingestion service will only run ahead by its buffer size. This guarantees
/// liveness and limits the amount of memory the pipeline can consume, by bounding the number of
/// checkpoints that can be received before the next checkpoint.
#[async_trait::async_trait]
pub trait Handler: Processor {
    /// If at least this many rows are pending, the committer will commit them eagerly.
    const MIN_EAGER_ROWS: usize = 50;

    /// Maximum number of checkpoints to try and write in a single batch. The larger this number
    /// is, the more chances the pipeline has to merge redundant writes, but the longer each write
    /// transaction is likely to be.
    const MAX_BATCH_CHECKPOINTS: usize = 5 * 60;

    /// A type to combine multiple `Self::Value`-s into. This can be used to avoid redundant writes
    /// by combining multiple rows into one (e.g. if one row supersedes another, the latter can be
    /// omitted).
    type Batch: Default + Send + Sync + 'static;

    /// Add `values` from processing a checkpoint to the current `batch`. Checkpoints are
    /// guaranteed to be presented to the batch in checkpoint order.
    fn batch(batch: &mut Self::Batch, values: Vec<Self::Value>);

    /// Take a batch of values and commit them to the database, returning the number of rows
    /// affected.
    async fn commit(batch: &Self::Batch, conn: &mut db::Connection<'_>) -> anyhow::Result<usize>;
}

/// Configuration for a sequential pipeline
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SequentialConfig {
    /// Configuration for the writer, that makes forward progress.
    pub committer: CommitterConfig,

    /// How many checkpoints to hold back writes for.
    pub checkpoint_lag: u64,
}

/// Start a new sequential (in-order) indexing pipeline, served by the handler, `H`. Starting
/// strictly after the `watermark` (or from the beginning if no watermark was provided).
///
/// Each pipeline consists of a processor which takes checkpoint data and breaks it down into rows,
/// ready for insertion, and a committer which orders the rows and combines them into batches to
/// write to the database.
///
/// Commits are performed in checkpoint order, potentially involving multiple checkpoints at a
/// time. The call to [Handler::commit] and the associated watermark update are performed in a
/// transaction to ensure atomicity. Unlike in the case of concurrent pipelines, the data passed to
/// [Handler::commit] is not chunked up, so the handler must perform this step itself, if
/// necessary.
///
/// The pipeline can optionally be configured to lag behind the ingestion service by a fixed number
/// of checkpoints (configured by `checkpoint_lag`).
///
/// Watermarks are also shared with the ingestion service, which is guaranteed to bound the
/// checkpoint height it pre-fetches to some constant additive factor above the pipeline's
/// watermark.
///
/// Checkpoint data is fed into the pipeline through the `checkpoint_rx` channel, watermark updates
/// are communicated to the ingestion service through the `watermark_tx` channel and internal
/// channels are created to communicate between its various components. The pipeline can be
/// shutdown using its `cancel` token, and will also shutdown if any of its input or output
/// channels close, or any of its independent tasks fail.
pub(crate) fn pipeline<H: Handler + Send + Sync + 'static>(
    handler: H,
    initial_watermark: Option<CommitterWatermark<'static>>,
    config: SequentialConfig,
    db: Db,
    checkpoint_rx: mpsc::Receiver<Arc<CheckpointData>>,
    watermark_tx: mpsc::UnboundedSender<(&'static str, u64)>,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    let (processor_tx, committer_rx) = mpsc::channel(H::FANOUT + PIPELINE_BUFFER);

    let processor = processor(
        Arc::new(handler),
        checkpoint_rx,
        processor_tx,
        metrics.clone(),
        cancel.clone(),
    );

    let committer = committer::<H>(
        config,
        initial_watermark,
        committer_rx,
        watermark_tx,
        db.clone(),
        metrics.clone(),
        cancel.clone(),
    );

    tokio::spawn(async move {
        let (_, _) = futures::join!(processor, committer);
    })
}
