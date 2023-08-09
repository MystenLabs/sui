// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    FileMetadata, FileType, Manifest, MAGIC_BYTES, MANIFEST_FILE_MAGIC, OBJECT_FILE_MAGIC,
    OBJECT_ID_BYTES, OBJECT_REF_BYTES, REFERENCE_FILE_MAGIC, SEQUENCE_NUM_BYTES, SHA3_BYTES,
};
use anyhow::{anyhow, Context, Result};
use backoff::future::retry;
use byteorder::{BigEndian, ReadBytesExt};
use bytes::{Buf, Bytes};
use fastcrypto::hash::{HashFunction, Sha3_256};
use futures::future::{AbortRegistration, Abortable};
use futures::StreamExt;
use integer_encoding::VarIntReader;
use object_store::path::Path;
use object_store::DynObjectStore;
use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use sui_core::authority::authority_store_tables::{AuthorityPerpetualTables, LiveObject};
use sui_core::authority::AuthorityStore;
use sui_storage::blob::{Blob, BlobEncoding};
use sui_storage::object_store::util::{copy_file, copy_files, path_to_filesystem};
use sui_storage::object_store::ObjectStoreConfig;
use sui_types::base_types::{ObjectDigest, ObjectID, ObjectRef, SequenceNumber};
use tokio::sync::Mutex;

pub type DigestByBucketAndPartition = BTreeMap<u32, BTreeMap<u32, [u8; 32]>>;
pub struct StateSnapshotReaderV1 {
    epoch: u64,
    local_staging_dir_root: PathBuf,
    remote_object_store: Arc<DynObjectStore>,
    local_object_store: Arc<DynObjectStore>,
    ref_files: BTreeMap<u32, FileMetadata>,
    object_files: BTreeMap<u32, BTreeMap<u32, FileMetadata>>,
    indirect_objects_threshold: usize,
    concurrency: usize,
}

impl StateSnapshotReaderV1 {
    pub async fn new(
        epoch: u64,
        remote_store_config: &ObjectStoreConfig,
        local_store_config: &ObjectStoreConfig,
        indirect_objects_threshold: usize,
        download_concurrency: NonZeroUsize,
    ) -> Result<Self> {
        let epoch_dir = format!("epoch_{}", epoch);
        let remote_object_store = remote_store_config.make()?;

        let local_object_store = local_store_config.make()?;
        let local_staging_dir_root = local_store_config
            .directory
            .as_ref()
            .context("No directory specified")?
            .clone();
        let local_epoch_dir_path = local_staging_dir_root.join(&epoch_dir);
        if local_epoch_dir_path.exists() {
            fs::remove_dir_all(&local_epoch_dir_path)?;
        }
        fs::create_dir_all(&local_epoch_dir_path)?;
        // Download MANIFEST first
        let manifest_file_path = Path::from(epoch_dir.clone()).child("MANIFEST");
        copy_file(
            manifest_file_path.clone(),
            manifest_file_path.clone(),
            remote_object_store.clone(),
            local_object_store.clone(),
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
                    ref_files.insert(file_metadata.bucket_num, file_metadata.clone());
                }
            }
        }
        let epoch_dir_path = Path::from(epoch_dir);
        let files: Vec<Path> = ref_files
            .values()
            .map(|file_metadata| file_metadata.file_path(&epoch_dir_path))
            .collect();
        copy_files(
            &files,
            &files,
            remote_object_store.clone(),
            local_object_store.clone(),
            NonZeroUsize::new(1).unwrap(),
        )
        .await?;
        Ok(StateSnapshotReaderV1 {
            epoch,
            local_staging_dir_root,
            remote_object_store,
            local_object_store,
            ref_files,
            object_files,
            indirect_objects_threshold,
            concurrency: download_concurrency.get(),
        })
    }

    pub async fn read(
        &mut self,
        perpetual_db: &AuthorityPerpetualTables,
        abort_registration: AbortRegistration,
    ) -> Result<()> {
        // This computes and stores the sha3 digest of object references in REFERENCE file for each
        // bucket partition. When downloading objects, we will match sha3 digest of object references
        // per *.obj file against this. We do this so during restore we can pre fetch object
        // references and start building state accumulator and fail early if the state root hash
        // doesn't match but we still need to ensure that objects match references exactly.
        let sha3_digests: Arc<Mutex<DigestByBucketAndPartition>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        for bucket in self.buckets()?.iter() {
            let mut sha3_digests = sha3_digests.lock().await;
            let ref_iter = self.ref_iter(*bucket)?;
            let mut hasher = Sha3_256::default();
            let mut current_part_num = 1;
            let mut empty = true;
            for (object_ref, part_num) in ref_iter {
                empty = false;
                if part_num != current_part_num {
                    sha3_digests
                        .entry(*bucket)
                        .or_insert(BTreeMap::new())
                        .entry(part_num.try_into().unwrap())
                        .or_insert(hasher.finalize().digest);
                    hasher = Sha3_256::default();
                    current_part_num = part_num;
                }
                hasher.update(object_ref.2.inner());
            }
            if !empty {
                sha3_digests
                    .entry(*bucket)
                    .or_insert(BTreeMap::new())
                    .entry(current_part_num.try_into().unwrap())
                    .or_insert(hasher.finalize().digest);
            }
        }
        let input_files: Vec<_> = self
            .object_files
            .iter()
            .flat_map(|(bucket, parts)| {
                let vec: Vec<_> = parts.iter().map(|entry| (bucket, entry)).collect();
                vec
            })
            .collect();
        let epoch_dir = self.epoch_dir();
        let remote_object_store = self.remote_object_store.clone();
        let indirect_objects_threshold = self.indirect_objects_threshold;
        let download_concurrency = self.concurrency;
        let sha3_digests_cloned = sha3_digests.clone();
        if let Err(e) = Abortable::new(
            async move {
                let results: Vec<Result<(), anyhow::Error>> =
                    futures::stream::iter(input_files.iter())
                        .map(|(bucket, (part_num, file_metadata))| {
                            let backoff = backoff::ExponentialBackoff::default();
                            let epoch_dir = epoch_dir.clone();
                            let file_path = file_metadata.file_path(&epoch_dir);
                            let remote_object_store = remote_object_store.clone();
                            let sha3_digests = sha3_digests_cloned.clone();
                            async move {
                                let bytes = retry(backoff, || async {
                                    remote_object_store
                                        .get(&file_path)
                                        .await
                                        .map_err(|e| anyhow!("Failed to download file: {e}"))
                                        .map_err(backoff::Error::transient)
                                })
                                .await?
                                .bytes()
                                .await?;
                                let obj_iter =
                                    LiveObjectIter::new(&(*file_metadata).clone(), bytes)?;
                                let expected_sha3_digest = {
                                    let mut sha3_digests = sha3_digests.lock().await;
                                    let bucket_map =
                                        sha3_digests.get_mut(bucket).context("Missing bucket")?;
                                    let sha3_digest =
                                        bucket_map.remove(part_num).context("Missing part")?;
                                    if bucket_map.is_empty() {
                                        sha3_digests.remove(bucket).context("Missing bucket")?;
                                    }
                                    sha3_digest
                                };
                                AuthorityStore::bulk_insert_live_objects(
                                    perpetual_db,
                                    obj_iter,
                                    indirect_objects_threshold,
                                    &expected_sha3_digest,
                                )
                                .await?;
                                Ok::<(), anyhow::Error>(())
                            }
                        })
                        .boxed()
                        .buffer_unordered(download_concurrency)
                        .collect()
                        .await;
                results
                    .into_iter()
                    .collect::<Result<Vec<()>, anyhow::Error>>()
            },
            abort_registration,
        )
        .await?
        {
            Err(anyhow!("Failed to read objects: {e}"))
        } else {
            let sha3_digests = sha3_digests.lock().await;
            if !sha3_digests.is_empty() {
                Err(anyhow!("Object refs did not match objects"))
            } else {
                Ok(())
            }
        }
    }

    pub fn ref_iter(&mut self, bucket_num: u32) -> Result<ObjectRefIter> {
        let file_metadata = self
            .ref_files
            .get(&bucket_num)
            .context(format!("No ref files found for bucket: {bucket_num}",))?;
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
}

/// An iterator over all object refs in a REFERENCE file.
pub struct ObjectRefIter {
    reader: Box<dyn Read>,
    part_num: u64,
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
            Ok(ObjectRefIter {
                reader,
                part_num: 1,
            })
        }
    }

    fn check_if_delimiter(&self, buf: &[u8]) -> bool {
        let (prefix, aligned, suffix) = unsafe { buf.align_to::<u128>() };
        prefix.iter().all(|&x| x == 0)
            && suffix.iter().all(|&x| x == 0)
            && aligned.iter().all(|&x| x == 0)
    }

    fn next_ref(&mut self) -> Result<(ObjectRef, u64)> {
        let mut buf = [0u8; OBJECT_REF_BYTES];
        self.reader.read_exact(&mut buf)?;
        if self.check_if_delimiter(&buf) {
            self.part_num += 1;
            self.next_ref()
        } else {
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
            Ok((object_ref, self.part_num))
        }
    }
}

impl Iterator for ObjectRefIter {
    type Item = (ObjectRef, u64);
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
