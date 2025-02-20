// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_store_pruner::{
    AuthorityStorePruner, AuthorityStorePruningMetrics, EPOCH_DURATION_MS_FOR_TESTING,
};
use crate::authority::authority_store_tables::AuthorityPerpetualTables;
use crate::checkpoints::CheckpointStore;
use crate::rpc_index::RpcIndexStore;
use anyhow::Result;
use bytes::Bytes;
use futures::future::try_join_all;
use object_store::path::Path;
use object_store::DynObjectStore;
use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::fs;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_config::node::AuthorityStorePruningConfig;
use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_storage::object_store::util::{
    copy_recursively, find_all_dirs_with_epoch_prefix, find_missing_epochs_dirs,
    path_to_filesystem, put, run_manifest_update_loop, write_snapshot_manifest,
};
use tracing::{debug, error, info};

pub const SUCCESS_MARKER: &str = "_SUCCESS";
pub const TEST_MARKER: &str = "_TEST";
pub const UPLOAD_COMPLETED_MARKER: &str = "_UPLOAD_COMPLETED";
pub const STATE_SNAPSHOT_COMPLETED_MARKER: &str = "_STATE_SNAPSHOT_COMPLETED";

pub struct DBCheckpointMetrics {
    pub first_missing_db_checkpoint_epoch: IntGauge,
    pub num_local_db_checkpoints: IntGauge,
}

impl DBCheckpointMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        let this = Self {
            first_missing_db_checkpoint_epoch: register_int_gauge_with_registry!(
                "first_missing_db_checkpoint_epoch",
                "First epoch for which we have no db checkpoint in remote store",
                registry
            )
            .unwrap(),
            num_local_db_checkpoints: register_int_gauge_with_registry!(
                "num_local_db_checkpoints",
                "Number of RocksDB checkpoints currently residing on local disk (i.e. not yet garbage collected)",
                registry
            )
            .unwrap(),
        };
        Arc::new(this)
    }
}

pub struct DBCheckpointHandler {
    /// Directory on local disk where db checkpoints are stored
    input_object_store: Arc<DynObjectStore>,
    /// DB checkpoint directory on local filesystem
    input_root_path: PathBuf,
    /// Bucket on cloud object store where db checkpoints will be copied
    output_object_store: Option<Arc<DynObjectStore>>,
    /// Time interval to check for presence of new db checkpoint
    interval: Duration,
    /// File markers which signal that local db checkpoint can be garbage collected
    gc_markers: Vec<String>,
    /// Boolean flag to enable/disable object pruning and manual compaction before upload
    prune_and_compact_before_upload: bool,
    /// If true, upload will block on state snapshot upload completed marker
    state_snapshot_enabled: bool,
    /// Pruning objects
    pruning_config: AuthorityStorePruningConfig,
    metrics: Arc<DBCheckpointMetrics>,
}

impl DBCheckpointHandler {
    pub fn new(
        input_path: &std::path::Path,
        output_object_store_config: Option<&ObjectStoreConfig>,
        interval_s: u64,
        prune_and_compact_before_upload: bool,
        pruning_config: AuthorityStorePruningConfig,
        registry: &Registry,
        state_snapshot_enabled: bool,
    ) -> Result<Arc<Self>> {
        let input_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(input_path.to_path_buf()),
            ..Default::default()
        };
        let mut gc_markers = vec![UPLOAD_COMPLETED_MARKER.to_string()];
        if state_snapshot_enabled {
            gc_markers.push(STATE_SNAPSHOT_COMPLETED_MARKER.to_string());
        }
        Ok(Arc::new(DBCheckpointHandler {
            input_object_store: input_store_config.make()?,
            input_root_path: input_path.to_path_buf(),
            output_object_store: output_object_store_config
                .map(|config| config.make().expect("Failed to make object store")),
            interval: Duration::from_secs(interval_s),
            gc_markers,
            prune_and_compact_before_upload,
            state_snapshot_enabled,
            pruning_config,
            metrics: DBCheckpointMetrics::new(registry),
        }))
    }
    pub fn new_for_test(
        input_object_store_config: &ObjectStoreConfig,
        output_object_store_config: Option<&ObjectStoreConfig>,
        interval_s: u64,
        prune_and_compact_before_upload: bool,
        state_snapshot_enabled: bool,
    ) -> Result<Arc<Self>> {
        Ok(Arc::new(DBCheckpointHandler {
            input_object_store: input_object_store_config.make()?,
            input_root_path: input_object_store_config
                .directory
                .as_ref()
                .unwrap()
                .clone(),
            output_object_store: output_object_store_config
                .map(|config| config.make().expect("Failed to make object store")),
            interval: Duration::from_secs(interval_s),
            gc_markers: vec![UPLOAD_COMPLETED_MARKER.to_string(), TEST_MARKER.to_string()],
            prune_and_compact_before_upload,
            state_snapshot_enabled,
            pruning_config: AuthorityStorePruningConfig::default(),
            metrics: DBCheckpointMetrics::new(&Registry::default()),
        }))
    }
    pub fn start(self: Arc<Self>) -> tokio::sync::broadcast::Sender<()> {
        let (kill_sender, _kill_receiver) = tokio::sync::broadcast::channel::<()>(1);
        if self.output_object_store.is_some() {
            tokio::task::spawn(Self::run_db_checkpoint_upload_loop(
                self.clone(),
                kill_sender.subscribe(),
            ));
            tokio::task::spawn(run_manifest_update_loop(
                self.output_object_store.as_ref().unwrap().clone(),
                kill_sender.subscribe(),
            ));
        } else {
            // if db checkpoint remote store is not specified, cleanup loop
            // is run to immediately mark db checkpoint upload as successful
            // so that they can be snapshotted and garbage collected
            tokio::task::spawn(Self::run_db_checkpoint_cleanup_loop(
                self.clone(),
                kill_sender.subscribe(),
            ));
        }
        tokio::task::spawn(Self::run_db_checkpoint_gc_loop(
            self,
            kill_sender.subscribe(),
        ));
        kill_sender
    }
    async fn run_db_checkpoint_upload_loop(
        self: Arc<Self>,
        mut recv: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()> {
        let mut interval = tokio::time::interval(self.interval);
        info!("DB checkpoint upload loop started");
        loop {
            tokio::select! {
                _now = interval.tick() => {
                    let local_checkpoints_by_epoch =
                        find_all_dirs_with_epoch_prefix(&self.input_object_store, None).await?;
                    self.metrics.num_local_db_checkpoints.set(local_checkpoints_by_epoch.len() as i64);
                    match find_missing_epochs_dirs(self.output_object_store.as_ref().unwrap(), SUCCESS_MARKER).await {
                        Ok(epochs) => {
                            self.metrics.first_missing_db_checkpoint_epoch.set(epochs.first().cloned().unwrap_or(0) as i64);
                            if let Err(err) = self.upload_db_checkpoints_to_object_store(epochs).await {
                                error!("Failed to upload db checkpoint to remote store with err: {:?}", err);
                            }
                        }
                        Err(err) => {
                            error!("Failed to find missing db checkpoints in remote store: {:?}", err);
                        }
                    }
                },
                 _ = recv.recv() => break,
            }
        }
        Ok(())
    }
    async fn run_db_checkpoint_cleanup_loop(
        self: Arc<Self>,
        mut recv: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()> {
        let mut interval = tokio::time::interval(self.interval);
        info!("DB checkpoint upload disabled. DB checkpoint cleanup loop started");
        loop {
            tokio::select! {
                _now = interval.tick() => {
                    let local_checkpoints_by_epoch =
                        find_all_dirs_with_epoch_prefix(&self.input_object_store, None).await?;
                    self.metrics.num_local_db_checkpoints.set(local_checkpoints_by_epoch.len() as i64);
                    let mut dirs: Vec<_> = local_checkpoints_by_epoch.iter().collect();
                    dirs.sort_by_key(|(epoch_num, _path)| *epoch_num);
                    for (_, db_path) in dirs {
                        // If db checkpoint marked as completed, skip
                        let local_db_path = path_to_filesystem(self.input_root_path.clone(), db_path)?;
                        let upload_completed_path = local_db_path.join(UPLOAD_COMPLETED_MARKER);
                        if upload_completed_path.exists() {
                            continue;
                        }
                        let bytes = Bytes::from_static(b"success");
                        let upload_completed_marker = db_path.child(UPLOAD_COMPLETED_MARKER);
                        put(&self.input_object_store,
                            &upload_completed_marker,
                            bytes.clone(),
                        )
                        .await?;
                    }
                },
                 _ = recv.recv() => break,
            }
        }
        Ok(())
    }
    async fn run_db_checkpoint_gc_loop(
        self: Arc<Self>,
        mut recv: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()> {
        let mut gc_interval = tokio::time::interval(Duration::from_secs(30));
        info!("DB checkpoint garbage collection loop started");
        loop {
            tokio::select! {
                _now = gc_interval.tick() => {
                    if let Ok(deleted) = self.garbage_collect_old_db_checkpoints().await {
                        if !deleted.is_empty() {
                            info!("Garbage collected local db checkpoints: {:?}", deleted);
                        }
                    }
                },
                 _ = recv.recv() => break,
            }
        }
        Ok(())
    }

    async fn prune_and_compact(
        &self,
        db_path: PathBuf,
        epoch: u64,
        epoch_duration_ms: u64,
    ) -> Result<()> {
        let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&db_path.join("store"), None));
        let checkpoint_store = Arc::new(CheckpointStore::new_for_db_checkpoint_handler(
            &db_path.join("checkpoints"),
        ));
        let rpc_index = RpcIndexStore::new_without_init(&db_path);
        let metrics = AuthorityStorePruningMetrics::new(&Registry::default());
        info!(
            "Pruning db checkpoint in {:?} for epoch: {epoch}",
            db_path.display()
        );
        AuthorityStorePruner::prune_objects_for_eligible_epochs(
            &perpetual_db,
            &checkpoint_store,
            Some(&rpc_index),
            None,
            self.pruning_config.clone(),
            metrics,
            epoch_duration_ms,
        )
        .await?;
        info!(
            "Compacting db checkpoint in {:?} for epoch: {epoch}",
            db_path.display()
        );
        AuthorityStorePruner::compact(&perpetual_db)?;
        Ok(())
    }
    async fn upload_db_checkpoints_to_object_store(
        &self,
        missing_epochs: Vec<u64>,
    ) -> Result<(), anyhow::Error> {
        let last_missing_epoch = missing_epochs.last().cloned().unwrap_or(0);
        let local_checkpoints_by_epoch =
            find_all_dirs_with_epoch_prefix(&self.input_object_store, None).await?;
        let mut dirs: Vec<_> = local_checkpoints_by_epoch.iter().collect();
        dirs.sort_by_key(|(epoch_num, _path)| *epoch_num);
        let object_store = self
            .output_object_store
            .as_ref()
            .expect("Expected object store to exist")
            .clone();
        for (epoch, db_path) in dirs {
            // Convert `db_path` to the local filesystem path to where db checkpoint is stored
            let local_db_path = path_to_filesystem(self.input_root_path.clone(), db_path)?;
            if missing_epochs.contains(epoch) || *epoch >= last_missing_epoch {
                if self.state_snapshot_enabled {
                    let snapshot_completed_marker =
                        local_db_path.join(STATE_SNAPSHOT_COMPLETED_MARKER);
                    if !snapshot_completed_marker.exists() {
                        info!("DB checkpoint upload for epoch {} to wait until state snasphot uploaded", *epoch);
                        continue;
                    }
                }

                if self.prune_and_compact_before_upload {
                    // Invoke pruning and compaction on the db checkpoint
                    self.prune_and_compact(local_db_path, *epoch, EPOCH_DURATION_MS_FOR_TESTING)
                        .await?;
                }

                info!("Copying db checkpoint for epoch: {epoch} to remote storage");
                copy_recursively(
                    db_path,
                    &self.input_object_store,
                    &object_store,
                    NonZeroUsize::new(20).unwrap(),
                )
                .await?;

                // This writes a single "MANIFEST" file which contains a list of all files that make up a db snapshot
                write_snapshot_manifest(db_path, &object_store, format!("epoch_{}/", epoch))
                    .await?;
                // Drop marker in the output directory that upload completed successfully
                let bytes = Bytes::from_static(b"success");
                let success_marker = db_path.child(SUCCESS_MARKER);
                put(&object_store, &success_marker, bytes.clone()).await?;
            }
            let bytes = Bytes::from_static(b"success");
            let upload_completed_marker = db_path.child(UPLOAD_COMPLETED_MARKER);
            put(
                &self.input_object_store,
                &upload_completed_marker,
                bytes.clone(),
            )
            .await?;
        }
        Ok(())
    }

    async fn garbage_collect_old_db_checkpoints(&self) -> Result<Vec<u64>> {
        let local_checkpoints_by_epoch =
            find_all_dirs_with_epoch_prefix(&self.input_object_store, None).await?;
        let mut deleted = Vec::new();
        for (epoch, path) in local_checkpoints_by_epoch.iter() {
            let marker_paths: Vec<Path> = self
                .gc_markers
                .iter()
                .map(|marker| path.child(marker.clone()))
                .collect();
            let all_markers_present = try_join_all(
                marker_paths
                    .iter()
                    .map(|path| self.input_object_store.get(path)),
            )
            .await;
            match all_markers_present {
                // After state snapshots, gc will also need to wait for a state snapshot
                // upload completed marker
                Ok(_) => {
                    info!("Deleting db checkpoint dir: {path} for epoch: {epoch}");
                    deleted.push(*epoch);
                    let local_fs_path = path_to_filesystem(self.input_root_path.clone(), path)?;
                    fs::remove_dir_all(&local_fs_path)?;
                }
                Err(_) => {
                    debug!("Not ready for deletion yet: {path}");
                }
            }
        }
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use crate::db_checkpoint_handler::{
        DBCheckpointHandler, SUCCESS_MARKER, TEST_MARKER, UPLOAD_COMPLETED_MARKER,
    };
    use itertools::Itertools;
    use std::fs;
    use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
    use sui_storage::object_store::util::{
        find_all_dirs_with_epoch_prefix, find_missing_epochs_dirs, path_to_filesystem,
    };
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_basic() -> anyhow::Result<()> {
        let checkpoint_dir = TempDir::new()?;
        let checkpoint_dir_path = checkpoint_dir.path();
        let local_epoch0_checkpoint = checkpoint_dir_path.join("epoch_0");
        fs::create_dir(&local_epoch0_checkpoint)?;
        let file1 = local_epoch0_checkpoint.join("file1");
        fs::write(file1, b"Lorem ipsum")?;
        let file2 = local_epoch0_checkpoint.join("file2");
        fs::write(file2, b"Lorem ipsum")?;
        let nested_dir = local_epoch0_checkpoint.join("data");
        fs::create_dir(&nested_dir)?;
        let file3 = nested_dir.join("file3");
        fs::write(file3, b"Lorem ipsum")?;

        let remote_checkpoint_dir = TempDir::new()?;
        let remote_checkpoint_dir_path = remote_checkpoint_dir.path();
        let remote_epoch0_checkpoint = remote_checkpoint_dir_path.join("epoch_0");

        let input_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(checkpoint_dir_path.to_path_buf()),
            ..Default::default()
        };
        let output_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(remote_checkpoint_dir_path.to_path_buf()),
            ..Default::default()
        };
        let db_checkpoint_handler = DBCheckpointHandler::new_for_test(
            &input_store_config,
            Some(&output_store_config),
            10,
            false,
            false,
        )?;
        let local_checkpoints_by_epoch =
            find_all_dirs_with_epoch_prefix(&db_checkpoint_handler.input_object_store, None)
                .await?;
        assert!(!local_checkpoints_by_epoch.is_empty());
        assert_eq!(*local_checkpoints_by_epoch.first_key_value().unwrap().0, 0);
        assert_eq!(
            path_to_filesystem(
                db_checkpoint_handler.input_root_path.clone(),
                local_checkpoints_by_epoch.first_key_value().unwrap().1
            )
            .unwrap(),
            std::fs::canonicalize(local_epoch0_checkpoint.clone()).unwrap()
        );
        let missing_epochs = find_missing_epochs_dirs(
            db_checkpoint_handler.output_object_store.as_ref().unwrap(),
            SUCCESS_MARKER,
        )
        .await?;
        db_checkpoint_handler
            .upload_db_checkpoints_to_object_store(missing_epochs)
            .await?;

        assert!(remote_epoch0_checkpoint.join("file1").exists());
        assert!(remote_epoch0_checkpoint.join("file2").exists());
        assert!(remote_epoch0_checkpoint.join("data").join("file3").exists());
        assert!(remote_epoch0_checkpoint.join(SUCCESS_MARKER).exists());
        assert!(local_epoch0_checkpoint
            .join(UPLOAD_COMPLETED_MARKER)
            .exists());

        // Drop an extra gc marker meant only for gc to trigger
        let test_marker = local_epoch0_checkpoint.join(TEST_MARKER);
        fs::write(test_marker, b"Lorem ipsum")?;
        db_checkpoint_handler
            .garbage_collect_old_db_checkpoints()
            .await?;

        assert!(!local_epoch0_checkpoint.join("file1").exists());
        assert!(!local_epoch0_checkpoint.join("file1").exists());
        assert!(!local_epoch0_checkpoint.join("file2").exists());
        assert!(!local_epoch0_checkpoint.join("data").join("file3").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_upload_resumes() -> anyhow::Result<()> {
        let checkpoint_dir = TempDir::new()?;
        let checkpoint_dir_path = checkpoint_dir.path();
        let local_epoch0_checkpoint = checkpoint_dir_path.join("epoch_0");

        let remote_checkpoint_dir = TempDir::new()?;
        let remote_checkpoint_dir_path = remote_checkpoint_dir.path();
        let remote_epoch0_checkpoint = remote_checkpoint_dir_path.join("epoch_0");

        let input_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(checkpoint_dir_path.to_path_buf()),
            ..Default::default()
        };
        let output_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(remote_checkpoint_dir_path.to_path_buf()),
            ..Default::default()
        };
        let db_checkpoint_handler = DBCheckpointHandler::new_for_test(
            &input_store_config,
            Some(&output_store_config),
            10,
            false,
            false,
        )?;

        fs::create_dir(&local_epoch0_checkpoint)?;
        let file1 = local_epoch0_checkpoint.join("file1");
        fs::write(file1, b"Lorem ipsum")?;
        let file2 = local_epoch0_checkpoint.join("file2");
        fs::write(file2, b"Lorem ipsum")?;
        let nested_dir = local_epoch0_checkpoint.join("data");
        fs::create_dir(&nested_dir)?;
        let file3 = nested_dir.join("file3");
        fs::write(file3, b"Lorem ipsum")?;

        let missing_epochs = find_missing_epochs_dirs(
            db_checkpoint_handler.output_object_store.as_ref().unwrap(),
            SUCCESS_MARKER,
        )
        .await?;
        db_checkpoint_handler
            .upload_db_checkpoints_to_object_store(missing_epochs)
            .await?;
        assert!(remote_epoch0_checkpoint.join("file1").exists());
        assert!(remote_epoch0_checkpoint.join("file2").exists());
        assert!(remote_epoch0_checkpoint.join("data").join("file3").exists());
        assert!(remote_epoch0_checkpoint.join(SUCCESS_MARKER).exists());
        assert!(local_epoch0_checkpoint
            .join(UPLOAD_COMPLETED_MARKER)
            .exists());

        // Add a new db checkpoint to the local checkpoint directory
        let local_epoch1_checkpoint = checkpoint_dir_path.join("epoch_1");
        fs::create_dir(&local_epoch1_checkpoint)?;
        let file1 = local_epoch1_checkpoint.join("file1");
        fs::write(file1, b"Lorem ipsum")?;
        let file2 = local_epoch1_checkpoint.join("file2");
        fs::write(file2, b"Lorem ipsum")?;
        let nested_dir = local_epoch1_checkpoint.join("data");
        fs::create_dir(&nested_dir)?;
        let file3 = nested_dir.join("file3");
        fs::write(file3, b"Lorem ipsum")?;

        // Now delete the success marker from remote checkpointed directory
        // This is the scenario where uploads stops mid way because system stopped
        fs::remove_file(remote_epoch0_checkpoint.join(SUCCESS_MARKER))?;

        // Checkpoint handler should copy checkpoint for epoch_0 first before copying
        // epoch_1
        let missing_epochs = find_missing_epochs_dirs(
            db_checkpoint_handler.output_object_store.as_ref().unwrap(),
            SUCCESS_MARKER,
        )
        .await?;
        db_checkpoint_handler
            .upload_db_checkpoints_to_object_store(missing_epochs)
            .await?;
        assert!(remote_epoch0_checkpoint.join("file1").exists());
        assert!(remote_epoch0_checkpoint.join("file2").exists());
        assert!(remote_epoch0_checkpoint.join("data").join("file3").exists());
        assert!(remote_epoch0_checkpoint.join(SUCCESS_MARKER).exists());
        assert!(local_epoch0_checkpoint
            .join(UPLOAD_COMPLETED_MARKER)
            .exists());

        let remote_epoch1_checkpoint = remote_checkpoint_dir_path.join("epoch_1");
        assert!(remote_epoch1_checkpoint.join("file1").exists());
        assert!(remote_epoch1_checkpoint.join("file2").exists());
        assert!(remote_epoch1_checkpoint.join("data").join("file3").exists());
        assert!(remote_epoch1_checkpoint.join(SUCCESS_MARKER).exists());
        assert!(local_epoch1_checkpoint
            .join(UPLOAD_COMPLETED_MARKER)
            .exists());

        // Drop an extra gc marker meant only for gc to trigger
        let test_marker = local_epoch0_checkpoint.join(TEST_MARKER);
        fs::write(test_marker, b"Lorem ipsum")?;
        let test_marker = local_epoch1_checkpoint.join(TEST_MARKER);
        fs::write(test_marker, b"Lorem ipsum")?;

        db_checkpoint_handler
            .garbage_collect_old_db_checkpoints()
            .await?;
        assert!(!local_epoch0_checkpoint.join("file1").exists());
        assert!(!local_epoch0_checkpoint.join("file1").exists());
        assert!(!local_epoch0_checkpoint.join("file2").exists());
        assert!(!local_epoch0_checkpoint.join("data").join("file3").exists());
        assert!(!local_epoch1_checkpoint.join("file1").exists());
        assert!(!local_epoch1_checkpoint.join("file1").exists());
        assert!(!local_epoch1_checkpoint.join("file2").exists());
        assert!(!local_epoch1_checkpoint.join("data").join("file3").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_missing_epochs() -> anyhow::Result<()> {
        let checkpoint_dir = TempDir::new()?;
        let checkpoint_dir_path = checkpoint_dir.path();
        let local_epoch0_checkpoint = checkpoint_dir_path.join("epoch_0");
        fs::create_dir(&local_epoch0_checkpoint)?;
        let local_epoch1_checkpoint = checkpoint_dir_path.join("epoch_1");
        fs::create_dir(&local_epoch1_checkpoint)?;
        // Missing epoch 2
        let local_epoch3_checkpoint = checkpoint_dir_path.join("epoch_3");
        fs::create_dir(&local_epoch3_checkpoint)?;
        let remote_checkpoint_dir = TempDir::new()?;
        let remote_checkpoint_dir_path = remote_checkpoint_dir.path();

        let input_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(checkpoint_dir_path.to_path_buf()),
            ..Default::default()
        };

        let output_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(remote_checkpoint_dir_path.to_path_buf()),
            ..Default::default()
        };
        let db_checkpoint_handler = DBCheckpointHandler::new_for_test(
            &input_store_config,
            Some(&output_store_config),
            10,
            false,
            false,
        )?;

        let missing_epochs = find_missing_epochs_dirs(
            db_checkpoint_handler.output_object_store.as_ref().unwrap(),
            SUCCESS_MARKER,
        )
        .await?;
        db_checkpoint_handler
            .upload_db_checkpoints_to_object_store(missing_epochs)
            .await?;

        let first_missing_epoch = find_missing_epochs_dirs(
            db_checkpoint_handler.output_object_store.as_ref().unwrap(),
            SUCCESS_MARKER,
        )
        .await?
        .first()
        .cloned()
        .unwrap();
        assert_eq!(first_missing_epoch, 2);

        let remote_epoch0_checkpoint = remote_checkpoint_dir_path.join("epoch_0");
        fs::remove_file(remote_epoch0_checkpoint.join(SUCCESS_MARKER))?;

        let first_missing_epoch = find_missing_epochs_dirs(
            db_checkpoint_handler.output_object_store.as_ref().unwrap(),
            SUCCESS_MARKER,
        )
        .await?
        .first()
        .cloned()
        .unwrap();
        assert_eq!(first_missing_epoch, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_range_missing_epochs() -> anyhow::Result<()> {
        let checkpoint_dir = TempDir::new()?;
        let checkpoint_dir_path = checkpoint_dir.path();
        let local_epoch100_checkpoint = checkpoint_dir_path.join("epoch_100");
        fs::create_dir(&local_epoch100_checkpoint)?;
        let local_epoch200_checkpoint = checkpoint_dir_path.join("epoch_200");
        fs::create_dir(&local_epoch200_checkpoint)?;
        let remote_checkpoint_dir = TempDir::new()?;
        let remote_checkpoint_dir_path = remote_checkpoint_dir.path();

        let input_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(checkpoint_dir_path.to_path_buf()),
            ..Default::default()
        };

        let output_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(remote_checkpoint_dir_path.to_path_buf()),
            ..Default::default()
        };
        let db_checkpoint_handler = DBCheckpointHandler::new_for_test(
            &input_store_config,
            Some(&output_store_config),
            10,
            false,
            false,
        )?;

        let missing_epochs = find_missing_epochs_dirs(
            db_checkpoint_handler.output_object_store.as_ref().unwrap(),
            SUCCESS_MARKER,
        )
        .await?;
        assert_eq!(missing_epochs, vec![0]);
        db_checkpoint_handler
            .upload_db_checkpoints_to_object_store(missing_epochs)
            .await?;

        let missing_epochs = find_missing_epochs_dirs(
            db_checkpoint_handler.output_object_store.as_ref().unwrap(),
            SUCCESS_MARKER,
        )
        .await?;
        let mut expected_missing_epochs: Vec<u64> = (0..100).collect();
        expected_missing_epochs.extend((101..200).collect_vec().iter());
        expected_missing_epochs.push(201);
        assert_eq!(missing_epochs, expected_missing_epochs);
        Ok(())
    }
}
