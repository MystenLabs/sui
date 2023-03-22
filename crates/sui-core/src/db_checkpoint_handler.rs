// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use futures::future::try_join_all;
use object_store::path::Path;
use object_store::{DynObjectStore, Error};
use oneshot::channel;
use std::collections::BTreeMap;
use std::collections::Bound::{Included, Unbounded};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;
use sui_storage::object_store::util::{copy_recursively, delete_recursively, put};
use sui_storage::object_store::{ObjectStoreConfig, ObjectStoreType};
use tokio::sync::oneshot;
use tokio::sync::oneshot::Sender;
use tracing::{debug, error, info};

pub const SUCCESS_MARKER: &str = "_SUCCESS";
pub const TEST_MARKER: &str = "_TEST";
pub const UPLOAD_COMPLETED_MARKER: &str = "_UPLOAD_COMPLETED";

pub struct DBCheckpointHandler {
    /// Directory on local disk where db checkpoints are stored
    input_object_store: Arc<DynObjectStore>,
    /// Bucket on cloud object store where db checkpoints will be copied
    output_object_store: Arc<DynObjectStore>,
    /// Time interval to check for presence of new db checkpoint
    interval: Duration,
    /// File markers which signal that local db checkpoint can be garbage collected
    gc_markers: Vec<String>,
}

impl DBCheckpointHandler {
    pub fn new(
        input_path: &std::path::Path,
        output_object_store_config: &ObjectStoreConfig,
        interval_s: u64,
    ) -> Result<Self> {
        let input_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(input_path.to_path_buf()),
            ..Default::default()
        };
        Ok(DBCheckpointHandler {
            input_object_store: input_store_config.make()?,
            output_object_store: output_object_store_config.make()?,
            interval: Duration::from_secs(interval_s),
            gc_markers: vec![UPLOAD_COMPLETED_MARKER.to_string()],
        })
    }
    pub fn new_for_test(
        input_object_store_config: &ObjectStoreConfig,
        output_object_store_config: &ObjectStoreConfig,
        interval_s: u64,
    ) -> Result<Self> {
        Ok(DBCheckpointHandler {
            input_object_store: input_object_store_config.make()?,
            output_object_store: output_object_store_config.make()?,
            interval: Duration::from_secs(interval_s),
            gc_markers: vec![UPLOAD_COMPLETED_MARKER.to_string(), TEST_MARKER.to_string()],
        })
    }
    pub fn start(self) -> Sender<()> {
        let (sender, mut recv) = channel::<()>();
        let mut interval = tokio::time::interval(self.interval);
        tokio::task::spawn(async move {
            info!("DB checkpoint handler loop started");
            loop {
                tokio::select! {
                    _now = interval.tick() => {
                        if let Err(err) = self.upload_db_checkpoint_to_object_store().await {
                            error!("Failed to upload db checkpoint to remote store with err: {:?}", err);
                        }
                    },
                    _ = &mut recv => break,
                }
            }
        });
        sender
    }
    async fn upload_db_checkpoint_to_object_store(&self) -> Result<()> {
        let local_checkpoints_by_epoch = self
            .read_checkpoint_dir(self.input_object_store.clone())
            .await?;
        let remote_checkpoints_by_epoch = self
            .read_checkpoint_dir(self.output_object_store.clone())
            .await?;

        let next_epoch = if let Some((last_epoch, path)) =
            remote_checkpoints_by_epoch.iter().next_back()
        {
            let success_marker = path.child(SUCCESS_MARKER);
            let get_result = self.output_object_store.get(&success_marker).await;
            match get_result {
                Ok(_) => last_epoch + 1,
                Err(Error::NotFound { .. }) => {
                    // delete this path recursively and try uploading the last epoch db checkpoint again
                    delete_recursively(
                        path,
                        self.output_object_store.clone(),
                        NonZeroUsize::new(20).unwrap(),
                    )
                    .await?;
                    *last_epoch
                }
                Err(err) => {
                    return Err(anyhow!("Failed to determine if last checkpoint was uploaded successfully with error: {:?}", err));
                }
            }
        } else {
            0
        };
        let next_db_checkpoint_to_copy = local_checkpoints_by_epoch
            .range((Included(next_epoch), Unbounded))
            .next();
        if let Some((epoch, db_path)) = next_db_checkpoint_to_copy {
            info!("Copying db checkpoint for epoch: {epoch} to remote storage");
            copy_recursively(
                db_path,
                self.input_object_store.clone(),
                self.output_object_store.clone(),
                NonZeroUsize::new(20).unwrap(),
            )
            .await?;
            // Drop marker in the output directory that upload completed successfully
            let bytes = Bytes::from_static(b"success");
            let success_marker = db_path.child(SUCCESS_MARKER);
            put(
                &success_marker,
                bytes.clone(),
                self.output_object_store.clone(),
            )
            .await?;
            // Drop marker in the db checkpoint directory that upload completed
            // This is a signal that it is possible to garbage collect it now (although
            // after state snapshot, gc will also wait on a successful state snapshot
            // done marker)
            for (gc_epoch, gc_path) in &local_checkpoints_by_epoch {
                if *gc_epoch <= *epoch {
                    let upload_completed_marker = gc_path.child(UPLOAD_COMPLETED_MARKER);
                    put(
                        &upload_completed_marker,
                        bytes.clone(),
                        self.input_object_store.clone(),
                    )
                    .await?;
                }
            }
        }
        self.garbage_collect_old_db_checkpoints().await?;
        Ok(())
    }
    async fn garbage_collect_old_db_checkpoints(&self) -> Result<()> {
        let local_checkpoints_by_epoch = self
            .read_checkpoint_dir(self.input_object_store.clone())
            .await?;
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
                    delete_recursively(
                        path,
                        self.input_object_store.clone(),
                        NonZeroUsize::new(20).unwrap(),
                    )
                    .await?;
                }
                Err(_) => {
                    debug!("Not ready for deletion yet: {path}");
                }
            }
        }
        Ok(())
    }
    async fn read_checkpoint_dir(&self, store: Arc<DynObjectStore>) -> Result<BTreeMap<u32, Path>> {
        let mut checkpoints_by_epoch = BTreeMap::new();
        let entries = store.list_with_delimiter(None).await?;
        for entry in entries.common_prefixes {
            if let Some(filename) = entry.filename() {
                if !filename.starts_with("epoch_") {
                    continue;
                }
                let epoch = filename
                    .split_once('_')
                    .context("Failed to split dir name")
                    .map(|(_, epoch)| epoch.parse::<u32>())??;
                checkpoints_by_epoch.insert(epoch, entry);
            }
        }
        Ok(checkpoints_by_epoch)
    }
}

#[cfg(test)]
mod tests {
    use crate::db_checkpoint_handler::{
        DBCheckpointHandler, SUCCESS_MARKER, TEST_MARKER, UPLOAD_COMPLETED_MARKER,
    };
    use std::fs;
    use sui_storage::object_store::{ObjectStoreConfig, ObjectStoreType};
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
        let db_checkpoint_handler =
            DBCheckpointHandler::new_for_test(&input_store_config, &output_store_config, 10)?;
        db_checkpoint_handler
            .upload_db_checkpoint_to_object_store()
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
        let db_checkpoint_handler =
            DBCheckpointHandler::new_for_test(&input_store_config, &output_store_config, 10)?;

        fs::create_dir(&local_epoch0_checkpoint)?;
        let file1 = local_epoch0_checkpoint.join("file1");
        fs::write(file1, b"Lorem ipsum")?;
        let file2 = local_epoch0_checkpoint.join("file2");
        fs::write(file2, b"Lorem ipsum")?;
        let nested_dir = local_epoch0_checkpoint.join("data");
        fs::create_dir(&nested_dir)?;
        let file3 = nested_dir.join("file3");
        fs::write(file3, b"Lorem ipsum")?;

        db_checkpoint_handler
            .upload_db_checkpoint_to_object_store()
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
        db_checkpoint_handler
            .upload_db_checkpoint_to_object_store()
            .await?;
        assert!(remote_epoch0_checkpoint.join("file1").exists());
        assert!(remote_epoch0_checkpoint.join("file2").exists());
        assert!(remote_epoch0_checkpoint.join("data").join("file3").exists());
        assert!(remote_epoch0_checkpoint.join(SUCCESS_MARKER).exists());
        assert!(local_epoch0_checkpoint
            .join(UPLOAD_COMPLETED_MARKER)
            .exists());

        db_checkpoint_handler
            .upload_db_checkpoint_to_object_store()
            .await?;
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
}
