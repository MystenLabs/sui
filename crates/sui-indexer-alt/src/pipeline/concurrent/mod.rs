// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_types::full_checkpoint_content::CheckpointData;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::{
    db::Db, handlers::Handler, metrics::IndexerMetrics, models::watermarks::CommitterWatermark,
};

use super::{processor::processor, PipelineConfig, PIPELINE_BUFFER};

use self::{collector::collector, committer::committer, watermark::watermark};

mod collector;
mod committer;
mod watermark;

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
pub fn pipeline<H: Handler + 'static>(
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
