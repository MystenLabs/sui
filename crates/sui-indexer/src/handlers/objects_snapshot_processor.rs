// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use sui_rest_api::Client;

use crate::types::IndexerResult;
use crate::{metrics::IndexerMetrics, store::IndexerStore};
use crate::handlers::TransactionObjectChangesToCommit;
use crate::store::ObjectChangeToCommit;

const OBJECTS_SNAPSHOT_MAX_CHECKPOINT_LAG: usize = 900;
const OBJECTS_SNAPSHOT_MIN_CHECKPOINT_LAG: usize = 300;
// The number of object changes in the buffer before the snapshot processor starts processing them.
// TODO: placeholder value, need to tune this
const OBJECTS_SNAPSHOT_BUFFER_THRESHOLD: usize = 50_000;

pub struct ObjectsSnapshotProcessor<S> {
    pub client: Client,
    pub store: S,
    metrics: IndexerMetrics,
    pub config: SnapshotLagConfig,
    // The buffer is consumed by the snapshot processor and is filled by the committer.
    buffer: Arc<Mutex<ObjectChangeBuffer>>,
    cancel: CancellationToken,
}

#[derive(Default)]
pub struct ObjectChangeBuffer {
    pub startup_checkpoint: Option<u64>,
    pub buffer: Vec<TransactionObjectChangesToCommit>,
}

impl ObjectChangeBuffer {
    pub fn num_object_changes(&self) -> usize {
        self.buffer.iter().map(|v| v.changed_objects.len() + v.deleted_objects.len()).sum()
    }
}

#[derive(Clone)]
pub struct SnapshotLagConfig {
    pub snapshot_min_lag: usize,
    pub snapshot_max_lag: usize,
    pub sleep_duration: u64,
    // TODO: maybe have a different sleep duration for buffer mode?
}

impl SnapshotLagConfig {
    pub fn new(
        min_lag: Option<usize>,
        max_lag: Option<usize>,
        sleep_duration: Option<u64>,
    ) -> Self {
        let default_config = Self::default();
        Self {
            snapshot_min_lag: min_lag.unwrap_or(default_config.snapshot_min_lag),
            snapshot_max_lag: max_lag.unwrap_or(default_config.snapshot_max_lag),
            sleep_duration: sleep_duration.unwrap_or(default_config.sleep_duration),
        }
    }
}

impl Default for SnapshotLagConfig {
    fn default() -> Self {
        let snapshot_min_lag = std::env::var("OBJECTS_SNAPSHOT_MIN_CHECKPOINT_LAG")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(OBJECTS_SNAPSHOT_MIN_CHECKPOINT_LAG);

        let snapshot_max_lag = std::env::var("OBJECTS_SNAPSHOT_MAX_CHECKPOINT_LAG")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(OBJECTS_SNAPSHOT_MAX_CHECKPOINT_LAG);

        SnapshotLagConfig {
            snapshot_min_lag,
            snapshot_max_lag,
            sleep_duration: 5,
        }
    }
}

// NOTE: "handler"
impl<S> ObjectsSnapshotProcessor<S>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    pub fn new_with_config(
        client: Client,
        store: S,
        buffer: Arc<Mutex<ObjectChangeBuffer>>,
        metrics: IndexerMetrics,
        config: SnapshotLagConfig,
        cancel: CancellationToken,
    ) -> ObjectsSnapshotProcessor<S> {
        Self {
            client,
            store,
            buffer,
            metrics,
            config,
            cancel,
        }
    }

    // The `objects_snapshot` table maintains a delayed snapshot of the `objects` table,
    // controlled by `object_snapshot_max_checkpoint_lag` (max lag) and
    // `object_snapshot_min_checkpoint_lag` (min lag). For instance, with a max lag of 900
    // and a min lag of 300 checkpoints, the `objects_snapshot` table will lag behind the
    // `objects` table by 300 to 900 checkpoints. The snapshot is updated when the lag
    // exceeds the max lag threshold, and updates continue until the lag is reduced to
    // the min lag threshold. Then, we have a consistent read range between
    // `latest_snapshot_cp` and `latest_cp` based on `objects_snapshot` and `objects_history`,
    // where the size of this range varies between the min and max lag values.
    pub async fn start(&self) -> IndexerResult<()> {
        info!("Starting object snapshot processor...");
        let latest_snapshot_cp = self
            .store
            .get_latest_object_snapshot_checkpoint_sequence_number()
            .await?
            .unwrap_or_default();
        let latest_indexer_cp = self
            .store
            .get_latest_checkpoint_sequence_number()
            .await?
            .unwrap_or_default();
        // make sure cp 0 is handled
        let mut start_cp = if latest_snapshot_cp == 0 {
            0
        } else {
            latest_snapshot_cp + 1
        };
        // with MAX and MIN, the CSR range will vary from MIN cps to MAX cps
        let snapshot_window =
            self.config.snapshot_max_lag as u64 - self.config.snapshot_min_lag as u64;
        let mut latest_fn_cp = self.client.get_latest_checkpoint().await?.sequence_number;

        // While the below is true, we are in backfill mode, and so `ObjectsSnapshotProcessor` will
        // no-op. Once we exit the loop, this task will then be responsible for updating the
        // `objects_snapshot` table.
        if latest_snapshot_cp == latest_indexer_cp {
            while latest_fn_cp > start_cp + self.config.snapshot_max_lag as u64 {
                tokio::select! {
                    _ = self.cancel.cancelled() => {
                        info!("Shutdown signal received, terminating object snapshot processor");
                        return Ok(());
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(self.config.sleep_duration)) => {
                        info!("Objects snapshot is in backfill mode, objects snapshot processor is sleeping for {} seconds", self.config.sleep_duration);
                        latest_fn_cp = self.client.get_latest_checkpoint().await?.sequence_number;
                        start_cp = self
                            .store
                            .get_latest_object_snapshot_checkpoint_sequence_number()
                            .await?
                            .unwrap_or_default();
                    }
                }
            }
        }

        // We are not in backfill mode but it's possible that the snapshot checkpoint is behind the
        // indexer, so we first update the objects snapshot table using objects history until it has
        // caught up to the checkpoint we have in the in-memory buffer. We then switch to using the
        // buffer to update objects snapshot.
        let mut buffer_cp = self.buffer.lock().unwrap().startup_checkpoint;

        // Use objects_history to update objects_snapshot table until the buffer has contents and
        // the snapshot checkpoint has caught up to the first checkpoint in the buffer.
        while (buffer_cp.is_none() || buffer_cp.unwrap() >= start_cp) {
            tokio::select! {
                _ = self.cancel.cancelled() => {
                    info!("Shutdown signal received, terminating object snapshot processor");
                    return Ok(());
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(self.config.sleep_duration)) => {

                    let latest_cp = self
                        .store
                        .get_latest_checkpoint_sequence_number()
                        .await?
                        .unwrap_or_default();

                    if latest_cp > start_cp + self.config.snapshot_max_lag as u64 {
                        info!("Objects snapshot processor is updating objects snapshot table from {} to {}", start_cp, start_cp + snapshot_window);
                        self.store
                            .update_objects_snapshot(start_cp, start_cp + snapshot_window)
                            .await?;
                        start_cp += snapshot_window;
                        self.metrics
                            .latest_object_snapshot_sequence_number
                            .set(start_cp as i64);
                    }

                    buffer_cp = self.buffer.lock().unwrap().startup_checkpoint;
                }
            }
        }

        // Now the cp in objects snapshot is greater than what's in the buffer, so we can start flushing
        // the buffer to objects snapshot.
        info!("Objects snapshot processor starts updating objects_snapshot periodically from the buffer...");
        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => {
                    info!("Shutdown signal received, terminating object snapshot processor");
                    return Ok(());
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(self.config.sleep_duration)) => {
                    let mut buffer_size = self.buffer.lock().unwrap().num_object_changes();
                    if buffer_size > OBJECTS_SNAPSHOT_BUFFER_THRESHOLD {
                        // flush everything in the buffer
                        // TODO: what if there's too many things in the buffer? Maybe it's better to flush in batches
                        let object_changes = self.buffer.lock().unwrap().buffer.drain(..).collect();
                        info!("Objects snapshot processor is committing {} object changes to objects_snapshot table", buffer_size);
                        // TODO: change the name of this function to `persist_objects_snapshot` since
                        // it's used beyond backfill mode
                        self.store.backfill_objects_snapshot(object_changes).await?;
                    }
                }
            }
        }


    }
}
