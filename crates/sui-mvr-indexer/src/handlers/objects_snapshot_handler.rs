// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use mysten_metrics::get_metrics;
use mysten_metrics::metered_channel::Sender;
use mysten_metrics::spawn_monitored_task;
use sui_data_ingestion_core::Worker;
use sui_rpc_api::CheckpointData;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::config::SnapshotLagConfig;
use crate::store::PgIndexerStore;
use crate::types::IndexerResult;
use crate::{metrics::IndexerMetrics, store::IndexerStore};

use super::checkpoint_handler::CheckpointHandler;
use super::{CommitterWatermark, ObjectsSnapshotHandlerTables, TransactionObjectChangesToCommit};
use super::{CommonHandler, Handler};

#[derive(Clone)]
pub struct ObjectsSnapshotHandler {
    pub store: PgIndexerStore,
    pub sender: Sender<(CommitterWatermark, TransactionObjectChangesToCommit)>,
    snapshot_config: SnapshotLagConfig,
    metrics: IndexerMetrics,
}

pub struct CheckpointObjectChanges {
    pub checkpoint_sequence_number: u64,
    pub object_changes: TransactionObjectChangesToCommit,
}

#[async_trait]
impl Worker for ObjectsSnapshotHandler {
    type Result = ();
    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> anyhow::Result<()> {
        let transformed_data = CheckpointHandler::index_objects(checkpoint, &self.metrics).await?;
        self.sender
            .send((CommitterWatermark::from(checkpoint), transformed_data))
            .await?;
        Ok(())
    }
}

#[async_trait]
impl Handler<TransactionObjectChangesToCommit> for ObjectsSnapshotHandler {
    fn name(&self) -> String {
        "objects_snapshot_handler".to_string()
    }

    async fn load(
        &self,
        transformed_data: Vec<TransactionObjectChangesToCommit>,
    ) -> IndexerResult<()> {
        self.store
            .persist_objects_snapshot(transformed_data)
            .await?;
        Ok(())
    }

    async fn get_watermark_hi(&self) -> IndexerResult<Option<u64>> {
        self.store
            .get_latest_object_snapshot_checkpoint_sequence_number()
            .await
    }

    async fn set_watermark_hi(&self, watermark: CommitterWatermark) -> IndexerResult<()> {
        self.store
            .update_watermarks_upper_bound::<ObjectsSnapshotHandlerTables>(watermark)
            .await?;

        self.metrics
            .latest_object_snapshot_sequence_number
            .set(watermark.checkpoint_hi_inclusive as i64);
        Ok(())
    }

    async fn get_max_committable_checkpoint(&self) -> IndexerResult<u64> {
        let latest_checkpoint = self.store.get_latest_checkpoint_sequence_number().await?;
        Ok(latest_checkpoint
            .map(|seq| seq.saturating_sub(self.snapshot_config.snapshot_min_lag as u64))
            .unwrap_or_default()) // hold snapshot handler until at least one checkpoint is in DB
    }
}

pub async fn start_objects_snapshot_handler(
    store: PgIndexerStore,
    metrics: IndexerMetrics,
    snapshot_config: SnapshotLagConfig,
    cancel: CancellationToken,
    start_checkpoint_opt: Option<u64>,
    end_checkpoint_opt: Option<u64>,
) -> IndexerResult<(ObjectsSnapshotHandler, u64)> {
    info!("Starting object snapshot handler...");

    let global_metrics = get_metrics().unwrap();
    let (sender, receiver) = mysten_metrics::metered_channel::channel(
        600,
        &global_metrics
            .channel_inflight
            .with_label_values(&["objects_snapshot_handler_checkpoint_data"]),
    );

    let objects_snapshot_handler =
        ObjectsSnapshotHandler::new(store.clone(), sender, metrics.clone(), snapshot_config);

    let next_cp_from_db = objects_snapshot_handler
        .get_watermark_hi()
        .await?
        .map(|cp| cp.saturating_add(1))
        .unwrap_or_default();
    let start_checkpoint = start_checkpoint_opt.unwrap_or(next_cp_from_db);
    let common_handler = CommonHandler::new(Box::new(objects_snapshot_handler.clone()));
    spawn_monitored_task!(common_handler.start_transform_and_load(
        receiver,
        cancel,
        start_checkpoint,
        end_checkpoint_opt,
    ));
    Ok((objects_snapshot_handler, start_checkpoint))
}

impl ObjectsSnapshotHandler {
    pub fn new(
        store: PgIndexerStore,
        sender: Sender<(CommitterWatermark, TransactionObjectChangesToCommit)>,
        metrics: IndexerMetrics,
        snapshot_config: SnapshotLagConfig,
    ) -> ObjectsSnapshotHandler {
        Self {
            store,
            sender,
            metrics,
            snapshot_config,
        }
    }
}
