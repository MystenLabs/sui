// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tracing::info;

use crate::types_v2::IndexerResult;
use crate::{metrics::IndexerMetrics, store::IndexerStoreV2};

const OBJECTS_SNAPSHOT_MAX_CHECKPOINT_LAG: usize = 900;
const OBJECTS_SNAPSHOT_MIN_CHECKPOINT_LAG: usize = 300;

pub struct ObjectsSnapshotProcessor<S> {
    pub store: S,
    metrics: IndexerMetrics,
    pub snapshot_min_lag: usize,
    pub snapshot_max_lag: usize,
}

impl<S> ObjectsSnapshotProcessor<S>
where
    S: IndexerStoreV2 + Clone + Sync + Send + 'static,
{
    pub fn new(store: S, metrics: IndexerMetrics) -> ObjectsSnapshotProcessor<S> {
        let snapshot_min_lag = std::env::var("OBJECTS_SNAPSHOT_MIN_CHECKPOINT_LAG")
            .map(|s| {
                s.parse::<usize>()
                    .unwrap_or(OBJECTS_SNAPSHOT_MIN_CHECKPOINT_LAG)
            })
            .unwrap_or(0);
        let snapshot_max_lag = std::env::var("OBJECTS_SNAPSHOT_MAX_CHECKPOINT_LAG")
            .map(|s| {
                s.parse::<usize>()
                    .unwrap_or(OBJECTS_SNAPSHOT_MAX_CHECKPOINT_LAG)
            })
            .unwrap_or(0);
        Self {
            store,
            metrics,
            snapshot_min_lag,
            snapshot_max_lag,
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
        // make sure cp 0 is handled
        let mut start_cp = if latest_snapshot_cp == 0 {
            0
        } else {
            latest_snapshot_cp + 1
        };
        // with MAX and MIN, the CSR range will vary from MIN cps to MAX cps
        let snapshot_window = self.snapshot_max_lag as u64 - self.snapshot_min_lag as u64;
        let mut latest_cp = self
            .store
            .get_latest_tx_checkpoint_sequence_number()
            .await?
            .unwrap_or_default();

        loop {
            while latest_cp <= start_cp + self.snapshot_max_lag as u64 {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                latest_cp = self
                    .store
                    .get_latest_tx_checkpoint_sequence_number()
                    .await?
                    .unwrap_or_default();
            }
            self.store
                .persist_object_snapshot(start_cp, start_cp + snapshot_window)
                .await?;
            start_cp += snapshot_window;
            self.metrics
                .latest_object_snapshot_sequence_number
                .set(start_cp as i64);
        }
    }
}
