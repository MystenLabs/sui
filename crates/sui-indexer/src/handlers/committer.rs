// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};

use tap::tap::TapFallible;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::instrument;
use tracing::{error, info};

use sui_rest_api::Client;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::metrics::IndexerMetrics;
use crate::store::IndexerStore;
use crate::types::IndexerResult;

use super::{CheckpointDataToCommit, EpochToCommit};

const CHECKPOINT_COMMIT_BATCH_SIZE: usize = 100;
const OBJECTS_SNAPSHOT_MAX_CHECKPOINT_LAG: u64 = 900;

pub async fn start_tx_checkpoint_commit_task<S>(
    state: S,
    client: Client,
    metrics: IndexerMetrics,
    tx_indexing_receiver: mysten_metrics::metered_channel::Receiver<CheckpointDataToCommit>,
    commit_notifier: watch::Sender<Option<CheckpointSequenceNumber>>,
    mut next_checkpoint_sequence_number: CheckpointSequenceNumber,
    cancel: CancellationToken,
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

    let mut object_snapshot_backfill_mode = true;
    let latest_object_snapshot_seq = state
        .get_latest_object_snapshot_checkpoint_sequence_number()
        .await?;
    let latest_cp_seq = state.get_latest_checkpoint_sequence_number().await?;
    if latest_object_snapshot_seq != latest_cp_seq {
        info!("Flipping object_snapshot_backfill_mode to false because objects_snapshot is behind already!");
        object_snapshot_backfill_mode = false;
    }

    let mut unprocessed = HashMap::new();
    let mut batch = vec![];

    while let Some(indexed_checkpoint_batch) = stream.next().await {
        if cancel.is_cancelled() {
            break;
        }

        let mut latest_fn_cp_res = client.get_latest_checkpoint().await;
        while latest_fn_cp_res.is_err() {
            error!("Failed to get latest checkpoint from the network");
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            latest_fn_cp_res = client.get_latest_checkpoint().await;
        }
        // unwrap is safe here because we checked that latest_fn_cp_res is Ok above
        let latest_fn_cp = latest_fn_cp_res.unwrap().sequence_number;
        // unwrap is safe b/c we checked for empty batch above
        let latest_committed_cp = indexed_checkpoint_batch
            .last()
            .unwrap()
            .checkpoint
            .sequence_number;

        // split the batch into smaller batches per epoch to handle partitioning
        for checkpoint in indexed_checkpoint_batch {
            unprocessed.insert(checkpoint.checkpoint.sequence_number, checkpoint);
        }
        while let Some(checkpoint) = unprocessed.remove(&next_checkpoint_sequence_number) {
            let epoch = checkpoint.epoch.clone();
            batch.push(checkpoint);
            next_checkpoint_sequence_number += 1;
            if batch.len() == checkpoint_commit_batch_size || epoch.is_some() {
                commit_checkpoints(
                    &state,
                    batch,
                    epoch,
                    &metrics,
                    &commit_notifier,
                    object_snapshot_backfill_mode,
                )
                .await;
                batch = vec![];
            }
        }
        if !batch.is_empty() && unprocessed.is_empty() {
            commit_checkpoints(
                &state,
                batch,
                None,
                &metrics,
                &commit_notifier,
                object_snapshot_backfill_mode,
            )
            .await;
            batch = vec![];
        }
        // this is a one-way flip in case indexer falls behind again, so that the objects snapshot
        // table will not be populated by both committer and async snapshot processor at the same time.
        if latest_committed_cp + OBJECTS_SNAPSHOT_MAX_CHECKPOINT_LAG > latest_fn_cp {
            info!("Flipping object_snapshot_backfill_mode to false because objects_snapshot is close to up-to-date.");
            object_snapshot_backfill_mode = false;
        }
    }
    Ok(())
}

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
    commit_notifier: &watch::Sender<Option<CheckpointSequenceNumber>>,
    object_snapshot_backfill_mode: bool,
) where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    let mut checkpoint_batch = vec![];
    let mut tx_batch = vec![];
    let mut events_batch = vec![];
    let mut tx_indices_batch = vec![];
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
            tx_indices,
            display_updates,
            object_changes,
            object_history_changes,
            object_versions,
            packages,
            epoch: _,
        } = indexed_checkpoint;
        checkpoint_batch.push(checkpoint);
        tx_batch.push(transactions);
        events_batch.push(events);
        tx_indices_batch.push(tx_indices);
        display_updates_batch.extend(display_updates.into_iter());
        object_changes_batch.push(object_changes);
        object_history_changes_batch.push(object_history_changes);
        object_versions_batch.extend(object_versions);
        packages_batch.push(packages);
    }

    let first_checkpoint_seq = checkpoint_batch.first().as_ref().unwrap().sequence_number;
    let last_checkpoint_seq = checkpoint_batch.last().as_ref().unwrap().sequence_number;

    let guard = metrics.checkpoint_db_commit_latency.start_timer();
    let tx_batch = tx_batch.into_iter().flatten().collect::<Vec<_>>();
    let tx_indices_batch = tx_indices_batch.into_iter().flatten().collect::<Vec<_>>();
    let events_batch = events_batch.into_iter().flatten().collect::<Vec<_>>();
    let packages_batch = packages_batch.into_iter().flatten().collect::<Vec<_>>();
    let checkpoint_num = checkpoint_batch.len();
    let tx_count = tx_batch.len();

    {
        let _step_1_guard = metrics.checkpoint_db_commit_latency_step_1.start_timer();
        let mut persist_tasks = vec![
            state.persist_transactions(tx_batch),
            state.persist_tx_indices(tx_indices_batch),
            state.persist_events(events_batch),
            state.persist_displays(display_updates_batch),
            state.persist_packages(packages_batch),
            state.persist_objects(object_changes_batch.clone()),
            state.persist_object_history(object_history_changes_batch.clone()),
            state.persist_objects_version(object_versions_batch.clone()),
        ];
        if object_snapshot_backfill_mode {
            persist_tasks.push(state.backfill_objects_snapshot(object_changes_batch));
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

    // handle partitioning on epoch boundary
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
    let elapsed = guard.stop_and_record();

    commit_notifier
        .send(Some(last_checkpoint_seq))
        .expect("Commit watcher should not be closed");

    info!(
        elapsed,
        "Checkpoint {}-{} committed with {} transactions.",
        first_checkpoint_seq,
        last_checkpoint_seq,
        tx_count,
    );
    metrics
        .latest_tx_checkpoint_sequence_number
        .set(last_checkpoint_seq as i64);
    metrics
        .total_tx_checkpoint_committed
        .inc_by(checkpoint_num as u64);
    metrics.total_transaction_committed.inc_by(tx_count as u64);
    if object_snapshot_backfill_mode {
        metrics
            .latest_object_snapshot_sequence_number
            .set(last_checkpoint_seq as i64);
    }
    metrics
        .transaction_per_checkpoint
        .observe(tx_count as f64 / (last_checkpoint_seq - first_checkpoint_seq + 1) as f64);
    // 1000.0 is not necessarily the batch size, it's to roughly map average tx commit latency to [0.1, 1] seconds,
    // which is well covered by DB_COMMIT_LATENCY_SEC_BUCKETS.
    metrics
        .thousand_transaction_avg_db_commit_latency
        .observe(elapsed * 1000.0 / tx_count as f64);
}
