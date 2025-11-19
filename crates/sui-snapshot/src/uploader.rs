// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::writer::StateSnapshotWriterV1;
use anyhow::Result;
use bytes::Bytes;
use futures::StreamExt;
use object_store::DynObjectStore;
use prometheus::{
    IntCounter, IntGauge, Registry, register_int_counter_with_registry,
    register_int_gauge_with_registry,
};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use sui_core::checkpoints::CheckpointStore;
use sui_core::db_checkpoint_handler::{STATE_SNAPSHOT_COMPLETED_MARKER, SUCCESS_MARKER};
use sui_storage::FileCompression;
use sui_storage::object_store::ObjectStoreListExt;
use sui_storage::object_store::util::{
    find_all_dirs_with_epoch_prefix, find_missing_epochs_dirs, get, path_to_filesystem, put,
    run_manifest_update_loop,
};
use sui_types::digests::ChainIdentifier;
use sui_types::messages_checkpoint::CheckpointCommitment::ECMHLiveObjectSetDigest;
use tracing::{debug, error, info};

pub struct StateSnapshotUploaderMetrics {
    pub first_missing_state_snapshot_epoch: IntGauge,
    pub state_snapshot_upload_err: IntCounter,
}

impl StateSnapshotUploaderMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        let this = Self {
            first_missing_state_snapshot_epoch: register_int_gauge_with_registry!(
                "first_missing_state_snapshot_epoch",
                "First epoch for which we have no state snapshot in remote store",
                registry
            )
            .unwrap(),
            state_snapshot_upload_err: register_int_counter_with_registry!(
                "state_snapshot_upload_err",
                "Track upload errors we can alert on",
                registry
            )
            .unwrap(),
        };
        Arc::new(this)
    }
}

pub struct StateSnapshotUploader {
    /// Directory path on local disk where db checkpoints are stored
    db_checkpoint_path: PathBuf,
    /// Store on local disk where db checkpoints are written to
    db_checkpoint_store: Arc<DynObjectStore>,
    /// Checkpoint store; needed to fetch epoch state commitments for verification
    checkpoint_store: Arc<CheckpointStore>,
    /// Directory path on local disk where state snapshots are staged for upload
    staging_path: PathBuf,
    /// Store on local disk where state snapshots are staged for upload
    staging_store: Arc<DynObjectStore>,
    /// Remote store i.e. S3, GCS, etc where state snapshots are uploaded to
    snapshot_store: Arc<DynObjectStore>,
    /// Time interval to check for presence of new db checkpoint
    interval: Duration,
    metrics: Arc<StateSnapshotUploaderMetrics>,
    /// The chain identifier is derived from the genesis checkpoint and used to identify the
    /// network.
    chain_identifier: ChainIdentifier,
    /// Archive snapshots every N epochs (0 = disabled)
    archive_interval_epochs: u64,
}

impl StateSnapshotUploader {
    pub fn new(
        db_checkpoint_path: &std::path::Path,
        staging_path: &std::path::Path,
        snapshot_store_config: ObjectStoreConfig,
        interval_s: u64,
        registry: &Registry,
        checkpoint_store: Arc<CheckpointStore>,
        chain_identifier: ChainIdentifier,
        archive_interval_epochs: u64,
    ) -> Result<Arc<Self>> {
        let db_checkpoint_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(db_checkpoint_path.to_path_buf()),
            ..Default::default()
        };
        let staging_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(staging_path.to_path_buf()),
            ..Default::default()
        };
        Ok(Arc::new(StateSnapshotUploader {
            db_checkpoint_path: db_checkpoint_path.to_path_buf(),
            db_checkpoint_store: db_checkpoint_store_config.make()?,
            checkpoint_store,
            staging_path: staging_path.to_path_buf(),
            staging_store: staging_store_config.make()?,
            snapshot_store: snapshot_store_config.make()?,
            interval: Duration::from_secs(interval_s),
            metrics: StateSnapshotUploaderMetrics::new(registry),
            chain_identifier,
            archive_interval_epochs,
        }))
    }

    pub fn start(self: Arc<Self>) -> tokio::sync::broadcast::Sender<()> {
        let (kill_sender, _kill_receiver) = tokio::sync::broadcast::channel::<()>(1);
        tokio::task::spawn(Self::run_upload_loop(self.clone(), kill_sender.subscribe()));
        tokio::task::spawn(run_manifest_update_loop(
            self.snapshot_store.clone(),
            kill_sender.subscribe(),
        ));
        kill_sender
    }

    async fn upload_state_snapshot_to_object_store(&self, missing_epochs: Vec<u64>) -> Result<()> {
        let last_missing_epoch = missing_epochs.last().cloned().unwrap_or(0);
        info!(
            "upload_state_snapshot_to_object_store called with {} missing epochs, last_missing_epoch: {}",
            missing_epochs.len(),
            last_missing_epoch
        );

        let local_checkpoints_by_epoch =
            find_all_dirs_with_epoch_prefix(&self.db_checkpoint_store, None).await?;
        info!(
            "Found {} local checkpoint directories on disk",
            local_checkpoints_by_epoch.len()
        );

        let mut dirs: Vec<_> = local_checkpoints_by_epoch.iter().collect();
        dirs.sort_by_key(|(epoch_num, _path)| *epoch_num);

        if !dirs.is_empty() {
            let first_epoch = dirs.first().map(|(e, _)| **e).unwrap_or(0);
            let last_epoch = dirs.last().map(|(e, _)| **e).unwrap_or(0);
            info!(
                "Local checkpoint epoch range: {} to {} ({} epochs)",
                first_epoch,
                last_epoch,
                dirs.len()
            );
        }

        for (epoch, db_path) in dirs {
            if missing_epochs.contains(epoch) || *epoch >= last_missing_epoch {
                // TEMPORARY: Only upload epochs divisible by archive_interval_epochs for backfill
                if self.archive_interval_epochs > 0 && !epoch.is_multiple_of(self.archive_interval_epochs) {
                    debug!("Skipping epoch {} (not divisible by {})", *epoch, self.archive_interval_epochs);
                    continue;
                }
                info!(
                    "Starting state snapshot creation for epoch: {} (db_path: {})",
                    *epoch,
                    db_path.as_ref()
                );
                let state_snapshot_writer = StateSnapshotWriterV1::new_from_store(
                    &self.staging_path,
                    &self.staging_store,
                    &self.snapshot_store,
                    FileCompression::Zstd,
                    NonZeroUsize::new(20).unwrap(),
                )
                .await?;
                let db = Arc::new(AuthorityPerpetualTables::open(
                    &path_to_filesystem(self.db_checkpoint_path.clone(), &db_path.child("store"))?,
                    None,
                    None,
                ));

                // Get epoch state commitments, skip if not available
                let commitments = match self.checkpoint_store.get_epoch_state_commitments(*epoch) {
                    Ok(Some(commitments)) => {
                        info!(
                            "Successfully retrieved {} commitment(s) for epoch {}",
                            commitments.len(),
                            *epoch
                        );
                        commitments
                    }
                    Ok(None) => {
                        // Check if we have the last checkpoint sequence number for this epoch
                        match self.checkpoint_store.get_epoch_last_checkpoint_seq_number(*epoch) {
                            Ok(Some(seq)) => {
                                error!(
                                    "Epoch {} has last checkpoint seq {} in checkpoint store but get_epoch_last_checkpoint returned None",
                                    *epoch, seq
                                );
                                // Try to get the checkpoint directly to see what's missing
                                match self.checkpoint_store.get_checkpoint_by_sequence_number(seq) {
                                    Ok(Some(checkpoint)) => {
                                        error!(
                                            "Checkpoint {} exists (epoch: {}, end_of_epoch: {}), but commitments unavailable",
                                            seq,
                                            checkpoint.epoch(),
                                            checkpoint.end_of_epoch_data.is_some()
                                        );
                                    }
                                    Ok(None) => {
                                        error!("Checkpoint {} not found in checkpoint store", seq);
                                    }
                                    Err(e) => {
                                        error!("Error fetching checkpoint {}: {:?}", seq, e);
                                    }
                                }
                            }
                            Ok(None) => {
                                info!(
                                    "Epoch {} has no entry in epoch_last_checkpoint_map, checkpoint data not synced yet",
                                    *epoch
                                );
                            }
                            Err(e) => {
                                error!("Failed to check last checkpoint for epoch {}: {:?}", *epoch, e);
                            }
                        }
                        continue;
                    }
                    Err(e) => {
                        error!("Failed to get epoch state commitments for epoch {}: {:?}", *epoch, e);
                        continue;
                    }
                };

                let state_hash_commitment = match commitments.last().cloned() {
                    Some(ECMHLiveObjectSetDigest(digest)) => digest,
                    Some(_) => {
                        error!("Expected ECMHLiveObjectSetDigest for epoch {}, skipping", *epoch);
                        continue;
                    }
                    None => {
                        info!("No commitments found for epoch {}, skipping", *epoch);
                        continue;
                    }
                };
                state_snapshot_writer
                    .write(*epoch, db, state_hash_commitment, self.chain_identifier)
                    .await?;
                info!("State snapshot creation successful for epoch: {}", *epoch);
                // Drop marker in the output directory that upload completed successfully
                let bytes = Bytes::from_static(b"success");
                let success_marker = db_path.child(SUCCESS_MARKER);
                put(&self.snapshot_store, &success_marker, bytes.clone()).await?;
                let bytes = Bytes::from_static(b"success");
                let state_snapshot_completed_marker =
                    db_path.child(STATE_SNAPSHOT_COMPLETED_MARKER);
                put(
                    &self.db_checkpoint_store.clone(),
                    &state_snapshot_completed_marker,
                    bytes.clone(),
                )
                .await?;
                info!("State snapshot completed for epoch: {epoch}");

                // Archive snapshot if epoch meets archival criteria
                if let Err(e) = self.archive_epoch_if_needed(*epoch).await {
                    error!(
                        "Failed to archive epoch {} (non-fatal, continuing): {:?}",
                        epoch, e
                    );
                }
            } else {
                debug!(
                    "Skipping epoch {} (not in missing_epochs list and < last_missing_epoch {})",
                    *epoch, last_missing_epoch
                );
                let bytes = Bytes::from_static(b"success");
                let state_snapshot_completed_marker =
                    db_path.child(STATE_SNAPSHOT_COMPLETED_MARKER);
                put(
                    &self.db_checkpoint_store.clone(),
                    &state_snapshot_completed_marker,
                    bytes.clone(),
                )
                .await?;
            }
        }
        info!("upload_state_snapshot_to_object_store completed");
        Ok(())
    }

    async fn run_upload_loop(
        self: Arc<Self>,
        mut recv: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()> {
        let mut interval = tokio::time::interval(self.interval);
        info!("State snapshot uploader loop started");
        loop {
            tokio::select! {
                _now = interval.tick() => {
                    let missing_epochs = self.get_missing_epochs().await;
                    match missing_epochs {
                        Ok(epochs) => {
                            let first_missing_epoch = epochs.first().cloned().unwrap_or(0);
                            self.metrics.first_missing_state_snapshot_epoch.set(first_missing_epoch as i64);
                            if let Err(err) = self.upload_state_snapshot_to_object_store(epochs).await {
                                self.metrics.state_snapshot_upload_err.inc();
                                error!("Failed to upload state snapshot to remote store with err: {:?}", err);
                            } else {
                                debug!("Successfully completed snapshot upload loop");
                            }
                        }
                        Err(err) => {
                            error!("Failed to find missing state snapshot in remote store: {:?}", err);
                        }
                    }
                },
                _ = recv.recv() => break,
            }
        }
        Ok(())
    }

    async fn get_missing_epochs(&self) -> Result<Vec<u64>> {
        let missing_epochs = find_missing_epochs_dirs(&self.snapshot_store, SUCCESS_MARKER).await?;
        Ok(missing_epochs.to_vec())
    }

    pub(crate) async fn archive_epoch_if_needed(&self, epoch: u64) -> Result<()> {
        if self.archive_interval_epochs == 0 {
            return Ok(());
        }

        if !epoch.is_multiple_of(self.archive_interval_epochs) {
            debug!(
                "Epoch {} is not divisible by archive interval {}, skipping archival",
                epoch, self.archive_interval_epochs
            );
            return Ok(());
        }

        info!(
            "Epoch {} is divisible by {}, archiving to archive/ subdirectory",
            epoch, self.archive_interval_epochs
        );

        let source_prefix = object_store::path::Path::from(format!("epoch_{}", epoch));

        info!("Listing files in {} for archival", source_prefix);

        let mut paths = self.snapshot_store.list_objects(Some(&source_prefix)).await;
        let mut files_copied = 0;

        while let Some(res) = paths.next().await {
            match res {
                Ok(object_metadata) => {
                    let source_path = &object_metadata.location;
                    let relative_path = source_path
                        .as_ref()
                        .strip_prefix(&format!("epoch_{}/", epoch))
                        .unwrap_or(source_path.as_ref());
                    let dest_path = object_store::path::Path::from(format!(
                        "archive/epoch_{}/{}",
                        epoch, relative_path
                    ));

                    debug!("Copying {} to {}", source_path, dest_path);

                    let bytes = get(&self.snapshot_store, source_path).await?;
                    put(&self.snapshot_store, &dest_path, bytes).await?;

                    files_copied += 1;
                }
                Err(e) => {
                    error!("Failed to list objects for archival: {:?}", e);
                    return Err(e.into());
                }
            }
        }

        info!(
            "Successfully archived epoch {} ({} files copied to archive/epoch_{})",
            epoch, files_copied, epoch
        );
        Ok(())
    }
}
