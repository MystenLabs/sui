// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use diesel::PgConnection;
use futures::future::{AbortHandle, AbortRegistration, Abortable};
use futures::{StreamExt, TryStreamExt};
use indicatif::MultiProgress;
use object_store::path::Path;
use secrecy::ExposeSecret;
use tokio::sync::Mutex;
use tracing::info;

use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_core::authority::authority_store_tables::LiveObject;
use sui_snapshot::reader::{download_bytes, LiveObjectIter, StateSnapshotReaderV1};
use sui_snapshot::FileMetadata;
use sui_storage::object_store::ObjectStoreGetExt;
use sui_types::accumulator::Accumulator;

use crate::db::new_connection_pool;
use crate::errors::IndexerError;
use crate::handlers::TransactionObjectChangesToCommit;
use crate::metrics::IndexerMetrics;
use crate::store::{indexer_store::IndexerStore, PgIndexerStore};
use crate::types::{IndexedDeletedObject, IndexedObject};
use crate::IndexerConfig;

pub type DigestByBucketAndPartition = BTreeMap<u32, BTreeMap<u32, [u8; 32]>>;
pub type SnapshotChecksums = (DigestByBucketAndPartition, Accumulator);
pub type Sha3DigestType = Arc<Mutex<BTreeMap<u32, BTreeMap<u32, [u8; 32]>>>>;

pub struct IndexerFormalSnapshotRestorer {
    reader: StateSnapshotReaderV1,
    store: PgIndexerStore<PgConnection>,
    next_checkpoint_after_epoch: u64,
}

pub struct IndexerFormalSnapshotRestorerConfig {
    pub cred_path: String,
    pub base_path: String,
    pub epoch: u64,
    pub next_checkpoint_after_epoch: u64,
}

impl IndexerFormalSnapshotRestorer {
    pub async fn new(
        indexer_config: IndexerConfig,
        indexer_metrics: IndexerMetrics,
        restorer_config: IndexerFormalSnapshotRestorerConfig,
    ) -> Result<Self, IndexerError> {
        let db_url_secret = indexer_config.get_db_url().map_err(|e| {
            IndexerError::PgPoolConnectionError(format!(
                "Failed parsing database url with error {:?}",
                e
            ))
        })?;
        let db_url = db_url_secret.expose_secret();
        let blocking_cp = new_connection_pool::<PgConnection>(db_url, None).map_err(|e| {
            tracing::error!(
                "Failed creating Postgres connection pool with error {:?}",
                e
            );
            e
        })?;
        let store = PgIndexerStore::<PgConnection>::new(blocking_cp, indexer_metrics.clone());
        info!("Finished creating PG indexer store.");

        let snapshot_bucket = indexer_config.formal_snapshot_bucket;
        let remote_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::GCS),
            bucket: Some(snapshot_bucket),
            google_service_account: Some(restorer_config.cred_path),
            object_store_connection_limit: 200,
            no_sign_request: false,
            ..Default::default()
        };

        let base_path = PathBuf::from(restorer_config.base_path);
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
            restorer_config.epoch,
            &remote_store_config,
            &local_store_config,
            usize::MAX,
            NonZeroUsize::new(100_usize).unwrap(),
            m.clone(),
        )
        .await
        .unwrap_or_else(|err| panic!("Failed to create reader: {}", err));
        info!(
            "Initialized formal snapshot reader at epoch {}",
            restorer_config.epoch
        );
        Ok(Self {
            store,
            reader,
            next_checkpoint_after_epoch: restorer_config.next_checkpoint_after_epoch,
        })
    }

    pub async fn restore(&mut self) -> Result<(), IndexerError> {
        let (sha3_digests, _num_part_files) = self.reader.compute_checksum().await?;
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();
        let (input_files, epoch_dir, remote_object_store, concurrency) =
            self.reader.export_metadata().await?;
        restore_package_objects(
            abort_registration,
            input_files.clone(),
            epoch_dir.clone(),
            remote_object_store.clone(),
            concurrency,
            sha3_digests.clone(),
            self.store.clone(),
            self.next_checkpoint_after_epoch,
        )
        .await?;
        info!("finished restoring packages");
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();
        restore_move_objects(
            abort_registration,
            input_files,
            epoch_dir,
            remote_object_store,
            concurrency,
            sha3_digests,
            self.store.clone(),
            self.next_checkpoint_after_epoch,
        )
        .await?;
        Ok(())
    }
}

async fn restore_package_objects(
    abort_registration: AbortRegistration,
    input_files: Vec<(&u32, (u32, FileMetadata))>,
    epoch_dir: Path,
    remote_object_store: Arc<dyn ObjectStoreGetExt>,
    concurrency: usize,
    sha3_digests: Arc<Mutex<DigestByBucketAndPartition>>,
    store: PgIndexerStore<PgConnection>,
    next_cp: u64,
) -> std::result::Result<(), anyhow::Error> {
    Abortable::new(
        async move {
            futures::stream::iter(input_files.iter())
                .map(|(bucket, (part_num, file_metadata))| {
                    let epoch_dir = epoch_dir.clone();
                    let remote_object_store_clone = remote_object_store.clone();
                    let sha3_digests_clone = sha3_digests.clone();
                    let store_clone = store.clone();
                    async move {
                        let (bytes, _) = download_bytes(
                            remote_object_store_clone,
                            file_metadata,
                            epoch_dir,
                            sha3_digests_clone,
                            bucket,
                            part_num,
                        )
                        .await;
                        let mut package_objects = vec![];
                        let _result: Result<(), anyhow::Error> =
                            LiveObjectIter::new(file_metadata, bytes.clone()).map(|obj_iter| {
                                for object in obj_iter {
                                    if let LiveObject::Normal(obj) = object {
                                        if obj.is_package() {
                                            let indexed_object =
                                                IndexedObject::from_object(next_cp, obj, None);
                                            package_objects.push(indexed_object);
                                        }
                                    }
                                }
                            });
                        let package_object_cnt = package_objects.len();
                        let object_changes = TransactionObjectChangesToCommit {
                            changed_objects: package_objects,
                            deleted_objects: vec![],
                        };
                        store_clone
                            .persist_objects(vec![object_changes.clone()])
                            .await
                            .expect("Failed to persist package objects");
                        store_clone
                            .backfill_objects_snapshot(vec![object_changes.clone()])
                            .await
                            .expect("Failed to backfill package objects snapshot");
                        info!("Restored {} package objects", package_object_cnt);
                        Ok::<(), anyhow::Error>(())
                    }
                })
                .boxed()
                .buffer_unordered(concurrency)
                .try_for_each(|()| futures::future::ready(Ok(())))
                .await
        },
        abort_registration,
    )
    .await?
}

async fn restore_move_objects(
    abort_registration: AbortRegistration,
    input_files: Vec<(&u32, (u32, FileMetadata))>,
    epoch_dir: Path,
    remote_object_store: Arc<dyn ObjectStoreGetExt>,
    concurrency: usize,
    sha3_digests: Arc<Mutex<DigestByBucketAndPartition>>,
    store: PgIndexerStore<PgConnection>,
    next_cp: u64,
) -> std::result::Result<(), anyhow::Error> {
    Abortable::new(
        async move {
            futures::stream::iter(input_files.iter())
                .map(|(bucket, (part_num, file_metadata))| {
                    let epoch_dir_clone = epoch_dir.clone();
                    let remote_object_store_clone = remote_object_store.clone();
                    let sha3_digests_clone = sha3_digests.clone();
                    let store_clone = store.clone();
                    async move {
                        // Download object file with retries check times
                        let (bytes, _) = download_bytes(
                            remote_object_store_clone,
                            file_metadata,
                            epoch_dir_clone,
                            sha3_digests_clone,
                            bucket,
                            part_num,
                        )
                        .await;
                        let mut move_objects = vec![];
                        let mut wrapped_or_deleted_move_objects = vec![];
                        let _result: Result<(), anyhow::Error> =
                            LiveObjectIter::new(file_metadata, bytes.clone()).map(|obj_iter| {
                                for object in obj_iter {
                                    match object {
                                        LiveObject::Normal(obj) => {
                                            if !obj.is_package() {
                                                let indexed_object =
                                                    IndexedObject::from_object(next_cp, obj, None);
                                                move_objects.push(indexed_object);
                                            }
                                        }
                                        LiveObject::Wrapped(obj_key) => {
                                            wrapped_or_deleted_move_objects.push(
                                                IndexedDeletedObject {
                                                    object_id: obj_key.0,
                                                    object_version: obj_key.1.value(),
                                                    checkpoint_sequence_number: next_cp,
                                                },
                                            );
                                        }
                                    }
                                }
                            });
                        let live_obj_cnt = move_objects.len();
                        let wrapped_or_deleted_obj_cnt = wrapped_or_deleted_move_objects.len();
                        let object_changes = TransactionObjectChangesToCommit {
                            changed_objects: move_objects.clone(),
                            deleted_objects: vec![],
                        };
                        let objects_snapshot_changes = TransactionObjectChangesToCommit {
                            changed_objects: move_objects.clone(),
                            deleted_objects: wrapped_or_deleted_move_objects,
                        };
                        store_clone
                            .persist_objects(vec![object_changes])
                            .await
                            .expect("Failed to persist to objects from restore");
                        store_clone
                            .backfill_objects_snapshot(vec![objects_snapshot_changes])
                            .await
                            .expect("Failed to backfill objects snapshot");
                        info!(
                            "Restored {} live Move objects and {} wrapped or deleted objects",
                            live_obj_cnt, wrapped_or_deleted_obj_cnt
                        );
                        Ok::<(), anyhow::Error>(())
                    }
                })
                .boxed()
                .buffer_unordered(concurrency)
                .try_for_each(|()| futures::future::ready(Ok(())))
                .await
        },
        abort_registration,
    )
    .await?
}
