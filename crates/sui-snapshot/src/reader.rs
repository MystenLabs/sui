// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    FileMetadata, FileType, Manifest, MAGIC_BYTES, MANIFEST_FILE_MAGIC, OBJECT_FILE_MAGIC,
    OBJECT_ID_BYTES, OBJECT_REF_BYTES, REFERENCE_FILE_MAGIC, SEQUENCE_NUM_BYTES, SHA3_BYTES,
};
use anyhow::{anyhow, Context, Result};
use byteorder::{BigEndian, ReadBytesExt};
use bytes::{Buf, Bytes};
use fastcrypto::hash::MultisetHash;
use fastcrypto::hash::{HashFunction, Sha3_256};
use futures::future::{AbortRegistration, Abortable};
use futures::{StreamExt, TryStreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use integer_encoding::VarIntReader;
use object_store::path::Path;
use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use sui_config::object_storage_config::ObjectStoreConfig;
use sui_core::authority::authority_store_tables::{AuthorityPerpetualTables, LiveObject};
use sui_core::authority::AuthorityStore;
use sui_indexer_alt_framework::task::TrySpawnStreamExt;
use sui_storage::blob::{Blob, BlobEncoding};
use sui_storage::object_store::http::HttpDownloaderBuilder;
use sui_storage::object_store::util::{copy_file, copy_files, path_to_filesystem};
use sui_storage::object_store::{ObjectStoreGetExt, ObjectStoreListExt, ObjectStorePutExt};
use sui_types::accumulator::Accumulator;
use sui_types::base_types::{ObjectDigest, ObjectID, ObjectRef, SequenceNumber};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio::time::Instant;
use tracing::{error, info};

pub type SnapshotChecksums = (DigestByBucketAndPartition, Accumulator);
pub type DigestByBucketAndPartition = BTreeMap<u32, BTreeMap<u32, [u8; 32]>>;
pub type Sha3DigestType = Arc<Mutex<BTreeMap<u32, BTreeMap<u32, [u8; 32]>>>>;
#[derive(Clone)]
pub struct StateSnapshotReaderV1 {
    epoch: u64,
    local_staging_dir_root: PathBuf,
    remote_object_store: Arc<dyn ObjectStoreGetExt>,
    local_object_store: Arc<dyn ObjectStorePutExt>,
    ref_files: BTreeMap<u32, BTreeMap<u32, FileMetadata>>,
    object_files: BTreeMap<u32, BTreeMap<u32, FileMetadata>>,
    m: MultiProgress,
    concurrency: usize,
}

impl StateSnapshotReaderV1 {
    pub async fn new(
        epoch: u64,
        remote_store_config: &ObjectStoreConfig,
        local_store_config: &ObjectStoreConfig,
        download_concurrency: NonZeroUsize,
        m: MultiProgress,
        skip_reset_local_store: bool,
    ) -> Result<Self> {
        let epoch_dir = format!("epoch_{}", epoch);
        let remote_object_store = if remote_store_config.no_sign_request {
            remote_store_config.make_http()?
        } else {
            remote_store_config.make().map(Arc::new)?
        };
        let local_object_store: Arc<dyn ObjectStorePutExt> =
            local_store_config.make().map(Arc::new)?;
        let local_object_store_list: Arc<dyn ObjectStoreListExt> =
            local_store_config.make().map(Arc::new)?;
        let local_staging_dir_root = local_store_config
            .directory
            .as_ref()
            .context("No directory specified")?
            .clone();
        if !skip_reset_local_store {
            let local_epoch_dir_path = local_staging_dir_root.join(&epoch_dir);
            if local_epoch_dir_path.exists() {
                fs::remove_dir_all(&local_epoch_dir_path)?;
            }
            fs::create_dir_all(&local_epoch_dir_path)?;
        }
        // Download MANIFEST first
        let manifest_file_path = Path::from(epoch_dir.clone()).child("MANIFEST");
        copy_file(
            &manifest_file_path,
            &manifest_file_path,
            &remote_object_store,
            &local_object_store,
        )
        .await?;
        let manifest = Self::read_manifest(path_to_filesystem(
            local_staging_dir_root.clone(),
            &manifest_file_path,
        )?)?;
        let snapshot_version = manifest.snapshot_version();
        if snapshot_version != 1u8 {
            return Err(anyhow!("Unexpected snapshot version: {}", snapshot_version));
        }
        if manifest.address_length() as usize > ObjectID::LENGTH {
            return Err(anyhow!(
                "Max possible address length is: {}",
                ObjectID::LENGTH
            ));
        }
        if manifest.epoch() != epoch {
            return Err(anyhow!("Download manifest is not for epoch: {}", epoch,));
        }
        let mut object_files = BTreeMap::new();
        let mut ref_files = BTreeMap::new();
        for file_metadata in manifest.file_metadata() {
            match file_metadata.file_type {
                FileType::Object => {
                    let entry = object_files
                        .entry(file_metadata.bucket_num)
                        .or_insert_with(BTreeMap::new);
                    entry.insert(file_metadata.part_num, file_metadata.clone());
                }
                FileType::Reference => {
                    let entry = ref_files
                        .entry(file_metadata.bucket_num)
                        .or_insert_with(BTreeMap::new);
                    entry.insert(file_metadata.part_num, file_metadata.clone());
                }
            }
        }
        let epoch_dir_path = Path::from(epoch_dir);
        let files: Vec<Path> = ref_files
            .values()
            .flat_map(|entry| {
                let files: Vec<_> = entry
                    .values()
                    .map(|file_metadata| file_metadata.file_path(&epoch_dir_path))
                    .collect();
                files
            })
            .collect();

        let files_to_download = if skip_reset_local_store {
            let mut list_stream = local_object_store_list
                .list_objects(Some(&epoch_dir_path))
                .await;
            let mut existing_files = std::collections::HashSet::new();
            while let Some(Ok(meta)) = list_stream.next().await {
                existing_files.insert(meta.location);
            }
            let mut missing_files = Vec::new();
            for file in &files {
                if !existing_files.contains(file) {
                    missing_files.push(file.clone());
                }
            }
            missing_files
        } else {
            files
        };
        let progress_bar = m.add(
            ProgressBar::new(files_to_download.len() as u64).with_style(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] {wide_bar} {pos} out of {len} missing .ref files done ({msg})",
                )
                .unwrap(),
            ),
        );
        copy_files(
            &files_to_download,
            &files_to_download,
            &remote_object_store,
            &local_object_store,
            download_concurrency,
            Some(progress_bar.clone()),
        )
        .await?;
        progress_bar.finish_with_message("Missing ref files download complete");
        Ok(StateSnapshotReaderV1 {
            epoch,
            local_staging_dir_root,
            remote_object_store,
            local_object_store,
            ref_files,
            object_files,
            m,
            concurrency: download_concurrency.get(),
        })
    }

    pub async fn read(
        &mut self,
        perpetual_db: &AuthorityPerpetualTables,
        abort_registration: AbortRegistration,
        sender: Option<tokio::sync::mpsc::Sender<(Accumulator, u64)>>,
    ) -> Result<()> {
        // This computes and stores the sha3 digest of object references in REFERENCE file for each
        // bucket partition. When downloading objects, we will match sha3 digest of object references
        // per *.obj file against this. We do this so during restore we can pre fetch object
        // references and start building state accumulator and fail early if the state root hash
        // doesn't match but we still need to ensure that objects match references exactly.
        let (sha3_digests, num_part_files) = self.compute_checksum().await?;
        let accum_handle =
            sender.map(|sender| self.spawn_accumulation_tasks(sender, num_part_files));
        self.sync_live_objects(perpetual_db, abort_registration, sha3_digests)
            .await?;
        if let Some(handle) = accum_handle {
            handle.await?;
        }
        Ok(())
    }

    pub async fn compute_checksum(
        &mut self,
    ) -> Result<(Arc<Mutex<BTreeMap<u32, BTreeMap<u32, [u8; 32]>>>>, usize), anyhow::Error> {
        let sha3_digests: Arc<Mutex<DigestByBucketAndPartition>> =
            Arc::new(Mutex::new(BTreeMap::new()));

        let num_part_files = self
            .ref_files
            .values()
            .map(|part_files| part_files.len())
            .sum::<usize>();

        // Generate checksums
        info!("Computing checksums");
        let checksum_progress_bar = self.m.add(
            ProgressBar::new(num_part_files as u64).with_style(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] {wide_bar} {pos} out of {len} ref files checksummed ({msg})",
                )
                .unwrap(),
            ),
        );

        let ref_files_iter = self.ref_files.clone().into_iter();
        futures::stream::iter(ref_files_iter)
            .flat_map(|(bucket, part_files)| {
                futures::stream::iter(
                    part_files
                        .into_iter()
                        .map(move |(part, part_file)| (bucket, part, part_file)),
                )
            })
            .try_for_each_spawned(self.concurrency, |(bucket, part, _part_file)| {
                let sha3_digests = sha3_digests.clone();
                let object_files = self.object_files.clone();
                let bar = checksum_progress_bar.clone();
                let this = self.clone();

                async move {
                    let ref_iter = this.ref_iter(bucket, part)?;
                    let mut hasher = Sha3_256::default();
                    let mut empty = true;

                    object_files
                        .get(&bucket)
                        .context(format!("No bucket exists for: {bucket}"))?
                        .get(&part)
                        .context(format!("No part exists for bucket: {bucket}, part: {part}"))?;

                    for object_ref in ref_iter {
                        hasher.update(object_ref.2.inner());
                        empty = false;
                    }

                    if !empty {
                        let mut digests = sha3_digests.lock().await;
                        digests
                            .entry(bucket)
                            .or_insert(BTreeMap::new())
                            .entry(part)
                            .or_insert(hasher.finalize().digest);
                    }

                    bar.inc(1);
                    bar.set_message(format!("Bucket: {}, Part: {}", bucket, part));
                    Ok::<(), anyhow::Error>(())
                }
            })
            .await?;
        checksum_progress_bar.finish_with_message("Checksumming complete");
        Ok((sha3_digests, num_part_files))
    }

    fn spawn_accumulation_tasks(
        &self,
        sender: tokio::sync::mpsc::Sender<(Accumulator, u64)>,
        num_part_files: usize,
    ) -> JoinHandle<()> {
        // Spawn accumulation progress bar
        let concurrency = self.concurrency;
        let accum_counter = Arc::new(AtomicU64::new(0));
        let cloned_accum_counter = accum_counter.clone();
        let accum_progress_bar = self.m.add(
             ProgressBar::new(num_part_files as u64).with_style(
                 ProgressStyle::with_template(
                     "[{elapsed_precise}] {wide_bar} {pos} out of {len} ref files accumulated from snapshot ({msg})",
                 )
                 .unwrap(),
             ),
         );
        let cloned_accum_progress_bar = accum_progress_bar.clone();
        tokio::spawn(async move {
            let a_instant = Instant::now();
            loop {
                if cloned_accum_progress_bar.is_finished() {
                    break;
                }
                let num_partitions = cloned_accum_counter.load(Ordering::Relaxed);
                let total_partitions_per_sec =
                    num_partitions as f64 / a_instant.elapsed().as_secs_f64();
                cloned_accum_progress_bar.set_position(num_partitions);
                cloned_accum_progress_bar.set_message(format!(
                    "file partitions per sec: {}",
                    total_partitions_per_sec
                ));
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });

        // spawn accumualation task
        let ref_files = self.ref_files.clone();
        let epoch_dir = self.epoch_dir();
        let local_staging_dir_root = self.local_staging_dir_root.clone();
        tokio::task::spawn(async move {
            let local_staging_dir_root_clone = local_staging_dir_root.clone();
            let epoch_dir_clone = epoch_dir.clone();
            for (bucket, part_files) in ref_files.clone().iter() {
                futures::stream::iter(part_files.iter())
                    .map(|(part, _part_files)| {
                        // TODO depending on concurrency limit here, we may be
                        // materializing too many refs into memory at once.
                        // This is only done because ObjectRefIter is not Send
                        let obj_digests = {
                            let file_metadata = ref_files
                                .get(bucket)
                                .expect("No ref files found for bucket: {bucket_num}")
                                .get(part)
                                .expect(
                                    "No ref files found for bucket: {bucket_num}, part: {part_num}",
                                );
                            ObjectRefIter::new(
                                file_metadata,
                                local_staging_dir_root_clone.clone(),
                                epoch_dir_clone.clone(),
                            )
                            .expect("Failed to create object ref iter")
                        }
                        .map(|obj_ref| obj_ref.2)
                        .collect::<Vec<ObjectDigest>>();
                        let sender_clone = sender.clone();
                        tokio::spawn(async move {
                            let mut partial_acc = Accumulator::default();
                            let num_objects = obj_digests.len();
                            partial_acc.insert_all(obj_digests);
                            sender_clone
                                .send((partial_acc, num_objects as u64))
                                .await
                                .expect("Unable to send accumulator from snapshot reader");
                        })
                    })
                    .boxed()
                    .buffer_unordered(concurrency)
                    .for_each(|result| {
                        result.expect("Failed to generate partial accumulator");
                        accum_counter.fetch_add(1, Ordering::Relaxed);
                        futures::future::ready(())
                    })
                    .await;
            }
            accum_progress_bar.finish_with_message("Accumulation complete");
        })
    }

    async fn sync_live_objects(
        &self,
        perpetual_db: &AuthorityPerpetualTables,
        abort_registration: AbortRegistration,
        sha3_digests: Arc<Mutex<DigestByBucketAndPartition>>,
    ) -> Result<(), anyhow::Error> {
        let epoch_dir = self.epoch_dir();
        let concurrency = self.concurrency;
        let remote_object_store = self.remote_object_store.clone();
        let input_files: Vec<_> = self
            .object_files
            .iter()
            .flat_map(|(bucket, parts)| {
                parts
                    .clone()
                    .into_iter()
                    .map(|entry| (bucket, entry))
                    .collect::<Vec<_>>()
            })
            .collect();
        let obj_progress_bar = self.m.add(
            ProgressBar::new(input_files.len() as u64).with_style(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] {wide_bar} {pos} out of {len} .obj files done ({msg})",
                )
                .unwrap(),
            ),
        );
        let obj_progress_bar_clone = obj_progress_bar.clone();
        let instant = Instant::now();
        let downloaded_bytes = AtomicUsize::new(0);

        let ret = Abortable::new(
            async move {
                futures::stream::iter(input_files.iter())
                    .map(|(bucket, (part_num, file_metadata))| {
                        let epoch_dir_clone = epoch_dir.clone();
                        let remote_object_store_clone = remote_object_store.clone();
                        let sha3_digests_clone = sha3_digests.clone();
                        async move {
                            // Download object file with retries
                            let (bytes, sha3_digest) = download_bytes(
                                remote_object_store_clone,
                                file_metadata,
                                epoch_dir_clone,
                                sha3_digests_clone,
                                bucket,
                                part_num,
                                None,
                            )
                            .await;
                            Ok::<(Bytes, FileMetadata, [u8; 32]), anyhow::Error>((
                                bytes,
                                (*file_metadata).clone(),
                                sha3_digest,
                            ))
                        }
                    })
                    .boxed()
                    .buffer_unordered(concurrency)
                    .try_for_each(|(bytes, file_metadata, sha3_digest)| {
                        let bytes_len = bytes.len();
                        let result: Result<(), anyhow::Error> =
                            LiveObjectIter::new(&file_metadata, bytes).map(|obj_iter| {
                                AuthorityStore::bulk_insert_live_objects(
                                    perpetual_db,
                                    obj_iter,
                                    &sha3_digest,
                                )
                                .expect("Failed to insert live objects");
                            });
                        downloaded_bytes.fetch_add(bytes_len, Ordering::Relaxed);
                        obj_progress_bar_clone.inc(1);
                        obj_progress_bar_clone.set_message(format!(
                            "Download speed: {} MiB/s",
                            downloaded_bytes.load(Ordering::Relaxed) as f64
                                / (1024 * 1024) as f64
                                / instant.elapsed().as_secs_f64(),
                        ));
                        futures::future::ready(result)
                    })
                    .await
            },
            abort_registration,
        )
        .await?;
        obj_progress_bar.finish_with_message("Objects download complete");
        ret
    }

    // NOTE: export these metadata for indexer restorer
    pub async fn export_metadata(
        &self,
    ) -> Result<
        (
            Vec<(&u32, (u32, FileMetadata))>,
            Path,
            Arc<dyn ObjectStoreGetExt>,
            usize,
        ),
        anyhow::Error,
    > {
        let epoch_dir = self.epoch_dir();
        let concurrency = self.concurrency;
        let remote_object_store = self.remote_object_store.clone();
        let input_files: Vec<(&u32, (u32, FileMetadata))> = self
            .object_files
            .iter()
            .flat_map(|(bucket, parts)| {
                parts
                    .clone()
                    .into_iter()
                    .map(|entry| (bucket, entry))
                    .collect::<Vec<_>>()
            })
            .collect();
        Ok((input_files, epoch_dir, remote_object_store, concurrency))
    }

    pub fn ref_iter(&self, bucket_num: u32, part_num: u32) -> Result<ObjectRefIter> {
        let file_metadata = self
            .ref_files
            .get(&bucket_num)
            .context(format!("No ref files found for bucket: {bucket_num}"))?
            .get(&part_num)
            .context(format!(
                "No ref files found for bucket: {bucket_num}, part: {part_num}"
            ))?;
        ObjectRefIter::new(
            file_metadata,
            self.local_staging_dir_root.clone(),
            self.epoch_dir(),
        )
    }

    fn buckets(&self) -> Result<Vec<u32>> {
        Ok(self.ref_files.keys().copied().collect())
    }

    fn epoch_dir(&self) -> Path {
        Path::from(format!("epoch_{}", self.epoch))
    }

    fn read_manifest(path: PathBuf) -> anyhow::Result<Manifest> {
        let manifest_file = File::open(path)?;
        let manifest_file_size = manifest_file.metadata()?.len() as usize;
        let mut manifest_reader = BufReader::new(manifest_file);
        manifest_reader.rewind()?;
        let magic = manifest_reader.read_u32::<BigEndian>()?;
        if magic != MANIFEST_FILE_MAGIC {
            return Err(anyhow!("Unexpected magic byte: {}", magic));
        }
        manifest_reader.seek(SeekFrom::End(-(SHA3_BYTES as i64)))?;
        let mut sha3_digest = [0u8; SHA3_BYTES];
        manifest_reader.read_exact(&mut sha3_digest)?;
        manifest_reader.rewind()?;
        let mut content_buf = vec![0u8; manifest_file_size - SHA3_BYTES];
        manifest_reader.read_exact(&mut content_buf)?;
        let mut hasher = Sha3_256::default();
        hasher.update(&content_buf);
        let computed_digest = hasher.finalize().digest;
        if computed_digest != sha3_digest {
            return Err(anyhow!(
                "Checksum: {:?} don't match: {:?}",
                computed_digest,
                sha3_digest
            ));
        }
        manifest_reader.rewind()?;
        manifest_reader.seek(SeekFrom::Start(MAGIC_BYTES as u64))?;
        let manifest = bcs::from_bytes(&content_buf[MAGIC_BYTES..])?;
        Ok(manifest)
    }

    pub fn get_multi_progress(&self) -> MultiProgress {
        self.m.clone()
    }
}

pub async fn download_bytes(
    remote_object_store: Arc<dyn ObjectStoreGetExt>,
    file_metadata: &FileMetadata,
    epoch_dir: Path,
    sha3_digests: Sha3DigestType,
    bucket: &&u32,
    part_num: &u32,
    max_timeout_secs: Option<u64>,
) -> (Bytes, [u8; 32]) {
    let max_timeout = Duration::from_secs(max_timeout_secs.unwrap_or(60));
    let mut timeout = Duration::from_secs(2);
    timeout += timeout / 2;
    timeout = std::cmp::min(max_timeout, timeout);
    let mut attempts = 0usize;
    let file_path = file_metadata.file_path(&epoch_dir);
    let bytes = loop {
        match remote_object_store.get_bytes(&file_path).await {
            Ok(bytes) => {
                break bytes;
            }
            Err(err) => {
                error!(
                    "Obj {} .get failed (attempt {}): {}",
                    file_metadata.file_path(&epoch_dir),
                    attempts,
                    err,
                );
                if timeout > max_timeout {
                    panic!("Failed to get obj file after {} attempts", attempts);
                } else {
                    attempts += 1;
                    tokio::time::sleep(timeout).await;
                    timeout += timeout / 2;
                    continue;
                }
            }
        }
    };
    let sha3_digest = sha3_digests.lock().await;
    let bucket_map = sha3_digest
        .get(bucket)
        .expect("Bucket not in digest map")
        .clone();
    let sha3_digest = *bucket_map
        .get(part_num)
        .expect("sha3 digest not in bucket map");
    (bytes, sha3_digest)
}

/// An iterator over all object refs in a .ref file.
pub struct ObjectRefIter {
    reader: Box<dyn Read>,
}

impl ObjectRefIter {
    pub fn new(file_metadata: &FileMetadata, root_path: PathBuf, dir_path: Path) -> Result<Self> {
        let file_path = file_metadata.local_file_path(&root_path, &dir_path)?;
        let mut reader = file_metadata.file_compression.decompress(&file_path)?;
        let magic = reader.read_u32::<BigEndian>()?;
        if magic != REFERENCE_FILE_MAGIC {
            Err(anyhow!(
                "Unexpected magic string in REFERENCE file: {:?}",
                magic
            ))
        } else {
            Ok(ObjectRefIter { reader })
        }
    }

    fn next_ref(&mut self) -> Result<ObjectRef> {
        let mut buf = [0u8; OBJECT_REF_BYTES];
        self.reader.read_exact(&mut buf)?;
        let object_id = &buf[0..OBJECT_ID_BYTES];
        let sequence_number = &buf[OBJECT_ID_BYTES..OBJECT_ID_BYTES + SEQUENCE_NUM_BYTES]
            .reader()
            .read_u64::<BigEndian>()?;
        let sha3_digest = &buf[OBJECT_ID_BYTES + SEQUENCE_NUM_BYTES..OBJECT_REF_BYTES];
        let object_ref: ObjectRef = (
            ObjectID::from_bytes(object_id)?,
            SequenceNumber::from_u64(*sequence_number),
            ObjectDigest::try_from(sha3_digest)?,
        );
        Ok(object_ref)
    }
}

impl Iterator for ObjectRefIter {
    type Item = ObjectRef;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_ref().ok()
    }
}

/// An iterator over all objects in a *.obj file.
pub struct LiveObjectIter {
    reader: Box<dyn Read>,
}

impl LiveObjectIter {
    pub fn new(file_metadata: &FileMetadata, bytes: Bytes) -> Result<Self> {
        let mut reader = file_metadata.file_compression.bytes_decompress(bytes)?;
        let magic = reader.read_u32::<BigEndian>()?;
        if magic != OBJECT_FILE_MAGIC {
            Err(anyhow!(
                "Unexpected magic string in object file: {:?}",
                magic
            ))
        } else {
            Ok(LiveObjectIter { reader })
        }
    }

    fn next_object(&mut self) -> Result<LiveObject> {
        let len = self.reader.read_varint::<u64>()? as usize;
        if len == 0 {
            return Err(anyhow!("Invalid object length of 0 in file"));
        }
        let encoding = self.reader.read_u8()?;
        let mut data = vec![0u8; len];
        self.reader.read_exact(&mut data)?;
        let blob = Blob {
            data,
            encoding: BlobEncoding::try_from(encoding)?,
        };
        blob.decode()
    }
}

impl Iterator for LiveObjectIter {
    type Item = LiveObject;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_object().ok()
    }
}
