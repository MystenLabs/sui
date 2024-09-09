// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use futures::future::{AbortHandle, AbortRegistration, Abortable};
use futures::{StreamExt, TryStreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use object_store::path::Path;
use tokio::sync::Mutex;
use tracing::info;

use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_core::authority::authority_store_tables::LiveObject;
use sui_snapshot::reader::{download_bytes, LiveObjectIter, StateSnapshotReaderV1};
use sui_snapshot::FileMetadata;
use sui_storage::object_store::ObjectStoreGetExt;
use sui_types::accumulator::Accumulator;

use crate::config::RestoreConfig;
use crate::errors::IndexerError;
use crate::handlers::TransactionObjectChangesToCommit;
use crate::restorer::archives::read_next_checkpoint_after_epoch;
use crate::store::{indexer_store::IndexerStore, PgIndexerStore};
use crate::types::{IndexedDeletedObject, IndexedObject};

pub type DigestByBucketAndPartition = BTreeMap<u32, BTreeMap<u32, [u8; 32]>>;
pub type SnapshotChecksums = (DigestByBucketAndPartition, Accumulator);
pub type Sha3DigestType = Arc<Mutex<BTreeMap<u32, BTreeMap<u32, [u8; 32]>>>>;

pub struct IndexerFormalSnapshotRestorer {
    store: PgIndexerStore,
    reader: StateSnapshotReaderV1,
    restore_config: RestoreConfig,
    next_checkpoint_after_epoch: u64,
}

impl IndexerFormalSnapshotRestorer {
    pub async fn new(
        store: PgIndexerStore,
        restore_config: RestoreConfig,
    ) -> Result<Self, IndexerError> {
        let remote_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::S3),
            aws_endpoint: Some(restore_config.s3_endpoint.clone()),
            aws_virtual_hosted_style_request: true,
            object_store_connection_limit: restore_config.object_store_concurrent_limit,
            no_sign_request: true,
            ..Default::default()
        };

        let base_path = PathBuf::from(restore_config.gcs_snapshot_dir.clone());
        let snapshot_dir = base_path.join("snapshot");
        if snapshot_dir.exists() {
            fs::remove_dir_all(snapshot_dir.clone()).unwrap();
            info!(
                "Deleted all files from snapshot directory: {:?}",
                snapshot_dir
            );
        } else {
            fs::create_dir(snapshot_dir.clone()).unwrap();
            info!("Created snapshot directory: {:?}", snapshot_dir);
        }

        let local_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(snapshot_dir.clone().to_path_buf()),
            ..Default::default()
        };

        let m = MultiProgress::new();
        let reader = StateSnapshotReaderV1::new(
            restore_config.start_epoch,
            &remote_store_config,
            &local_store_config,
            usize::MAX,
            NonZeroUsize::new(restore_config.object_store_concurrent_limit).unwrap(),
            m.clone(),
        )
        .await
        .unwrap_or_else(|err| panic!("Failed to create reader: {}", err));
        info!(
            "Initialized formal snapshot reader at epoch {}",
            restore_config.start_epoch
        );

        let next_checkpoint_after_epoch = read_next_checkpoint_after_epoch(
            restore_config.gcs_cred_path.clone(),
            Some(restore_config.gcs_archive_bucket.clone()),
            restore_config.start_epoch,
        )
        .await?;

        Ok(Self {
            store,
            reader,
            next_checkpoint_after_epoch,
            restore_config: restore_config.clone(),
        })
    }

    pub async fn restore(&mut self) -> Result<(), IndexerError> {
        let (sha3_digests, num_part_files) = self.reader.compute_checksum().await?;
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();
        let (input_files, epoch_dir, remote_object_store, _concurrency) =
            self.reader.export_metadata().await?;
        self.restore_move_objects(
            abort_registration,
            input_files,
            epoch_dir,
            remote_object_store,
            sha3_digests,
            num_part_files,
        )
        .await?;
        info!("Finished restoring move objects");
        Ok(())
    }

    async fn restore_move_objects(
        &self,
        abort_registration: AbortRegistration,
        input_files: Vec<(&u32, (u32, FileMetadata))>,
        epoch_dir: Path,
        remote_object_store: Arc<dyn ObjectStoreGetExt>,
        sha3_digests: Arc<Mutex<DigestByBucketAndPartition>>,
        num_part_files: usize,
    ) -> std::result::Result<(), anyhow::Error> {
        let move_object_progress_bar = self.reader.get_multi_progress().add(
            ProgressBar::new(num_part_files as u64).with_style(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] {wide_bar} {pos} out of {len} move object files restored ({msg})",
                )
                .unwrap(),
            ),
        );
        Abortable::new(
            async move {
                futures::stream::iter(input_files.iter())
                    .map(|(bucket, (part_num, file_metadata))| {
                        let epoch_dir_clone = epoch_dir.clone();
                        let remote_object_store_clone = remote_object_store.clone();
                        let sha3_digests_clone = sha3_digests.clone();
                        let store_clone = self.store.clone();
                        let bar_clone = move_object_progress_bar.clone();
                        async move {
                            let object_file_path = file_metadata.file_path(&epoch_dir_clone);
                            let (bytes, _) = download_bytes(
                                remote_object_store_clone,
                                file_metadata,
                                epoch_dir_clone,
                                sha3_digests_clone,
                                bucket,
                                part_num,
                                Some(self.restore_config.object_store_max_timeout_secs),
                            )
                            .await;
                            info!(
                                "Finished downloading move object file {:?}",
                                object_file_path,
                            );
                            let mut move_objects = vec![];
                            let _result: Result<(), anyhow::Error> =
                                LiveObjectIter::new(file_metadata, bytes.clone()).map(|obj_iter| {
                                    for object in obj_iter {
                                        match object {
                                            LiveObject::Normal(obj) => {
                                                if !obj.is_package() {
                                                    let indexed_object = IndexedObject::from_object(
                                                        self.next_checkpoint_after_epoch,
                                                        obj,
                                                        None,
                                                    );
                                                    move_objects.push(indexed_object);
                                                }
                                            }
                                            LiveObject::Wrapped(_) => {}
                                        }
                                    }
                                });
                            let live_obj_cnt = move_objects.len();
                            let object_changes = TransactionObjectChangesToCommit {
                                changed_objects: move_objects.clone(),
                                deleted_objects: vec![],
                            };
                            info!("Start persisting {} objects to objects table from {}", live_obj_cnt, object_file_path);
                            store_clone
                                .persist_objects(vec![object_changes])
                                .await
                                .expect("Failed to persist to objects from restore");
                            info!("Finished persisting {} objects to objects table from {}", live_obj_cnt, object_file_path);
                            let objects_snapshot_changes = TransactionObjectChangesToCommit {
                                changed_objects: move_objects.clone(),
                                deleted_objects: vec![],
                            };
                            info!("Finished persisting {} objects to objects snapshot from {}", live_obj_cnt, object_file_path);

                            store_clone
                                .persist_objects_snapshot(vec![objects_snapshot_changes])
                                .await
                                .expect("Failed to persist objects snapshot");
                            bar_clone.inc(1);
                            bar_clone.set_message(format!("Restored {} live move objects from {}", live_obj_cnt, object_file_path));
                            Ok::<(), anyhow::Error>(())
                        }
                    })
                    .boxed()
                    .buffer_unordered(self.restore_config.object_store_concurrent_limit)
                    .try_for_each(|()| futures::future::ready(Ok(())))
                    .await
            },
            abort_registration,
        )
        .await?
    }
}
