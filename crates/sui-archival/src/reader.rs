// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    read_manifest, FileMetadata, FileType, Manifest, CHECKPOINT_FILE_MAGIC, EPOCH_DIR_PREFIX,
    SUMMARY_FILE_MAGIC,
};
use anyhow::{anyhow, Context, Result};
use backoff::future::retry;
use byteorder::{BigEndian, ReadBytesExt};
use bytes::Bytes;
use futures::StreamExt;
use futures::TryStreamExt;
use integer_encoding::VarIntReader;
use itertools::Itertools;
use object_store::path::Path;
use object_store::DynObjectStore;
use std::future;
use std::io::Read;
use std::num::NonZeroUsize;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_storage::object_store::ObjectStoreConfig;
use sui_storage::{verify_checkpoint, Blob, Encoding};
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary as Checkpoint, CertifiedCheckpointSummary, CheckpointSequenceNumber,
    FullCheckpointContents as CheckpointContents, VerifiedCheckpoint, VerifiedCheckpointContents,
};
use sui_types::storage::{ReadStore, WriteStore};
use tokio::sync::oneshot::Sender;
use tokio::sync::{oneshot, Mutex};
use tracing::info;

pub struct ArchiveReaderV1 {
    concurrency: usize,
    sender: Sender<()>,
    manifest: Arc<Mutex<Manifest>>,
    remote_object_store: Arc<DynObjectStore>,
}

impl ArchiveReaderV1 {
    pub fn new(
        remote_store_config: ObjectStoreConfig,
        download_concurrency: NonZeroUsize,
    ) -> Result<Self> {
        let remote_object_store = remote_store_config.make()?;
        let (sender, recv) = tokio::sync::oneshot::channel();
        let manifest = Arc::new(Mutex::new(Manifest::new(0, 0)));
        Self::spawn_manifest_sync_task(remote_object_store.clone(), manifest.clone(), recv);
        Ok(ArchiveReaderV1 {
            manifest,
            sender,
            remote_object_store,
            concurrency: download_concurrency.get(),
        })
    }

    pub async fn read_unordered<S>(
        &mut self,
        store: S,
        checkpoint_range: Range<CheckpointSequenceNumber>,
        counter: Arc<AtomicU64>,
    ) -> Result<()>
    where
        S: WriteStore + Clone,
        <S as ReadStore>::Error: std::error::Error,
    {
        let manifest = self.manifest.lock().await.clone();
        let files = manifest.files();
        if files.is_empty() {
            return Err(anyhow!("No files in archive store to read from"));
        }
        let mut summary_files: Vec<_> = files
            .clone()
            .into_iter()
            .filter(|f| f.file_type == FileType::CheckpointSummary)
            .collect();
        let mut contents_files: Vec<_> = files
            .into_iter()
            .filter(|f| f.file_type == FileType::CheckpointContent)
            .collect();
        assert_eq!(summary_files.len(), contents_files.len());

        summary_files.sort_by_key(|f| f.checkpoint_seq_range.start);
        contents_files.sort_by_key(|f| f.checkpoint_seq_range.start);

        assert!(summary_files
            .windows(2)
            .all(|w| w[1].checkpoint_seq_range.start == w[0].checkpoint_seq_range.end));
        assert!(contents_files
            .windows(2)
            .all(|w| w[1].checkpoint_seq_range.start == w[0].checkpoint_seq_range.end));

        let files: Vec<_> = summary_files
            .into_iter()
            .zip(contents_files.into_iter())
            .map(|(s, c)| {
                assert_eq!(s.checkpoint_seq_range, c.checkpoint_seq_range);
                (s, c)
            })
            .collect();

        assert_eq!(files.first().unwrap().0.checkpoint_seq_range.start, 0);

        let latest_available_checkpoint = manifest
            .next_checkpoint_seq_num()
            .checked_sub(1)
            .context("Checkpoint seq num underflow")?;
        if checkpoint_range.start > latest_available_checkpoint {
            return Err(anyhow!("Archive cannot complete the request as the latest available checkpoint available is: {}", latest_available_checkpoint));
        }

        let start_index = match files.binary_search_by_key(&checkpoint_range.start, |(s, _c)| {
            s.checkpoint_seq_range.start
        }) {
            Ok(index) => index,
            Err(index) => index - 1,
        };

        let end_index = match files.binary_search_by_key(&checkpoint_range.end, |(s, _c)| {
            s.checkpoint_seq_range.start
        }) {
            Ok(index) => index,
            Err(index) => index,
        };

        let remote_object_store = self.remote_object_store.clone();
        let results: Vec<Result<Result<(), anyhow::Error>, anyhow::Error>> =
            futures::stream::iter(files.iter())
                .enumerate()
                .filter(|(index, (_s, _c))| {
                    future::ready(*index >= start_index && *index < end_index)
                })
                .map(|(_, (summary_metadata, content_metadata))| {
                    let remote_object_store = remote_object_store.clone();
                    async move {
                        let summary_bytes = Self::download_file(
                            summary_metadata.clone(),
                            remote_object_store.clone(),
                        )
                        .await?;
                        let content_bytes = Self::download_file(
                            content_metadata.clone(),
                            remote_object_store.clone(),
                        )
                        .await?;
                        Ok::<((FileMetadata, Bytes), (FileMetadata, Bytes)), anyhow::Error>((
                            (summary_metadata.clone(), summary_bytes),
                            (content_metadata.clone(), content_bytes),
                        ))
                    }
                })
                .boxed()
                .buffer_unordered(self.concurrency)
                .map_ok(
                    |((summary_metadata, summary_bytes), (content_metadata, content_bytes))| {
                        let summary_iter =
                            CheckpointSummaryIter::new(&summary_metadata, summary_bytes)
                                .expect("Checkpoint summary iter creation must not fail");
                        let content_iter =
                            CheckpointContentsIter::new(&content_metadata, content_bytes)
                                .expect("Checkpoint content iter creation must not fail");
                        let now = Instant::now();
                        let (summaries, contents): (Vec<_>, Vec<_>) = summary_iter
                            .zip(content_iter)
                            .filter(|(s, _c)| {
                                s.sequence_number >= checkpoint_range.start
                                    && s.sequence_number < checkpoint_range.end
                            })
                            .unzip();
                        let to_parse = now.elapsed();
                        info!("Time to parse: {:?}", to_parse);
                        let verified_summaries: Vec<_> = summaries
                            .into_iter()
                            .map(|certified_checkpoint| {
                                VerifiedCheckpoint::new_unchecked(certified_checkpoint)
                            })
                            .collect();
                        let verified_contents: Vec<_> = contents
                            .into_iter()
                            .map(|contents| VerifiedCheckpointContents::new_unchecked(contents))
                            .collect();
                        counter.fetch_add(verified_summaries.len() as u64, Ordering::Relaxed);
                        // Insert summaries
                        let result = store
                            .batch_insert_checkpoint(&verified_summaries)
                            .map_err(|e| anyhow!("Failed to insert summaries: {e}"))
                            .and_then(|_| {
                                store
                                    .batch_insert_checkpoint_contents(
                                        &verified_summaries,
                                        &verified_contents,
                                    )
                                    .map_err(|e| anyhow!("Failed to insert content: {e}"))
                            });
                        info!("Time to write: {:?}", now.elapsed() - to_parse);
                        result
                    },
                )
                .collect()
                .await;
        results
            .into_iter()
            .flatten()
            .into_iter()
            .collect::<Result<Vec<()>, anyhow::Error>>()?;
        Ok(())
    }

    pub async fn read<S>(
        &mut self,
        store: S,
        checkpoint_range: Range<CheckpointSequenceNumber>,
        counter: Arc<AtomicU64>,
    ) -> Result<()>
    where
        S: WriteStore + Clone,
        <S as ReadStore>::Error: std::error::Error,
    {
        let manifest = self.manifest.lock().await.clone();
        let files = manifest.files();
        if files.is_empty() {
            return Err(anyhow!("No files in archive store to read from"));
        }
        let mut summary_files: Vec<_> = files
            .clone()
            .into_iter()
            .filter(|f| f.file_type == FileType::CheckpointSummary)
            .collect();
        let mut contents_files: Vec<_> = files
            .into_iter()
            .filter(|f| f.file_type == FileType::CheckpointContent)
            .collect();
        assert_eq!(summary_files.len(), contents_files.len());

        summary_files.sort_by_key(|f| f.checkpoint_seq_range.start);
        contents_files.sort_by_key(|f| f.checkpoint_seq_range.start);

        assert!(summary_files
            .windows(2)
            .all(|w| w[1].checkpoint_seq_range.start == w[0].checkpoint_seq_range.end));
        assert!(contents_files
            .windows(2)
            .all(|w| w[1].checkpoint_seq_range.start == w[0].checkpoint_seq_range.end));

        let files: Vec<_> = summary_files
            .into_iter()
            .zip(contents_files.into_iter())
            .map(|(s, c)| {
                assert_eq!(s.checkpoint_seq_range, c.checkpoint_seq_range);
                (s, c)
            })
            .collect();

        assert_eq!(files.first().unwrap().0.checkpoint_seq_range.start, 0);

        let latest_available_checkpoint = manifest
            .next_checkpoint_seq_num()
            .checked_sub(1)
            .context("Checkpoint seq num underflow")?;
        if checkpoint_range.start > latest_available_checkpoint {
            return Err(anyhow!("Archive cannot complete the request as the latest available checkpoint available is: {}", latest_available_checkpoint));
        }

        let start_index = match files.binary_search_by_key(&checkpoint_range.start, |(s, _c)| {
            s.checkpoint_seq_range.start
        }) {
            Ok(index) => index,
            Err(index) => index - 1,
        };

        let end_index = match files.binary_search_by_key(&checkpoint_range.end, |(s, _c)| {
            s.checkpoint_seq_range.start
        }) {
            Ok(index) => index,
            Err(index) => index,
        };

        let remote_object_store = self.remote_object_store.clone();

        let results: Vec<Result<Result<(), anyhow::Error>, anyhow::Error>> =
            futures::stream::iter(files.iter())
                .enumerate()
                .filter(|(index, (_s, _c))| {
                    future::ready(*index >= start_index && *index < end_index)
                })
                .map(|(_, (summary_metadata, content_metadata))| {
                    let remote_object_store = remote_object_store.clone();
                    async move {
                        let summary_bytes = Self::download_file(
                            summary_metadata.clone(),
                            remote_object_store.clone(),
                        )
                        .await?;
                        let content_bytes = Self::download_file(
                            content_metadata.clone(),
                            remote_object_store.clone(),
                        )
                        .await?;

                        Ok::<((FileMetadata, Bytes), (FileMetadata, Bytes)), anyhow::Error>((
                            (summary_metadata.clone(), summary_bytes),
                            (content_metadata.clone(), content_bytes),
                        ))
                    }
                })
                .boxed()
                .buffered(self.concurrency)
                .map_ok(
                    |((summary_metadata, summary_bytes), (content_metadata, content_bytes))| {
                        let summary_iter =
                            CheckpointSummaryIter::new(&summary_metadata, summary_bytes)
                                .expect("Checkpoint summary iter creation must not fail");
                        let content_iter =
                            CheckpointContentsIter::new(&content_metadata, content_bytes)
                                .expect("Checkpoint content iter creation must not fail");

                        let result: Vec<_> = summary_iter
                            .zip(content_iter)
                            .filter(|(s, _c)| {
                                s.sequence_number >= checkpoint_range.start
                                    && s.sequence_number < checkpoint_range.end
                            })
                            .map(|(summary, contents)| {
                                let verified_checkpoint =
                                    Self::get_or_insert_verified_checkpoint(&store, summary)?;
                                counter.fetch_add(contents.size() as u64, Ordering::Relaxed);
                                // Verify content
                                let digest = verified_checkpoint.content_digest;
                                contents.verify_digests(digest)?;
                                let verified_contents =
                                    VerifiedCheckpointContents::new_unchecked(contents.clone());

                                // Insert content
                                store
                                    .insert_checkpoint_contents(
                                        &verified_checkpoint,
                                        verified_contents,
                                    )
                                    .map_err(|e| anyhow!("Failed to insert content: {e}"))?;
                                store
                                    .update_highest_synced_checkpoint(&verified_checkpoint)
                                    .map_err(|e| anyhow!("Failed to update watermark: {e}"))?;
                                Ok::<(), anyhow::Error>(())
                            })
                            .collect();
                        result
                            .into_iter()
                            .collect::<Result<Vec<()>, anyhow::Error>>()
                            .map(|_| ())
                    },
                )
                .collect()
                .await;
        results
            .into_iter()
            .flatten()
            .into_iter()
            .collect::<Result<Vec<()>, anyhow::Error>>()?;
        Ok(())
    }

    pub async fn latest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber> {
        let manifest = self.manifest.lock().await.clone();
        manifest
            .next_checkpoint_seq_num()
            .checked_sub(1)
            .context("No checkpoint data in archive")
    }

    pub async fn sync_manifest_once(&self) -> Result<()> {
        Self::sync_manifest(self.remote_object_store.clone(), self.manifest.clone()).await?;
        Ok(())
    }

    async fn sync_manifest(
        remote_store: Arc<DynObjectStore>,
        manifest: Arc<Mutex<Manifest>>,
    ) -> Result<()> {
        let new_manifest = read_manifest(remote_store.clone()).await?;
        let mut locked = manifest.lock().await;
        *locked = new_manifest;
        Ok(())
    }

    async fn download_file(
        file_metadata: FileMetadata,
        object_store: Arc<DynObjectStore>,
    ) -> Result<Bytes> {
        let backoff = backoff::ExponentialBackoff::default();
        let file_path = file_metadata.file_path(&Self::epoch_dir(file_metadata.epoch_num));
        let remote_object_store = object_store.clone();
        let bytes = retry(backoff.clone(), || async {
            remote_object_store
                .get(&file_path)
                .await
                .map_err(|e| anyhow!("Failed to download file: {e}"))
                .map_err(backoff::Error::transient)
        })
        .await?
        .bytes()
        .await?;
        Ok(bytes)
    }

    fn insert_checkpoint<S>(
        store: &S,
        certified_checkpoint: CertifiedCheckpointSummary,
    ) -> Result<VerifiedCheckpoint>
    where
        S: WriteStore + Clone,
        <S as ReadStore>::Error: std::error::Error,
    {
        // Insert checkpoint summary
        let verified_checkpoint = VerifiedCheckpoint::new_unchecked(certified_checkpoint);
        store
            .batch_insert_checkpoint(&[verified_checkpoint.clone()])
            .map_err(|e| anyhow!("Failed to insert checkpoint: {e}"))?;
        Ok(verified_checkpoint)
    }

    fn get_or_insert_verified_checkpoint<S>(
        store: &S,
        certified_checkpoint: CertifiedCheckpointSummary,
    ) -> Result<VerifiedCheckpoint>
    where
        S: WriteStore + Clone,
        <S as ReadStore>::Error: std::error::Error,
    {
        store
            .get_checkpoint_by_sequence_number(certified_checkpoint.sequence_number)
            .map_err(|e| anyhow!("Store op failed: {e}"))?
            .map(Ok::<VerifiedCheckpoint, anyhow::Error>)
            .unwrap_or_else(|| {
                // Verify checkpoint summary
                let prev_checkpoint_seq_num = certified_checkpoint
                    .sequence_number
                    .checked_sub(1)
                    .context("Checkpoint seq num underflow")?;
                let prev_checkpoint = store
                    .get_checkpoint_by_sequence_number(prev_checkpoint_seq_num)
                    .map_err(|e| anyhow!("Store op failed: {e}"))?
                    .context(format!(
                        "Missing previous checkpoint {} in store",
                        prev_checkpoint_seq_num
                    ))?;
                let verified_checkpoint =
                    verify_checkpoint(&prev_checkpoint, &store, certified_checkpoint)
                        .map_err(|_| anyhow!("Checkpoint verification failed"))?;
                // Insert checkpoint summary
                store
                    .insert_checkpoint(verified_checkpoint.clone())
                    .map_err(|e| anyhow!("Failed to insert checkpoint: {e}"))?;
                Ok::<VerifiedCheckpoint, anyhow::Error>(verified_checkpoint)
            })
            .map_err(|e| anyhow!("Failed to get verified checkpoint: {:?}", e))
    }

    fn spawn_manifest_sync_task(
        remote_store: Arc<DynObjectStore>,
        manifest: Arc<Mutex<Manifest>>,
        mut recv: oneshot::Receiver<()>,
    ) {
        tokio::task::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let new_manifest = read_manifest(remote_store.clone()).await?;
                        let mut locked = manifest.lock().await;
                        *locked = new_manifest;
                    }
                    _ = &mut recv => break,
                }
            }
            info!("Terminating the manifest sync loop");
            Ok::<(), anyhow::Error>(())
        });
    }

    fn epoch_dir(epoch_num: u64) -> Path {
        Path::from(format!("{}{}", EPOCH_DIR_PREFIX, epoch_num))
    }
}

/// An iterator over all checkpoints in a *.chk file.
pub struct CheckpointContentsIter {
    reader: Box<dyn Read>,
}

impl CheckpointContentsIter {
    pub fn new(file_metadata: &FileMetadata, bytes: Bytes) -> Result<Self> {
        let mut reader = file_metadata.file_compression.bulk_decompress(bytes)?;
        let magic = reader.read_u32::<BigEndian>()?;
        if magic != CHECKPOINT_FILE_MAGIC {
            Err(anyhow!(
                "Unexpected magic string in checkpoint file: {:?}",
                magic
            ))
        } else {
            Ok(CheckpointContentsIter { reader })
        }
    }

    fn next_checkpoint(&mut self) -> Result<CheckpointContents> {
        let len = self.reader.read_varint::<u64>()? as usize;
        if len == 0 {
            return Err(anyhow!("Invalid checkpoint length of 0 in file"));
        }
        let encoding = self.reader.read_u8()?;
        let mut data = vec![0u8; len];
        self.reader.read_exact(&mut data)?;
        let blob = Blob {
            data,
            encoding: Encoding::try_from(encoding)?,
        };
        blob.decode()
    }
}

impl Iterator for CheckpointContentsIter {
    type Item = CheckpointContents;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_checkpoint().ok()
    }
}

/// An iterator over all checkpoint summaries in a *.chk file.
pub struct CheckpointSummaryIter {
    reader: Box<dyn Read>,
}

impl CheckpointSummaryIter {
    pub fn new(file_metadata: &FileMetadata, bytes: Bytes) -> Result<Self> {
        let mut reader = file_metadata.file_compression.bulk_decompress(bytes)?;
        let magic = reader.read_u32::<BigEndian>()?;
        if magic != SUMMARY_FILE_MAGIC {
            Err(anyhow!(
                "Unexpected magic string in checkpoint file: {:?}",
                magic
            ))
        } else {
            Ok(CheckpointSummaryIter { reader })
        }
    }

    fn next_checkpoint(&mut self) -> Result<Checkpoint> {
        let len = self.reader.read_varint::<u64>()? as usize;
        if len == 0 {
            return Err(anyhow!("Invalid checkpoint length of 0 in file"));
        }
        let encoding = self.reader.read_u8()?;
        let mut data = vec![0u8; len];
        self.reader.read_exact(&mut data)?;
        let blob = Blob {
            data,
            encoding: Encoding::try_from(encoding)?,
        };
        blob.decode()
    }
}

impl Iterator for CheckpointSummaryIter {
    type Item = Checkpoint;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_checkpoint().ok()
    }
}
