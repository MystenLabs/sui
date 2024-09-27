// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use mysten_metrics::get_metrics;
use mysten_metrics::metered_channel::Sender;
use mysten_metrics::spawn_monitored_task;
use sui_data_ingestion_core::Worker;
use sui_rest_api::CheckpointData;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::config::SnapshotLagConfig;
use crate::store::PgIndexerStore;
use crate::types::IndexerResult;
use crate::{metrics::IndexerMetrics, store::IndexerStore};

use super::checkpoint_handler::CheckpointHandler;
use super::Handler;
use super::TransactionObjectChangesToCommit;

#[derive(Clone)]
pub struct ObjectsSnapshotHandler {
    pub store: PgIndexerStore,
    pub cp_sender: Sender<CheckpointData>,
    snapshot_config: SnapshotLagConfig,
    metrics: IndexerMetrics,
}

pub struct CheckpointObjectChanges {
    pub checkpoint_sequence_number: u64,
    pub object_changes: TransactionObjectChangesToCommit,
}

#[async_trait]
impl Worker for ObjectsSnapshotHandler {
    // ?? change Worker trait to use Arc<CheckpointData> to avoid clone
    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> anyhow::Result<()> {
        self.cp_sender.send(checkpoint.clone()).await?;
        Ok(())
    }
}

#[async_trait]
impl Handler<TransactionObjectChangesToCommit> for ObjectsSnapshotHandler {
    fn name(&self) -> String {
        "objects_snapshot_handler".to_string()
    }

    async fn transform(
        &self,
        cp_batch: Vec<CheckpointData>,
    ) -> IndexerResult<Vec<TransactionObjectChangesToCommit>> {
        futures::future::join_all(cp_batch.into_iter().map(|checkpoint| {
            let metrics = self.metrics.clone();
            async move { CheckpointHandler::index_objects(&checkpoint, &metrics).await }
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<TransactionObjectChangesToCommit>, _>>()
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

    // TODO: read watermark table when it's ready.
    async fn get_watermark_hi(&self) -> IndexerResult<Option<u64>> {
        self.store
            .get_latest_object_snapshot_checkpoint_sequence_number()
            .await
    }

    // TODO: update watermark table when it's ready.
    async fn set_watermark_hi(&self, watermark_hi: u64) -> IndexerResult<()> {
        self.metrics
            .latest_object_snapshot_sequence_number
            .set(watermark_hi as i64);
        Ok(())
    }

    async fn get_checkpoint_lag_limiter(&self) -> IndexerResult<u64> {
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
) -> IndexerResult<(ObjectsSnapshotHandler, u64)> {
    info!("Starting object snapshot handler...");

    let global_metrics = get_metrics().unwrap();
    let (cp_sender, cp_receiver) = mysten_metrics::metered_channel::channel(
        600,
        &global_metrics
            .channel_inflight
            .with_label_values(&["objects_snapshot_handler_checkpoint_data"]),
    );

    let objects_snapshot_handler =
        ObjectsSnapshotHandler::new(store.clone(), cp_sender, metrics.clone(), snapshot_config);

    let watermark_hi = objects_snapshot_handler.get_watermark_hi().await?;
    let handler_clone = objects_snapshot_handler.clone();
    spawn_monitored_task!(handler_clone.start_transform_and_load(cp_receiver, cancel,));
    Ok((objects_snapshot_handler, watermark_hi.unwrap_or_default()))
}

impl ObjectsSnapshotHandler {
    pub fn new(
        store: PgIndexerStore,
        cp_sender: Sender<CheckpointData>,
        metrics: IndexerMetrics,
        snapshot_config: SnapshotLagConfig,
    ) -> ObjectsSnapshotHandler {
        Self {
            store,
            cp_sender,
            metrics,
            snapshot_config,
        }
    }
}
