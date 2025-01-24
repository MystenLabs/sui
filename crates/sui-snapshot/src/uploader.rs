// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::writer::StateSnapshotWriterV1;
use anyhow::Result;
use bytes::Bytes;
use object_store::DynObjectStore;
use prometheus::{
    register_int_counter_with_registry, register_int_gauge_with_registry, IntCounter, IntGauge,
    Registry,
};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use sui_core::checkpoints::CheckpointStore;
use sui_core::db_checkpoint_handler::{STATE_SNAPSHOT_COMPLETED_MARKER, SUCCESS_MARKER};
use sui_storage::object_store::util::{
    find_all_dirs_with_epoch_prefix, find_missing_epochs_dirs, path_to_filesystem, put,
    run_manifest_update_loop,
};
use sui_storage::FileCompression;
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
        let local_checkpoints_by_epoch =
            find_all_dirs_with_epoch_prefix(&self.db_checkpoint_store, None).await?;
        let mut dirs: Vec<_> = local_checkpoints_by_epoch.iter().collect();
        dirs.sort_by_key(|(epoch_num, _path)| *epoch_num);
        for (epoch, db_path) in dirs {
            if missing_epochs.contains(epoch) || *epoch >= last_missing_epoch {
                info!("Starting state snapshot creation for epoch: {}", *epoch);
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
                ));
                let commitments = self
                    .checkpoint_store
                    .get_epoch_state_commitments(*epoch)
                    .expect("Expected last checkpoint of epoch to have end of epoch data")
                    .expect("Expected end of epoch data to be present");
                let ECMHLiveObjectSetDigest(state_hash_commitment) = commitments
                    .last()
                    .expect("Expected at least one commitment")
                    .clone();
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
            } else {
                let bytes = Bytes::from_static(b"success");
                let state_snapshot_completed_marker =
                    db_path.child(STATE_SNAPSHOT_COMPLETED_MARKER);
                put(
                    &self.db_checkpoint_store.clone(),
                    &state_snapshot_completed_marker,
                    bytes.clone(),
                )
                .await?;
                info!("State snapshot skipped for epoch: {epoch}");
            }
        }
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
}
