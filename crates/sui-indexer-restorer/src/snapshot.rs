// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Error;
use diesel_async::RunQueryDsl;
use futures::future::{AbortHandle, AbortRegistration, Abortable};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use object_store::path::Path;
use sui_indexer_alt::models::objects::{create_stored_obj_info, StoredObjInfo};
use tokio::sync::{Mutex, Semaphore};
use tokio::task;
use tracing::info;
use url::Url;

use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_core::authority::authority_store_tables::LiveObject;
use sui_field_count::FieldCount;
use sui_indexer_alt::db::{Db, DbArgs};
use sui_indexer_alt::schema::obj_info;
use sui_snapshot::{
    reader::{download_bytes, LiveObjectIter, StateSnapshotReaderV1},
    FileMetadata,
};
use sui_storage::object_store::ObjectStoreGetExt;

use crate::Args;

pub type DigestByBucketAndPartition = BTreeMap<u32, BTreeMap<u32, [u8; 32]>>;

pub struct SnapshotRestorer {
    pub restore_args: Args,
    pub next_checkpoint_after_epoch: u64,
    pub snapshot_reader: StateSnapshotReaderV1,
    pub db: Db,
}

impl SnapshotRestorer {
    pub async fn new(args: &Args, next_checkpoint_after_epoch: u64) -> Result<Self, Error> {
        let remote_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::S3),
            aws_endpoint: Some(args.endpoint.clone()),
            aws_virtual_hosted_style_request: true,
            object_store_connection_limit: args.concurrency,
            no_sign_request: true,
            ..Default::default()
        };

        let local_path = PathBuf::from(&args.snapshot_local_dir);
        let snapshot_dir = local_path.join("snapshot");
        let local_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(snapshot_dir.clone().to_path_buf()),
            ..Default::default()
        };

        let m = MultiProgress::new();
        let snapshot_reader = StateSnapshotReaderV1::new(
            args.start_epoch,
            &remote_store_config,
            &local_store_config,
            usize::MAX,
            NonZeroUsize::new(args.concurrency).unwrap(),
            m,
            true, // skip_reset_local_store
        )
        .await?;

        let database_url = Url::parse(&args.database_url)?;
        let db_args = DbArgs::with_url(database_url);
        let db = Db::new(db_args).await?;

        Ok(Self {
            restore_args: args.clone(),
            snapshot_reader,
            db,
            next_checkpoint_after_epoch,
        })
    }

    pub async fn restore(&mut self) -> Result<(), Error> {
        info!(
            "Starting snapshot restore from epoch {}",
            self.restore_args.start_epoch
        );
        let (sha3_digests, num_part_files) = self.snapshot_reader.compute_checksum().await?;
        let (_abort_handle, abort_registration) = AbortHandle::new_pair();
        let (input_files, epoch_dir, remote_object_store, _concurrency) =
            self.snapshot_reader.export_metadata().await?;
        let owned_input_files: Vec<(u32, (u32, FileMetadata))> = input_files
            .into_iter()
            .map(|(bucket, (part_num, metadata))| (*bucket, (part_num, metadata.clone())))
            .collect();

        self.restore_object_infos(
            abort_registration,
            owned_input_files,
            epoch_dir,
            remote_object_store,
            sha3_digests,
            num_part_files,
            self.restore_args.concurrency,
        )
        .await?;
        info!(
            "Finished snapshot restore from epoch {}",
            self.restore_args.start_epoch
        );
        Ok(())
    }

    async fn restore_object_infos(
        &self,
        abort_registration: AbortRegistration,
        input_files: Vec<(u32, (u32, FileMetadata))>,
        epoch_dir: Path,
        remote_object_store: Arc<dyn ObjectStoreGetExt>,
        sha3_digests: Arc<Mutex<DigestByBucketAndPartition>>,
        num_part_files: usize,
        concurrency: usize,
    ) -> std::result::Result<(), anyhow::Error> {
        let move_object_progress_bar = Arc::new(self.snapshot_reader.get_multi_progress().add(
            ProgressBar::new(num_part_files as u64).with_style(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] {wide_bar} {pos} out of {len} move object files restored ({msg})",
                )
                .unwrap(),
            ),
        ));

        Abortable::new(
            async move {
                let sema_limit = Arc::new(Semaphore::new(concurrency));
                let mut restore_tasks = vec![];

                for (bucket, (part_num, file_metadata)) in input_files.into_iter() {
                    let sema_limit_clone = sema_limit.clone();
                    let epoch_dir_clone = epoch_dir.clone();
                    let remote_object_store_clone = remote_object_store.clone();
                    let sha3_digests_clone = sha3_digests.clone();
                    let bar_clone = move_object_progress_bar.clone();
                    let db = self.db.clone();
                    let next_cp = self.next_checkpoint_after_epoch;

                    let restore_task = task::spawn(async move {
                        let mut conn = db.connect().await?;
                        let _permit = sema_limit_clone.acquire().await.unwrap();
                        let object_file_path = file_metadata.file_path(&epoch_dir_clone);
                        let (bytes, _) = download_bytes(
                            remote_object_store_clone,
                            &file_metadata,
                            epoch_dir_clone,
                            sha3_digests_clone,
                            &&bucket,
                            &part_num,
                            Some(512),
                        )
                        .await;
                        info!(
                            "Finished downloading move object file {:?}",
                            object_file_path
                        );
                        let mut object_infos = vec![];
                        let _result: Result<(), anyhow::Error> =
                            LiveObjectIter::new(&file_metadata, bytes.clone()).map(|obj_iter| {
                                for object in obj_iter {
                                    match object {
                                        LiveObject::Normal(obj) => {
                                            let object_info = create_stored_obj_info(
                                                obj.id(),
                                                next_cp as i64,
                                                &obj,
                                            );
                                            object_infos.push(object_info);
                                        }
                                        LiveObject::Wrapped(_) => {}
                                    }
                                }
                            });
                        let object_infos =
                            object_infos.into_iter().collect::<Result<Vec<_>, _>>()?;

                        // NOTE: chunk to avoid hitting the PG limit
                        let object_info_count = object_infos.len();
                        let chunk_size: usize = i16::MAX as usize / StoredObjInfo::FIELD_COUNT;
                        for chunk in object_infos.chunks(chunk_size) {
                            diesel::insert_into(obj_info::table)
                                .values(chunk)
                                .on_conflict_do_nothing()
                                .execute(&mut conn)
                                .await?;
                        }
                        bar_clone.inc(1);
                        info!("Restored {} move objects", object_info_count);
                        Ok::<(), anyhow::Error>(())
                    });
                    restore_tasks.push(restore_task);
                }

                let restore_task_results = futures::future::join_all(restore_tasks).await;
                for restore_task_result in restore_task_results {
                    restore_task_result??;
                }
                Ok(())
            },
            abort_registration,
        )
        .await?
    }
}
