// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Error;
use diesel_async::RunQueryDsl;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use object_store::path::Path;
use tokio::sync::Mutex;
use tracing::{debug, info};

use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_core::authority::authority_store_tables::LiveObject;
use sui_field_count::FieldCount;
use sui_indexer_alt_framework::task::TrySpawnStreamExt;
use sui_indexer_alt_schema::objects::StoredObjInfo;
use sui_indexer_alt_schema::schema::obj_info;
use sui_pg_db::Db;
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
            NonZeroUsize::new(args.concurrency).unwrap(),
            m,
            true, // skip_reset_local_store
        )
        .await?;
        let db = Db::for_write(args.db_args.clone()).await?;

        Ok(Self {
            restore_args: args.clone(),
            snapshot_reader,
            db,
            next_checkpoint_after_epoch,
        })
    }

    pub async fn restore(&mut self) -> Result<(), Error> {
        info!(
            epoch = self.restore_args.start_epoch,
            "Starting snapshot restore"
        );
        let (sha3_digests, num_part_files) = self.snapshot_reader.compute_checksum().await?;
        let (input_files, epoch_dir, remote_object_store, _concurrency) =
            self.snapshot_reader.export_metadata().await?;
        let owned_input_files: Vec<(u32, (u32, FileMetadata))> = input_files
            .into_iter()
            .map(|(bucket, (part_num, metadata))| (*bucket, (part_num, metadata.clone())))
            .collect();
        info!("Start snapshot restore.");
        self.restore_object_infos(
            owned_input_files,
            epoch_dir,
            remote_object_store,
            sha3_digests,
            num_part_files,
        )
        .await?;
        info!(
            epoch = self.restore_args.start_epoch,
            "Finished snapshot restore"
        );
        Ok(())
    }

    async fn restore_object_infos(
        &self,
        input_files: Vec<(u32, (u32, FileMetadata))>,
        epoch_dir: Path,
        remote_object_store: Arc<dyn ObjectStoreGetExt>,
        sha3_digests: Arc<Mutex<DigestByBucketAndPartition>>,
        num_part_files: usize,
    ) -> anyhow::Result<()> {
        let move_object_progress_bar = Arc::new(self.snapshot_reader.get_multi_progress().add(
            ProgressBar::new(num_part_files as u64).with_style(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] {wide_bar} {pos} out of {len} move object files restored ({msg})",
                )
                .unwrap(),
            ),
        ));

        futures::stream::iter(input_files)
            .try_for_each_spawned(
                self.restore_args.concurrency,
                |(bucket, (part_num, file_metadata))| {
                    let epoch_dir = epoch_dir.clone();
                    let remote_object_store = remote_object_store.clone();
                    let sha3_digests = sha3_digests.clone();
                    let bar = move_object_progress_bar.clone();
                    let db = self.db.clone();
                    let next_cp = self.next_checkpoint_after_epoch;

                    async move {
                        debug!(
                            bucket = bucket,
                            part_num = part_num,
                            "Start downloading move object file"
                        );
                        let mut conn = db.connect().await?;
                        let (bytes, _) = download_bytes(
                            remote_object_store,
                            &file_metadata,
                            epoch_dir,
                            sha3_digests,
                            &&bucket,
                            &part_num,
                            Some(512), // max_timeout_secs
                        )
                        .await;
                        debug!(
                            bucket = bucket,
                            part_num = part_num,
                            "Finished downloading move object file"
                        );
                        let object_infos = LiveObjectIter::new(&file_metadata, bytes.clone())?
                            .filter_map(|object| match object {
                                LiveObject::Normal(obj) => {
                                    Some(StoredObjInfo::from_object(&obj, next_cp as i64))
                                }
                                LiveObject::Wrapped(_) => None,
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        let num_object_infos = object_infos.len();
                        // NOTE: chunk to avoid hitting the PG limit
                        let chunk_size: usize = i16::MAX as usize / StoredObjInfo::FIELD_COUNT;
                        debug!(
                            bucket = bucket,
                            part_num = part_num,
                            num_object_infos = num_object_infos,
                            "Start inserting object infos"
                        );
                        for chunk in object_infos.chunks(chunk_size) {
                            diesel::insert_into(obj_info::table)
                                .values(chunk)
                                .on_conflict_do_nothing()
                                .execute(&mut conn)
                                .await?;
                        }
                        debug!(
                            bucket = bucket,
                            part_num = part_num,
                            num_object_infos = num_object_infos,
                            "Finished inserting object infos"
                        );
                        bar.inc(1);
                        bar.set_message(format!("Bucket: {}, Part: {}", bucket, part_num));
                        Ok::<(), anyhow::Error>(())
                    }
                },
            )
            .await?;
        Ok(())
    }
}
