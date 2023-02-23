// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod snapshot;
#[cfg(test)]
mod tests;
mod util;

use crate::snapshot::StateSnapshotWriterV1;
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
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use tokio::sync::oneshot::Sender;
use tokio::time::Instant;
use tracing::info;
use tracing::log::error;
use typed_store::rocks::safe_drop_db;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
struct SuiSnapshotConfig {
    /// Check to see if new snapshot can be taken at this interval.
    ///
    /// If unspecified, this will default to `300` seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_period_s: Option<u64>,
    /// Number of latest `snapshots` to keep in the `output_path`. Older
    /// snapshots will be deleted
    ///
    /// If unspecified, this will default to `1`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_latest_snapshots_to_keep: Option<u32>,
    /// Absolute path on the local disk where snapshots are stored.
    ///
    /// If unspecified, this will default to `/opt/sui/snapshot`
    pub snapshot_path: Option<PathBuf>,
}

#[derive(Debug)]
pub struct SuiSnapshotMetrics {
    pub snapshot_success: IntCounter,
    pub snapshot_size_in_bytes: IntGauge,
    pub snapshot_latency_secs: Histogram,
}

impl SuiSnapshotMetrics {
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
            snapshot_latency_secs: register_histogram_with_registry!(
                "snapshot_latency_secs",
                "Latency of taking a state snapshot",
                registry,
            )
            .unwrap(),
        }
    }
}

struct SuiSnapshotCreateLoop {
    snapshot_path: PathBuf,
    db_path: PathBuf,
    interval_period_s: Duration,
    num_latest_snapshots_to_keep: u32,
    metrics: SuiSnapshotMetrics,
}

impl SuiSnapshotCreateLoop {
    pub fn new(config: SuiSnapshotConfig, db_path: &Path, registry: &Registry) -> Result<Self> {
        let metrics = SuiSnapshotMetrics::new(registry);
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
        Ok(SuiSnapshotCreateLoop {
            snapshot_path,
            db_path: db_path.to_path_buf(),
            interval_period_s: Duration::from_secs(config.interval_period_s.unwrap_or(300)),
            num_latest_snapshots_to_keep: config.num_latest_snapshots_to_keep.unwrap_or(1),
            metrics,
        })
    }
    pub async fn start(mut self) -> Sender<()> {
        info!("Sui snapshot creator loop started");
        let (sender, mut recv) = tokio::sync::oneshot::channel();
        let mut interval = tokio::time::interval(self.interval_period_s);
        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    now = interval.tick() => {
                        info!("Started writing state snapshot");
                        if let Err(e) = self.handle_snapshot_tick() {
                            error!("Failed to take state snapshot with error: {:?}", e);
                        } else {
                            info!("Finished writing state snapshot");
                            self.metrics.snapshot_latency_secs.observe(now.elapsed().as_secs() as f64);
                        }
                    },
                    _ = &mut recv => break,
                }
            }
        });
        sender
    }
    fn handle_snapshot_tick(&mut self) -> Result<()> {
        let input_dirs = read_dir(&self.db_path)?;
        let mut checkpoints_by_epoch = BTreeMap::new();
        for dir in input_dirs {
            let entry = dir?;
            let file_name = entry
                .file_name()
                .into_string()
                .map_err(|o| anyhow!("Failed while converting path to string for {:?}", o))?;
            if !file_name.starts_with("epoch-") && !entry.metadata()?.is_dir() {
                continue;
            }
            let epoch = file_name
                .split_once('-')
                .context("Failed to split dir name")
                .map(|(_, epoch)| epoch.parse::<u32>())??;
            checkpoints_by_epoch.insert(epoch, entry.path());
        }
        if checkpoints_by_epoch.is_empty() {
            return Ok(());
        }
        let output_dirs = read_dir(&self.snapshot_path)?;
        let mut snapshots_by_epoch = BTreeMap::new();
        for dir in output_dirs {
            let entry = dir?;
            let file_name = entry
                .file_name()
                .into_string()
                .map_err(|o| anyhow!("Failed while converting path to string for {:?}", o))?;
            let file_metadata = entry.metadata()?;
            if file_name.starts_with("tmp-epoch-") && file_metadata.is_dir() {
                fs::remove_dir_all(entry.path())?;
                continue;
            }
            if !file_name.starts_with("epoch-") || !file_metadata.is_dir() {
                continue;
            }
            let epoch = file_name
                .split_once('-')
                .context("Failed to split dir name")
                .map(|(_, epoch)| epoch.parse::<u32>())??;
            snapshots_by_epoch.insert(epoch, entry.path());
        }
        let next_epoch_to_snapshot = snapshots_by_epoch
            .last_key_value()
            .map(|(k, _)| k + 1)
            .unwrap_or(u32::MIN);
        let next_checkpointed_epoch = checkpoints_by_epoch
            .range((Included(next_epoch_to_snapshot), Unbounded))
            .next();
        if next_checkpointed_epoch.is_none() {
            return Ok(());
        }
        let (epoch_to_snapshot, checkpointed_db_path) = next_checkpointed_epoch.unwrap();
        let snapshot_tmp_path = self
            .snapshot_path
            .join(format!("tmp-epoch-{}", *epoch_to_snapshot));
        let snapshot_path = self
            .snapshot_path
            .join(format!("epoch-{}", *epoch_to_snapshot));
        let perpetual_db = AuthorityPerpetualTables::open(checkpointed_db_path, None);
        let snapshot_writer = StateSnapshotWriterV1::new(&snapshot_tmp_path)?;
        snapshot_writer.write_objects(perpetual_db.iter_live_object_set(), &perpetual_db)?;
        fs::rename(snapshot_tmp_path, &snapshot_path)?;
        info!(
            "Successfully completed state snapshot of epoch: {} at location: {}",
            *epoch_to_snapshot,
            snapshot_path.display()
        );
        mem::drop(perpetual_db);
        self.metrics.snapshot_success.inc();
        if let Ok(size) = get_size(snapshot_path) {
            self.metrics.snapshot_size_in_bytes.set(size as i64);
        }
        self.garbage_collect_old_db_checkpoints(*epoch_to_snapshot)?;
        self.garbage_collect_old_snapshots()?;
        Ok(())
    }
    fn garbage_collect_old_db_checkpoints(&mut self, max_epoch_to_delete: u32) -> Result<()> {
        let input_dirs = read_dir(&self.db_path)?;
        let mut checkpoints_by_epoch = BTreeMap::new();
        for dir in input_dirs {
            let entry = dir?;
            let file_name = entry
                .file_name()
                .into_string()
                .map_err(|o| anyhow!("Failed while converting path to string for {:?}", o))?;
            if !file_name.starts_with("epoch-") && !entry.metadata()?.is_dir() {
                continue;
            }
            let epoch = file_name
                .split_once('-')
                .context("Failed to split dir name")
                .map(|(_, epoch)| epoch.parse::<u32>())??;
            checkpoints_by_epoch.insert(epoch, entry.path());
        }
        for (epoch, path) in checkpoints_by_epoch.iter().rev() {
            if *epoch <= max_epoch_to_delete {
                info!("Dropping checkpointed db: {}", path.display());
                safe_drop_db(path.clone())?;
            }
        }
        Ok(())
    }
    fn garbage_collect_old_snapshots(&mut self) -> Result<()> {
        let output_dirs = read_dir(&self.snapshot_path)?;
        let mut snapshots_by_epoch = BTreeMap::new();
        for dir in output_dirs {
            let entry = dir?;
            let file_name = entry
                .file_name()
                .into_string()
                .map_err(|o| anyhow!("Failed while converting path to string for {:?}", o))?;
            let file_metadata = entry.metadata()?;
            if !file_name.starts_with("epoch-") || !file_metadata.is_dir() {
                continue;
            }
            let epoch = file_name
                .split_once('-')
                .context("Failed to split dir name")
                .map(|(_, epoch)| epoch.parse::<u32>())??;
            snapshots_by_epoch.insert(epoch, entry.path());
        }
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
