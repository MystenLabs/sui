// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    FileMetadata, FileType, MAGIC_BYTES, MANIFEST_FILE_MAGIC, Manifest, OBJECT_FILE_MAGIC,
    OBJECT_ID_BYTES, OBJECT_REF_BYTES, REFERENCE_FILE_MAGIC, SEQUENCE_NUM_BYTES, SHA3_BYTES,
};
use anyhow::{Context, Result, anyhow};
use byteorder::{BigEndian, ReadBytesExt};
use bytes::{Buf, Bytes};
use prometheus::{
    IntCounter, IntGauge, Registry, register_int_counter_with_registry,
    register_int_gauge_with_registry,
};
use fastcrypto::hash::MultisetHash;
use fastcrypto::hash::{HashFunction, Sha3_256};
use futures::future::{AbortRegistration, Abortable};
use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use integer_encoding::VarIntReader;
use object_store::path::Path;
use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;
use sui_config::object_storage_config::ObjectStoreConfig;
use sui_core::authority::AuthorityStore;
use sui_core::authority::authority_store_tables::{AuthorityPerpetualTables, LiveObject};
use sui_futures::stream::TrySpawnStreamExt;
use sui_storage::blob::{Blob, BlobEncoding};
use sui_storage::object_store::http::HttpDownloaderBuilder;
use sui_storage::object_store::util::{copy_files, path_to_filesystem};
use sui_storage::object_store::{ObjectStoreGetExt, ObjectStoreListExt, ObjectStorePutExt};
use sui_types::base_types::{ObjectDigest, ObjectID, ObjectRef, SequenceNumber};
use sui_types::global_state_hash::GlobalStateHash;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{debug, error, info};

pub type SnapshotChecksums = (DigestByBucketAndPartition, GlobalStateHash);
pub type DigestByBucketAndPartition = BTreeMap<u32, BTreeMap<u32, [u8; 32]>>;
pub type Sha3DigestType = Arc<Mutex<BTreeMap<u32, BTreeMap<u32, [u8; 32]>>>>;

/// Number of parallel DB insert tasks spawned within a single object file.
const INSERT_CONCURRENCY: usize = 8;

pub struct StateSnapshotRestoreMetrics {
    /// Bytes currently held in memory across all in-progress object-file slots.
    /// The primary signal for memory pressure during restore.
    pub bytes_in_flight: IntGauge,
    /// Number of object files currently being downloaded or inserted.
    pub files_in_flight: IntGauge,
    /// Cumulative bytes across all fully-completed object files.
    pub downloaded_bytes_total: IntCounter,
    /// Cumulative number of fully-completed object files.
    pub completed_files_total: IntCounter,
}

impl StateSnapshotRestoreMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            bytes_in_flight: register_int_gauge_with_registry!(
                "snapshot_restore_bytes_in_flight",
                "Bytes currently held in memory across all in-progress object-file slots",
                registry
            )
            .unwrap(),
            files_in_flight: register_int_gauge_with_registry!(
                "snapshot_restore_files_in_flight",
                "Number of object files currently being downloaded or inserted",
                registry
            )
            .unwrap(),
            downloaded_bytes_total: register_int_counter_with_registry!(
                "snapshot_restore_downloaded_bytes_total",
                "Total bytes downloaded and inserted across all completed object files",
                registry
            )
            .unwrap(),
            completed_files_total: register_int_counter_with_registry!(
                "snapshot_restore_completed_files_total",
                "Total number of object files fully downloaded and inserted",
                registry
            )
            .unwrap(),
        })
    }
}

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
    max_retries: usize,
    remote_epoch_prefix: Path,
    metrics: Arc<StateSnapshotRestoreMetrics>,
}

impl StateSnapshotReaderV1 {
    async fn copy_file_with_retry<S: ObjectStoreGetExt, D: ObjectStorePutExt>(
        src: &Path,
        dest: &Path,
        src_store: &S,
        dest_store: &D,
        max_retries: usize,
    ) -> Result<()> {
        let mut attempts = 0;
        let max_attempts = max_retries + 1;
        loop {
            attempts += 1;
            match src_store.get_bytes(src).await {
                Ok(bytes) => {
                    if bytes.is_empty() {
                        tracing::warn!("Not copying empty file: {:?}", src);
                        return Ok(());
                    }
                    match dest_store.put_bytes(dest, bytes).await {
                        Ok(_) => return Ok(()),
                        Err(e) => {
                            if attempts >= max_attempts {
                                return Err(anyhow::anyhow!(
                                    "Failed to write {} after {} attempts: {}",
                                    dest,
                                    attempts,
                                    e
                                ));
                            }
                            tracing::warn!(
                                "Failed to write {} (attempt {}/{}): {}, retrying in {}ms",
                                dest,
                                attempts,
                                max_attempts,
                                e,
                                1000 * attempts
                            );
                            tokio::time::sleep(Duration::from_millis(1000 * attempts as u64)).await;
                        }
                    }
                }
                Err(e) => {
                    if attempts >= max_attempts {
                        return Err(anyhow::anyhow!(
                            "Failed to download {} after {} attempts: {}",
                            src,
                            attempts,
                            e
                        ));
                    }
                    tracing::warn!(
                        "Failed to download {} (attempt {}/{}): {}, retrying in {}ms",
                        src,
                        attempts,
                        max_attempts,
                        e,
                        1000 * attempts
                    );
                    tokio::time::sleep(Duration::from_millis(1000 * attempts as u64)).await;
                }
            }
        }
    }

    pub async fn new(
        epoch: u64,
        remote_store_config: &ObjectStoreConfig,
        local_store_config: &ObjectStoreConfig,
        download_concurrency: NonZeroUsize,
        m: MultiProgress,
        skip_reset_local_store: bool,
        max_retries: usize,
        registry: &Registry,
    ) -> Result<Self> {
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

        let local_epoch_dir_name = format!("epoch_{}", epoch);
        let local_epoch_dir_path = Path::from(local_epoch_dir_name.clone());

        if !skip_reset_local_store {
            let local_epoch_dir_absolute_path = local_staging_dir_root.join(&local_epoch_dir_name);
            if local_epoch_dir_absolute_path.exists() {
                fs::remove_dir_all(&local_epoch_dir_absolute_path)?;
            }
            fs::create_dir_all(&local_epoch_dir_absolute_path)?;
        }

        // Try to download MANIFEST from standard location first, then archive
        let standard_epoch_dir = Path::from(format!("epoch_{}", epoch));
        let archive_epoch_dir = Path::from(format!("archive/epoch_{}", epoch));

        let standard_manifest_path = standard_epoch_dir.child("MANIFEST");
        let archive_manifest_path = archive_epoch_dir.child("MANIFEST");

        // We always download to local epoch dir's MANIFEST
        let local_manifest_path = local_epoch_dir_path.child("MANIFEST");

        let (remote_epoch_prefix, manifest_download_result) = match Self::copy_file_with_retry(
            &standard_manifest_path,
            &local_manifest_path,
            &remote_object_store,
            &local_object_store,
            max_retries,
        )
        .await
        {
            Ok(_) => (standard_epoch_dir, Ok(())),
            Err(_) => {
                // Try archive
                match Self::copy_file_with_retry(
                    &archive_manifest_path,
                    &local_manifest_path,
                    &remote_object_store,
                    &local_object_store,
                    max_retries,
                )
                .await
                {
                    Ok(_) => (archive_epoch_dir, Ok(())),
                    Err(e) => (standard_epoch_dir, Err(e)), // Return standard dir but error
                }
            }
        };

        manifest_download_result?;

        let manifest = Self::read_manifest(path_to_filesystem(
            local_staging_dir_root.clone(),
            &local_manifest_path,
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

        let mut src_files = Vec::new();
        let mut dest_files = Vec::new();

        let existing_files = if skip_reset_local_store {
            let mut existing = std::collections::HashSet::new();
            let mut list_stream = local_object_store_list
                .list_objects(Some(&local_epoch_dir_path))
                .await;
            while let Some(Ok(meta)) = list_stream.next().await {
                existing.insert(meta.location);
            }
            Some(existing)
        } else {
            None
        };

        for entry in ref_files.values() {
            for file_metadata in entry.values() {
                let dest = file_metadata.file_path(&local_epoch_dir_path);
                if existing_files
                    .as_ref()
                    .is_some_and(|existing| existing.contains(&dest))
                {
                    continue;
                }
                src_files.push(file_metadata.file_path(&remote_epoch_prefix));
                dest_files.push(dest);
            }
        }

        let progress_bar = m.add(
            ProgressBar::new(src_files.len() as u64).with_style(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] {wide_bar} {pos} out of {len} missing .ref files done ({msg})",
                )
                .unwrap(),
            ),
        );
        copy_files(
            &src_files,
            &dest_files,
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
            max_retries,
            remote_epoch_prefix,
            metrics: StateSnapshotRestoreMetrics::new(registry),
        })
    }

    pub async fn read(
        &mut self,
        perpetual_db: Arc<AuthorityPerpetualTables>,
        abort_registration: AbortRegistration,
        sender: Option<tokio::sync::mpsc::Sender<(GlobalStateHash, u64)>>,
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
        sender: tokio::sync::mpsc::Sender<(GlobalStateHash, u64)>,
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
                            let mut partial_acc = GlobalStateHash::default();
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
        perpetual_db: Arc<AuthorityPerpetualTables>,
        abort_registration: AbortRegistration,
        sha3_digests: Arc<Mutex<DigestByBucketAndPartition>>,
    ) -> Result<(), anyhow::Error> {
        let epoch_dir = self.remote_epoch_prefix.clone();
        let remote_object_store = self.remote_object_store.clone();
        // Store owned u32 keys so we can move items into async closures without lifetime issues.
        let input_files: Vec<(u32, u32, FileMetadata)> = self
            .object_files
            .iter()
            .flat_map(|(bucket, parts)| {
                parts
                    .iter()
                    .map(|(part, metadata)| (*bucket, *part, metadata.clone()))
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
        let metrics = self.metrics.clone();
        let downloaded_bytes = Arc::new(AtomicUsize::new(0));

        let ret = Abortable::new(
            async move {
                for (bucket, part_num, file_metadata) in input_files {
                    let (bytes, sha3_digest) = download_bytes(
                        remote_object_store.clone(),
                        &file_metadata,
                        epoch_dir.clone(),
                        sha3_digests.clone(),
                        &&bucket,
                        &part_num,
                        None,
                    )
                    .await;
                    let bytes_len = bytes.len();
                    metrics.bytes_in_flight.add(bytes_len as i64);

                    // Collect objects and validate checksum against the pre-computed
                    // digest from the reference file.
                    let mut hasher = Sha3_256::default();
                    let mut objects: Vec<LiveObject> = Vec::new();
                    for obj in LiveObjectIter::new(&file_metadata, bytes)? {
                        hasher.update(obj.object_reference().2.inner());
                        objects.push(obj);
                    }
                    let computed_digest = hasher.finalize().digest;
                    if computed_digest != sha3_digest {
                        return Err(anyhow!(
                            "Checksum mismatch for bucket {} part {}",
                            bucket,
                            part_num
                        ));
                    }

                    // Split objects across INSERT_CONCURRENCY tasks and insert in parallel.
                    if !objects.is_empty() {
                        let chunk_size =
                            (objects.len() + INSERT_CONCURRENCY - 1) / INSERT_CONCURRENCY;
                        let handles: Vec<_> = objects
                            .chunks(chunk_size)
                            .map(|chunk| {
                                let perpetual_db = perpetual_db.clone();
                                let chunk = chunk.to_vec();
                                tokio::task::spawn_blocking(move || {
                                    AuthorityStore::insert_live_objects_batch(
                                        &perpetual_db,
                                        chunk.into_iter(),
                                    )
                                })
                            })
                            .collect();
                        for handle in handles {
                            handle.await.expect("Insert task panicked")?;
                        }
                    }

                    metrics.bytes_in_flight.sub(bytes_len as i64);
                    metrics.downloaded_bytes_total.inc_by(bytes_len as u64);
                    metrics.completed_files_total.inc();
                    let total =
                        downloaded_bytes.fetch_add(bytes_len, Ordering::Relaxed) + bytes_len;
                    obj_progress_bar_clone.inc(1);
                    obj_progress_bar_clone.set_message(format!(
                        "Download speed: {} MiB/s",
                        total as f64 / (1024 * 1024) as f64 / instant.elapsed().as_secs_f64(),
                    ));
                }
                Ok(())
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
        let epoch_dir = self.remote_epoch_prefix.clone();
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
    const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes for large files
    const INITIAL_BACKOFF: Duration = Duration::from_secs(3);

    let backoff_cap = Duration::from_secs(max_timeout_secs.unwrap_or(60));
    let mut backoff = INITIAL_BACKOFF;
    let mut attempts = 0usize;
    let file_path = file_metadata.file_path(&epoch_dir);
    let bytes = loop {
        debug!(
            "Downloading obj file: {:?} (attempt {}, timeout {:?})",
            file_path, attempts, DOWNLOAD_TIMEOUT
        );

        match tokio::time::timeout(DOWNLOAD_TIMEOUT, remote_object_store.get_bytes(&file_path))
            .await
        {
            Ok(Ok(bytes)) => {
                break bytes;
            }
            Ok(Err(err)) => {
                error!(
                    "Failed to download {}: {} (attempt {})",
                    file_path, err, attempts,
                );
            }
            Err(_) => {
                error!(
                    "Download timed out for {} after {:?} (attempt {})",
                    file_path, DOWNLOAD_TIMEOUT, attempts,
                );
            }
        }

        attempts += 1;
        debug!("Retrying {} in {:?}...", file_path, backoff);
        tokio::time::sleep(backoff).await;

        // Exponential backoff with 1.5x multiplier, capped at backoff_cap
        backoff += backoff / 2;
        backoff = std::cmp::min(backoff, backoff_cap);
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
