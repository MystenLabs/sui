// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};

use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tap::tap::TapFallible;
use tokio_util::sync::CancellationToken;
use tracing::instrument;
use tracing::{error, info};

use crate::metrics::IndexerMetrics;
use crate::models::raw_checkpoints::StoredRawCheckpoint;
use crate::store::IndexerStore;
use crate::types::IndexerResult;

use super::{CheckpointDataToCommit, CommitterTables, CommitterWatermark, EpochToCommit};

pub(crate) const CHECKPOINT_COMMIT_BATCH_SIZE: usize = 100;

pub async fn start_tx_checkpoint_commit_task<S>(
    state: S,
    metrics: IndexerMetrics,
    tx_indexing_receiver: mysten_metrics::metered_channel::Receiver<CheckpointDataToCommit>,
    cancel: CancellationToken,
    mut next_checkpoint_sequence_number: CheckpointSequenceNumber,
    end_checkpoint_opt: Option<CheckpointSequenceNumber>,
    mvr_mode: bool,
) -> IndexerResult<()>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    use futures::StreamExt;

    info!("Indexer checkpoint commit task started...");
    let checkpoint_commit_batch_size = std::env::var("CHECKPOINT_COMMIT_BATCH_SIZE")
        .unwrap_or(CHECKPOINT_COMMIT_BATCH_SIZE.to_string())
        .parse::<usize>()
        .unwrap();
    info!("Using checkpoint commit batch size {checkpoint_commit_batch_size}");

    let mut stream = mysten_metrics::metered_channel::ReceiverStream::new(tx_indexing_receiver)
        .ready_chunks(checkpoint_commit_batch_size);

    let mut unprocessed = HashMap::new();
    let mut batch = vec![];

    while let Some(indexed_checkpoint_batch) = stream.next().await {
        if cancel.is_cancelled() {
            break;
        }

        // split the batch into smaller batches per epoch to handle partitioning
        for checkpoint in indexed_checkpoint_batch {
            unprocessed.insert(checkpoint.checkpoint.sequence_number, checkpoint);
        }
        while let Some(checkpoint) = unprocessed.remove(&next_checkpoint_sequence_number) {
            let epoch = checkpoint.epoch.clone();
            batch.push(checkpoint);
            next_checkpoint_sequence_number += 1;
            let epoch_number_option = epoch.as_ref().map(|epoch| epoch.new_epoch_id());
            // The batch will consist of contiguous checkpoints and at most one epoch boundary at
            // the end.
            if batch.len() == checkpoint_commit_batch_size || epoch.is_some() {
                commit_checkpoints(&state, batch, epoch, &metrics, mvr_mode).await;
                batch = vec![];
            }
            if let Some(epoch_number) = epoch_number_option {
                state.upload_display(epoch_number).await.tap_err(|e| {
                    error!(
                        "Failed to upload display table before epoch {} with error: {}",
                        epoch_number,
                        e.to_string()
                    );
                })?;
            }
            // stop adding to the commit batch if we've reached the end checkpoint
            if let Some(end_checkpoint_sequence_number) = end_checkpoint_opt {
                if next_checkpoint_sequence_number > end_checkpoint_sequence_number {
                    break;
                }
            }
        }
        if !batch.is_empty() {
            commit_checkpoints(&state, batch, None, &metrics, mvr_mode).await;
            batch = vec![];
        }

        // stop the commit task if we've reached the end checkpoint
        if let Some(end_checkpoint_sequence_number) = end_checkpoint_opt {
            if next_checkpoint_sequence_number > end_checkpoint_sequence_number {
                break;
            }
        }
    }
    Ok(())
}

/// Writes indexed checkpoint data to the database, and then update watermark upper bounds and
/// metrics. Expects `indexed_checkpoint_batch` to be non-empty, and contain contiguous checkpoints.
/// There can be at most one epoch boundary at the end. If an epoch boundary is detected,
/// epoch-partitioned tables must be advanced.
// Unwrap: Caller needs to make sure indexed_checkpoint_batch is not empty
#[instrument(skip_all, fields(
    first = indexed_checkpoint_batch.first().as_ref().unwrap().checkpoint.sequence_number,
    last = indexed_checkpoint_batch.last().as_ref().unwrap().checkpoint.sequence_number
))]
async fn commit_checkpoints<S>(
    state: &S,
    indexed_checkpoint_batch: Vec<CheckpointDataToCommit>,
    epoch: Option<EpochToCommit>,
    metrics: &IndexerMetrics,
    mvr_mode: bool,
) where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    let mut checkpoint_batch = vec![];
    let mut tx_batch = vec![];
    let mut events_batch = vec![];
    let mut tx_indices_batch = vec![];
    let mut event_indices_batch = vec![];
    let mut display_updates_batch = BTreeMap::new();
    let mut object_changes_batch = vec![];
    let mut object_history_changes_batch = vec![];
    let mut object_versions_batch = vec![];
    let mut packages_batch = vec![];

    for indexed_checkpoint in indexed_checkpoint_batch {
        let CheckpointDataToCommit {
            checkpoint,
            transactions,
            events,
            event_indices,
            tx_indices,
            display_updates,
            object_changes,
            object_history_changes,
            object_versions,
            packages,
            epoch: _,
        } = indexed_checkpoint;
        // In MVR mode, persist only object_history, packages, checkpoints, and epochs
        if !mvr_mode {
            tx_batch.push(transactions);
            events_batch.push(events);
            tx_indices_batch.push(tx_indices);
            event_indices_batch.push(event_indices);
            display_updates_batch.extend(display_updates.into_iter());
            object_changes_batch.push(object_changes);
            object_versions_batch.push(object_versions);
        }
        object_history_changes_batch.push(object_history_changes);
        checkpoint_batch.push(checkpoint);
        packages_batch.push(packages);
    }

    let first_checkpoint_seq = checkpoint_batch.first().unwrap().sequence_number;
    let last_checkpoint = checkpoint_batch.last().unwrap();
    let committer_watermark = CommitterWatermark::from(last_checkpoint);

    let guard = metrics.checkpoint_db_commit_latency.start_timer();
    let tx_batch = tx_batch.into_iter().flatten().collect::<Vec<_>>();
    let tx_indices_batch = tx_indices_batch.into_iter().flatten().collect::<Vec<_>>();
    let events_batch = events_batch.into_iter().flatten().collect::<Vec<_>>();
    let event_indices_batch = event_indices_batch
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let object_versions_batch = object_versions_batch
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let packages_batch = packages_batch.into_iter().flatten().collect::<Vec<_>>();
    let checkpoint_num = checkpoint_batch.len();
    let tx_count = tx_batch.len();
    let raw_checkpoints_batch = checkpoint_batch
        .iter()
        .map(|c| c.into())
        .collect::<Vec<StoredRawCheckpoint>>();

    {
        let _step_1_guard = metrics.checkpoint_db_commit_latency_step_1.start_timer();
        let mut persist_tasks = Vec::new();
        // In MVR mode, persist only packages and object history
        persist_tasks.push(state.persist_packages(packages_batch));
        persist_tasks.push(state.persist_object_history(object_history_changes_batch.clone()));
        if !mvr_mode {
            persist_tasks.push(state.persist_transactions(tx_batch));
            persist_tasks.push(state.persist_tx_indices(tx_indices_batch));
            persist_tasks.push(state.persist_events(events_batch));
            persist_tasks.push(state.persist_event_indices(event_indices_batch));
            persist_tasks.push(state.persist_displays(display_updates_batch));
            persist_tasks.push(state.persist_objects(object_changes_batch.clone()));
            persist_tasks
                .push(state.persist_full_objects_history(object_history_changes_batch.clone()));
            persist_tasks.push(state.persist_objects_version(object_versions_batch.clone()));
            persist_tasks.push(state.persist_raw_checkpoints(raw_checkpoints_batch));
        }

        if let Some(epoch_data) = epoch.clone() {
            persist_tasks.push(state.persist_epoch(epoch_data));
        }
        futures::future::join_all(persist_tasks)
            .await
            .into_iter()
            .map(|res| {
                if res.is_err() {
                    error!("Failed to persist data with error: {:?}", res);
                }
                res
            })
            .collect::<IndexerResult<Vec<_>>>()
            .expect("Persisting data into DB should not fail.");
    }

    let is_epoch_end = epoch.is_some();

    // On epoch boundary, we need to modify the existing partitions' upper bound, and introduce a
    // new partition for incoming data for the upcoming epoch.
    if let Some(epoch_data) = epoch {
        state
            .advance_epoch(epoch_data)
            .await
            .tap_err(|e| {
                error!("Failed to advance epoch with error: {}", e.to_string());
            })
            .expect("Advancing epochs in DB should not fail.");
        metrics.total_epoch_committed.inc();
    }

    state
        .persist_checkpoints(checkpoint_batch)
        .await
        .tap_err(|e| {
            error!(
                "Failed to persist checkpoint data with error: {}",
                e.to_string()
            );
        })
        .expect("Persisting data into DB should not fail.");

    if is_epoch_end {
        // The epoch has advanced so we update the configs for the new protocol version, if it has changed.
        let chain_id = state
            .get_chain_identifier()
            .await
            .expect("Failed to get chain identifier")
            .expect("Chain identifier should have been indexed at this point");
        let _ = state
            .persist_protocol_configs_and_feature_flags(chain_id)
            .await;
    }

    state
        .update_watermarks_upper_bound::<CommitterTables>(committer_watermark)
        .await
        .tap_err(|e| {
            error!(
                "Failed to update watermark upper bound with error: {}",
                e.to_string()
            );
        })
        .expect("Updating watermark upper bound in DB should not fail.");

    let elapsed = guard.stop_and_record();

    info!(
        elapsed,
        "Checkpoint {}-{} committed with {} transactions.",
        first_checkpoint_seq,
        committer_watermark.checkpoint_hi_inclusive,
        tx_count,
    );
    metrics
        .latest_tx_checkpoint_sequence_number
        .set(committer_watermark.checkpoint_hi_inclusive as i64);
    metrics
        .total_tx_checkpoint_committed
        .inc_by(checkpoint_num as u64);
    metrics.total_transaction_committed.inc_by(tx_count as u64);
    metrics.transaction_per_checkpoint.observe(
        tx_count as f64
            / (committer_watermark.checkpoint_hi_inclusive - first_checkpoint_seq + 1) as f64,
    );
    // 1000.0 is not necessarily the batch size, it's to roughly map average tx commit latency to [0.1, 1] seconds,
    // which is well covered by DB_COMMIT_LATENCY_SEC_BUCKETS.
    metrics
        .thousand_transaction_avg_db_commit_latency
        .observe(elapsed * 1000.0 / tx_count as f64);
}
