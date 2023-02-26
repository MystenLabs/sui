// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod snapshot;
#[cfg(test)]
mod tests;
mod util;

use crate::snapshot::{StateSnapshotReader, StateSnapshotReaderV1, StateSnapshotWriterV1};
use anyhow::{anyhow, Context, Result};
use fs_extra::dir::get_size;
use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntGauge, Registry,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::Bound::{Included, Unbounded};
use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fs, mem};
use std::sync::Arc;
use sui_core::authority::authority_store_tables::{AuthorityPerpetualTables, CURRENT_EPOCH_KEY};
use tokio::sync::oneshot::Sender;
use tokio::time::Instant;
use tracing::{debug, info};
use tracing::log::error;
use sui_core::checkpoints::CheckpointStore;
use sui_types::base_types::{EpochId, ObjectID};
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use sui_types::SUI_SYSTEM_STATE_OBJECT_ID;
use typed_store::rocks::safe_drop_db;
use crate::util::{get_db_checkpoints_by_epoch, get_snapshots_by_epoch};

const MAX_OPS_IN_ONE_WRITE_BATCH: u64 = 10000;

#[derive(Debug)]
pub struct StateSnapshotMetrics {
    pub snapshot_success: IntCounter,
    pub snapshot_size_in_bytes: IntGauge,
    pub num_objects_in_snapshot: IntGauge,
    pub snapshot_latency_secs: Histogram,
}

impl StateSnapshotMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            snapshot_success: register_int_counter_with_registry!(
                "snapshot_success",
                "If snapshot was successfully taken, increment the counter",
                registry,
            )
            .unwrap(),
            snapshot_size_in_bytes: register_int_gauge_with_registry!(
                "snapshot_size_in_bytes",
                "Size of the sui snapshot in bytes",
                registry,
            )
            .unwrap(),
            num_objects_in_snapshot: register_int_gauge_with_registry!(
                "num_objects_in_snapshot",
                "Number of objects in snapshot",
                registry,
            )
                .unwrap(),
            snapshot_latency_secs: register_histogram_with_registry!(
                "snapshot_latency_secs",
                "Latency of taking a state snapshot",
                registry,
            )
            .unwrap(),
        }
    }
}

#[derive(Debug)]
pub struct StateSnapshotRecoveryMetrics {
    pub snapshot_recovery_success: IntCounter,
    pub snapshot_recovery_latency_secs: Histogram,
}

impl StateSnapshotRecoveryMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            snapshot_recovery_success: register_int_counter_with_registry!(
                "snapshot_recovery_success",
                "If snapshot recovery was successfully taken, increment the counter",
                registry,
            )
                .unwrap(),
            snapshot_recovery_latency_secs: register_histogram_with_registry!(
                "snapshot_recovery_latency_secs",
                "Latency of recovering from a state snapshot",
                registry,
            )
                .unwrap(),
        }
    }
}


struct StateSnapshotCreateLoop {
    snapshot_path: PathBuf,
    db_path: PathBuf,
    interval_period_s: Duration,
    num_latest_snapshots_to_keep: u32,
    metrics: StateSnapshotMetrics,
}

impl StateSnapshotCreateLoop {
    pub fn new(config: StateSnapshotConfig, db_path: &Path, registry: &Registry) -> Result<Self> {
        let metrics = StateSnapshotMetrics::new(registry);
        let snapshot_path = config
            .snapshot_path
            .unwrap_or(PathBuf::from("/opt/sui/snapshot"));
        if !snapshot_path.exists() {
            return Err(anyhow!(
                "Snapshot output path does not exist: {:?}",
                snapshot_path.display()
            ));
        }
        let metadata = fs::metadata(&snapshot_path)?;
        if !metadata.is_dir() {
            return Err(anyhow!(
                "Snapshot output path is not a dir: {:?}",
                snapshot_path.display()
            ));
        }
        if metadata.permissions().readonly() {
            return Err(anyhow!(
                "No write permission on snapshot dir: {:?}",
                snapshot_path.display()
            ));
        }
        Ok(StateSnapshotCreateLoop {
            snapshot_path,
            db_path: db_path.to_path_buf(),
            interval_period_s: Duration::from_secs(config.interval_period_s.unwrap_or(300)),
            num_latest_snapshots_to_keep: config.num_latest_snapshots_to_keep.unwrap_or(1),
            metrics,
        })
    }
    pub async fn start(mut self) -> Sender<()> {
        info!("State snapshot create loop started");
        let (sender, mut recv) = tokio::sync::oneshot::channel();
        let mut interval = tokio::time::interval(self.interval_period_s);
        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    now = interval.tick() => {
                        if let Err(e) = self.handle_snapshot_tick().await {
                            error!("Failed to take state snapshot with error: {:?}", e);
                        } else {
                            info!("Finished taking state snapshot");
                            self.metrics.snapshot_latency_secs.observe(now.elapsed().as_secs() as f64);
                        }
                    },
                    _ = &mut recv => break,
                }
            }
        });
        sender
    }
    async fn handle_snapshot_tick(&mut self) -> Result<()> {
        let checkpoints_by_epoch = get_db_checkpoints_by_epoch(&self.db_path)?;
        if checkpoints_by_epoch.is_empty() {
            return Ok(());
        }
        let snapshots_by_epoch = get_snapshots_by_epoch(&self.snapshot_path)?;
        let next_epoch_to_snapshot = snapshots_by_epoch
            .last_key_value()
            .map(|(k, _)| k + 1)
            .unwrap_or(u32::MIN);
        let next_checkpointed_epoch_db = checkpoints_by_epoch
            .range((Included(next_epoch_to_snapshot), Unbounded))
            .next();
        if next_checkpointed_epoch_db.is_none() {
            return Ok(());
        }
        // Unwrap is safe because we checked for is_none() above
        let (epoch_to_snapshot, checkpointed_db_path) = next_checkpointed_epoch_db.unwrap();
        let perpetual_db = AuthorityPerpetualTables::open(checkpointed_db_path, None);
        let snapshot_tmp_path = self
            .snapshot_path
            .join(format!("tmp-epoch-{}", *epoch_to_snapshot));
        let snapshot_path = self
            .snapshot_path
            .join(format!("epoch-{}", *epoch_to_snapshot));
        let snapshot_writer = StateSnapshotWriterV1::new(&snapshot_tmp_path)?;
        let epoch_last_checkpoint_seq_number: u64 = fs::read_to_string(checkpointed_db_path.join("checkpoint"))?.parse()?;
        info!("Starting taking state snapshot for epoch: {}, checkpoint: {}", *epoch_to_snapshot, epoch_last_checkpoint_seq_number);
        snapshot_writer.write_objects(perpetual_db.iter_live_object_set(), &perpetual_db, epoch_last_checkpoint_seq_number)?;
        fs::rename(snapshot_tmp_path, &snapshot_path)?;
        info!(
            "Successfully completed state snapshot of epoch: {} at location: {}",
            *epoch_to_snapshot,
            snapshot_path.display()
        );
        mem::drop(perpetual_db);
        // Give some time to cleanup lock file in the checkpointed db directory
        tokio::time::sleep(Duration::from_secs(1)).await;
        self.metrics.snapshot_success.inc();
        if let Ok(size) = get_size(snapshot_path) {
            self.metrics.snapshot_size_in_bytes.set(size as i64);
        }
        self.garbage_collect_old_db_checkpoints(*epoch_to_snapshot)?;
        self.garbage_collect_old_snapshots()?;
        Ok(())
    }
    fn garbage_collect_old_db_checkpoints(&mut self, max_epoch_to_delete: u32) -> Result<()> {
        let checkpoints_by_epoch = get_db_checkpoints_by_epoch(&self.db_path)?;
        for (epoch, path) in checkpoints_by_epoch.iter().rev() {
            if *epoch <= max_epoch_to_delete {
                info!("Dropping checkpointed db for epoch: {} at path: {}", *epoch, path.display());
                safe_drop_db(path.clone())?;
            }
        }
        Ok(())
    }
    fn garbage_collect_old_snapshots(&mut self) -> Result<()> {
        let snapshots_by_epoch = get_snapshots_by_epoch(&self.snapshot_path)?;
        let mut counter = 0;
        for (_, path) in snapshots_by_epoch.iter().rev() {
            if counter >= self.num_latest_snapshots_to_keep {
                info!(
                    "Garbage collecting old snapshot directory: {}",
                    path.display()
                );
                fs::remove_dir_all(path)?;
            } else {
                counter += 1;
            }
        }
        Ok(())
    }
}


pub struct StateSnapshotRecoveryLoop {
    snapshot_reader: StateSnapshotReader,
    interval_period_s: Duration,
    timeout: Duration,
    metrics: StateSnapshotRecoveryMetrics,
}

impl StateSnapshotRecoveryLoop {
    pub fn new(config: StateSnapshotRecoveryConfig, highest_executed_local_checkpoint: u64, registry: &Registry) -> Result<Self> {
        let path = config.snapshot_path.unwrap_or(PathBuf::from("/opt/sui/snapshot"));
        let snapshots_by_epoch = get_snapshots_by_epoch(&path)?;
        let (epoch, snapshot_path) = snapshots_by_epoch.last_key_value().ok_or(anyhow!("No snapshot to recover from in snapshot dir: {}", path.display()))?;
        info!("Latest snapshot available to recover from is from epoch: {epoch} ");
        let snapshot_reader = StateSnapshotReader::new(&snapshot_path)?;
        if epoch != snapshot_reader.epoch() {
            return Err(anyhow!("Snapshot data in epoch dir for epoch: {epoch} is for a different epoch: {}", snapshot_reader.epoch()));
        }
        let snapshot_checkpoint_seq_number = snapshot_reader.checkpoint_seq_number();
        if snapshot_checkpoint_seq_number <= highest_executed_local_checkpoint {
            return Err(anyhow!("Latest snapshot with checkpoint: {snapshot_checkpoint_seq_number} is behind current highest executed checkpoint: {highest_executed_checkpoint}"));
        }
        Ok(StateSnapshotRecoveryLoop {
            snapshot_reader,
            interval_period_s: Duration::from_secs(config.interval_period_s.unwrap_or(10)),
            timeout: Duration::from_secs(config.interval_period_s.unwrap_or(60 * 60)),
            metrics: StateSnapshotRecoveryMetrics::new(registry),
        })
    }
    pub async fn start(mut self, perpetual_db: &AuthorityPerpetualTables, checkpoint_store: &Arc<CheckpointStore>) -> Result<()> {
        info!("State snapshot creator loop started with snapshot recovery epoch: {}, checkpoint: {}", self.snapshot_reader.epoch(), self.snapshot_reader.checkpoint_seq_number());
        let mut interval = tokio::time::interval(self.interval_period_s);
        let start_ts = Instant::now();
        let timeout = self.timeout;
        loop {
            tokio::select! {
                now = interval.tick() => {
                    if self.handle_recovery_tick(perpetual_db, checkpoint_store)? {
                        self.metrics.snapshot_recovery_success.inc();
                        self.metrics.snapshot_recovery_latency_secs.observe(now.elapsed().as_secs() as f64);
                        return Ok(());
                    }
                    let wait_so_far = Instant::now().duration_since(start_ts);
                    if wait_so_far > timeout {
                        info!("Snapshot recovery timing out after : {:?}", wait_so_far);
                        return Err(anyhow!("Timed out snapshot recovery waiting for checkpoints to catch up"));
                    }
                }
            }
        }
    }
    fn handle_recovery_tick(&mut self, perpetual_db: &AuthorityPerpetualTables, checkpoint_store: &CheckpointStore) -> Result<bool> {
        let highest_verified_checkpoint = checkpoint_store.get_highest_verified_checkpoint()?.ok_or(anyhow!("No verified checkpoint in the store"))?;
        if highest_verified_checkpoint.summary.sequence_number < self.snapshot_reader.checkpoint_seq_number() {
            info!("Highest verified checkpoint is: {}, waiting for: {}", highest_verified_checkpoint.summary.sequence_number, self.snapshot_reader.checkpoint_seq_number());
            return Ok(false);
        }
        let snapshot_checkpoint = checkpoint_store.get_checkpoint_by_sequence_number(self.snapshot_reader.checkpoint_seq_number())?.ok_or(anyhow!("Checkpoint missing in the store"))?;
        let _end_of_epoch_data = snapshot_checkpoint.summary.end_of_epoch_data.clone().ok_or(anyhow!("Snapshot checkpoint: {} is not for the end of epoch", self.snapshot_reader.checkpoint_seq_number()))?;
        // TODO: Verify root state hash in checkpoint matches the one computed from references in snapshot
        info!("Starting snapshot recovery from snapshot epoch: {}", self.snapshot_reader.epoch());
        match &mut self.snapshot_reader {
            StateSnapshotReader::StateSnapshotReaderV1(reader) => Self::handle_snapshot_version_1_recovery(reader, perpetual_db, checkpoint_store, snapshot_checkpoint)?,
        }
        info!("Snapshot recovery complete for snapshot epoch: {}", self.snapshot_reader.epoch());
        Ok(true)
    }
    fn handle_snapshot_version_1_recovery(reader: &mut StateSnapshotReaderV1, perpetual_db: &AuthorityPerpetualTables, checkpoint_store: &CheckpointStore, snapshot_checkpoint: VerifiedCheckpoint) -> Result<()> {
        let reader_buckets = reader.buckets()?;
        let mut wb = perpetual_db.objects.batch();
        let mut num_pending_wb_ops = 0;
        for bucket in reader_buckets.iter() {
            let object_iter = reader.safe_obj_iter(*bucket)?;
            for (object, _) in object_iter {
                let object_key = ObjectKey(object.id(), object.version());
                wb = wb.insert_batch(&perpetual_db.parent_sync,  std::iter::once((object.compute_object_reference(), object.previous_transaction.clone())))?;
                wb = wb.insert_batch(&perpetual_db.objects,  std::iter::once((object_key, object)))?;
                num_pending_wb_ops += 2;
            }
            if num_pending_wb_ops >= MAX_OPS_IN_ONE_WRITE_BATCH {
                wb.write()?;
                wb = perpetual_db.objects.batch();
                num_pending_wb_ops = 0;
            }
        }
        wb.write()?;
        checkpoint_store.update_highest_executed_checkpoint(&snapshot_checkpoint)?;
        perpetual_db.set_recovery_epoch(reader.epoch)?;
        Ok(())

    }
}
