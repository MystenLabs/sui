// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    read_manifest, FileMetadata, FileType, Manifest, CHECKPOINT_FILE_MAGIC, SUMMARY_FILE_MAGIC,
};
use anyhow::{anyhow, Context, Result};
use bytes::buf::Reader;
use bytes::{Buf, Bytes};
use futures::{StreamExt, TryStreamExt};
use prometheus::{register_int_counter_vec_with_registry, IntCounterVec, Registry};
use rand::seq::SliceRandom;
use std::borrow::Borrow;
use std::future;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sui_config::node::ArchiveReaderConfig;
use sui_storage::object_store::http::HttpDownloaderBuilder;
use sui_storage::object_store::util::get;
use sui_storage::object_store::ObjectStoreGetExt;
use sui_storage::{compute_sha3_checksum_for_bytes, make_iterator, verify_checkpoint};
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointSequenceNumber,
    FullCheckpointContents as CheckpointContents, VerifiedCheckpoint, VerifiedCheckpointContents,
};
use sui_types::storage::WriteStore;
use tokio::sync::oneshot::Sender;
use tokio::sync::{oneshot, Mutex};
use tracing::info;

#[derive(Debug)]
pub struct ArchiveReaderMetrics {
    pub archive_txns_read: IntCounterVec,
    pub archive_checkpoints_read: IntCounterVec,
}

impl ArchiveReaderMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        let this = Self {
            archive_txns_read: register_int_counter_vec_with_registry!(
                "archive_txns_read",
                "Number of transactions read from archive",
                &["bucket"],
                registry
            )
            .unwrap(),
            archive_checkpoints_read: register_int_counter_vec_with_registry!(
                "archive_checkpoints_read",
                "Number of checkpoints read from archive",
                &["bucket"],
                registry
            )
            .unwrap(),
        };
        Arc::new(this)
    }
}

// ArchiveReaderBalancer selects archives for reading based on whether they can fulfill a checkpoint request
#[derive(Default, Clone)]
pub struct ArchiveReaderBalancer {
    readers: Vec<Arc<ArchiveReader>>,
}

impl ArchiveReaderBalancer {
    pub fn new(configs: Vec<ArchiveReaderConfig>, registry: &Registry) -> Result<Self> {
        let mut readers = vec![];
        let metrics = ArchiveReaderMetrics::new(registry);
        for config in configs.into_iter() {
            readers.push(Arc::new(ArchiveReader::new(config.clone(), &metrics)?));
        }
        Ok(ArchiveReaderBalancer { readers })
    }
    pub async fn get_archive_watermark(&self) -> Result<Option<u64>> {
        let mut checkpoints: Vec<Result<CheckpointSequenceNumber>> = vec![];
        for reader in self
            .readers
            .iter()
            .filter(|r| r.use_for_pruning_watermark())
        {
            let latest_checkpoint = reader.latest_available_checkpoint().await;
            info!(
                "Latest archived checkpoint in remote store: {:?} is: {:?}",
                reader.remote_store_identifier(),
                latest_checkpoint
            );
            checkpoints.push(latest_checkpoint)
        }
        let checkpoints: Result<Vec<CheckpointSequenceNumber>> = checkpoints.into_iter().collect();
        checkpoints.map(|vec| vec.into_iter().min())
    }
    pub async fn pick_one_random(
        &self,
        checkpoint_range: Range<CheckpointSequenceNumber>,
    ) -> Option<Arc<ArchiveReader>> {
        let mut archives_with_complete_range = vec![];
        for reader in self.readers.iter() {
            let latest_checkpoint = reader.latest_available_checkpoint().await.unwrap_or(0);
            if latest_checkpoint >= checkpoint_range.end {
                archives_with_complete_range.push(reader.clone());
            }
        }
        if !archives_with_complete_range.is_empty() {
            return Some(
                archives_with_complete_range
                    .choose(&mut rand::thread_rng())
                    .unwrap()
                    .clone(),
            );
        }
        let mut archives_with_partial_range = vec![];
        for reader in self.readers.iter() {
            let latest_checkpoint = reader.latest_available_checkpoint().await.unwrap_or(0);
            if latest_checkpoint >= checkpoint_range.start {
                archives_with_partial_range.push(reader.clone());
            }
        }
        if !archives_with_partial_range.is_empty() {
            return Some(
                archives_with_partial_range
                    .choose(&mut rand::thread_rng())
                    .unwrap()
                    .clone(),
            );
        }
        None
    }
}

#[derive(Clone)]
pub struct ArchiveReader {
    bucket: String,
    concurrency: usize,
    sender: Arc<Sender<()>>,
    manifest: Arc<Mutex<Manifest>>,
    use_for_pruning_watermark: bool,
    remote_object_store: Arc<dyn ObjectStoreGetExt>,
    archive_reader_metrics: Arc<ArchiveReaderMetrics>,
}

impl ArchiveReader {
    pub fn new(config: ArchiveReaderConfig, metrics: &Arc<ArchiveReaderMetrics>) -> Result<Self> {
        let bucket = config
            .remote_store_config
            .bucket
            .clone()
            .unwrap_or("unknown".to_string());
        let remote_object_store = if config.remote_store_config.no_sign_request {
            config.remote_store_config.make_http()?
        } else {
            config.remote_store_config.make().map(Arc::new)?
        };
        let (sender, recv) = oneshot::channel();
        let manifest = Arc::new(Mutex::new(Manifest::new(0, 0)));
        // Start a background tokio task to keep local manifest in sync with remote
        Self::spawn_manifest_sync_task(remote_object_store.clone(), manifest.clone(), recv);
        Ok(ArchiveReader {
            bucket,
            manifest,
            sender: Arc::new(sender),
            remote_object_store,
            use_for_pruning_watermark: config.use_for_pruning_watermark,
            concurrency: config.download_concurrency.get(),
            archive_reader_metrics: metrics.clone(),
        })
    }

    /// This function verifies that the files in archive cover the entire range of checkpoints from
    /// sequence number 0 until the latest available checkpoint with no missing checkpoint
    pub async fn verify_manifest(
        &self,
        manifest: Manifest,
    ) -> Result<Vec<(FileMetadata, FileMetadata)>> {
        let files = manifest.files();
        if files.is_empty() {
            return Err(anyhow!("Unexpected empty archive store"));
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

        let files: Vec<(FileMetadata, FileMetadata)> = summary_files
            .into_iter()
            .zip(contents_files.into_iter())
            .map(|(s, c)| {
                assert_eq!(s.checkpoint_seq_range, c.checkpoint_seq_range);
                (s, c)
            })
            .collect();

        assert_eq!(files.first().unwrap().0.checkpoint_seq_range.start, 0);

        Ok(files)
    }

    /// This function downloads summary and content files and ensures their computed checksum matches
    /// the one in manifest
    pub async fn verify_file_consistency(
        &self,
        files: Vec<(FileMetadata, FileMetadata)>,
    ) -> Result<()> {
        let remote_object_store = self.remote_object_store.clone();
        futures::stream::iter(files.iter())
            .enumerate()
            .map(|(_, (summary_metadata, content_metadata))| {
                let remote_object_store = remote_object_store.clone();
                async move {
                    let summary_data =
                        get(&remote_object_store, &summary_metadata.file_path()).await?;
                    let content_data =
                        get(&remote_object_store, &content_metadata.file_path()).await?;
                    Ok::<((Bytes, &FileMetadata), (Bytes, &FileMetadata)), anyhow::Error>((
                        (summary_data, summary_metadata),
                        (content_data, content_metadata),
                    ))
                }
            })
            .boxed()
            .buffer_unordered(self.concurrency)
            .try_for_each(
                |((summary_data, summary_metadata), (content_data, content_metadata))| {
                    let checksums = compute_sha3_checksum_for_bytes(summary_data).and_then(|s| {
                        compute_sha3_checksum_for_bytes(content_data).map(|c| (s, c))
                    });
                    let result = checksums.and_then(|(summary_checksum, content_checksum)| {
                        (summary_checksum == summary_metadata.sha3_digest)
                            .then_some(())
                            .ok_or(anyhow!(
                                "Summary checksum doesn't match for file: {:?}",
                                summary_metadata.file_path()
                            ))?;
                        (content_checksum == content_metadata.sha3_digest)
                            .then_some(())
                            .ok_or(anyhow!(
                                "Content checksum doesn't match for file: {:?}",
                                content_metadata.file_path()
                            ))?;
                        Ok::<(), anyhow::Error>(())
                    });
                    futures::future::ready(result)
                },
            )
            .await
    }

    /// Load checkpoints from archive into the input store `S` for the given checkpoint
    /// range. Summaries are downloaded out of order and inserted without verification
    pub async fn read_summaries_for_range_no_verify<S>(
        &self,
        store: S,
        checkpoint_range: Range<CheckpointSequenceNumber>,
        checkpoint_counter: Arc<AtomicU64>,
    ) -> Result<()>
    where
        S: WriteStore + Clone,
    {
        let (summary_files, start_index, end_index) = self
            .get_summary_files_for_range(checkpoint_range.clone())
            .await?;
        let remote_object_store = self.remote_object_store.clone();
        let stream = futures::stream::iter(summary_files.iter())
            .enumerate()
            .filter(|(index, _s)| future::ready(*index >= start_index && *index < end_index))
            .map(|(_, summary_metadata)| {
                let remote_object_store = remote_object_store.clone();
                async move {
                    let summary_data =
                        get(&remote_object_store, &summary_metadata.file_path()).await?;
                    Ok::<Bytes, anyhow::Error>(summary_data)
                }
            })
            .boxed();
        stream
            .buffer_unordered(self.concurrency)
            .try_for_each(|summary_data| {
                let result: Result<(), anyhow::Error> =
                    make_iterator::<CertifiedCheckpointSummary, Reader<Bytes>>(
                        SUMMARY_FILE_MAGIC,
                        summary_data.reader(),
                    )
                    .and_then(|summary_iter| {
                        summary_iter
                            .filter(|s| {
                                s.sequence_number >= checkpoint_range.start
                                    && s.sequence_number < checkpoint_range.end
                            })
                            .try_for_each(|summary| {
                                Self::insert_certified_checkpoint(&store, summary)?;
                                checkpoint_counter.fetch_add(1, Ordering::Relaxed);
                                Ok::<(), anyhow::Error>(())
                            })
                    });
                futures::future::ready(result)
            })
            .await
    }

    /// Load given list of checkpoints from archive into the input store `S`.
    /// Summaries are downloaded out of order and inserted without verification
    pub async fn read_summaries_for_list_no_verify<S>(
        &self,
        store: S,
        skiplist: Vec<CheckpointSequenceNumber>,
        checkpoint_counter: Arc<AtomicU64>,
    ) -> Result<()>
    where
        S: WriteStore + Clone,
    {
        let summary_files = self.get_summary_files_for_list(skiplist.clone()).await?;
        let remote_object_store = self.remote_object_store.clone();
        let stream = futures::stream::iter(summary_files.iter())
            .map(|summary_metadata| {
                let remote_object_store = remote_object_store.clone();
                async move {
                    let summary_data =
                        get(&remote_object_store, &summary_metadata.file_path()).await?;
                    Ok::<Bytes, anyhow::Error>(summary_data)
                }
            })
            .boxed();

        stream
            .buffer_unordered(self.concurrency)
            .try_for_each(|summary_data| {
                let result: Result<(), anyhow::Error> =
                    make_iterator::<CertifiedCheckpointSummary, Reader<Bytes>>(
                        SUMMARY_FILE_MAGIC,
                        summary_data.reader(),
                    )
                    .and_then(|summary_iter| {
                        summary_iter
                            .filter(|s| skiplist.contains(&s.sequence_number))
                            .try_for_each(|summary| {
                                Self::insert_certified_checkpoint(&store, summary)?;
                                checkpoint_counter.fetch_add(1, Ordering::Relaxed);
                                Ok::<(), anyhow::Error>(())
                            })
                    });
                futures::future::ready(result)
            })
            .await
    }

    pub async fn get_summaries_for_list_no_verify(
        &self,
        cp_list: Vec<CheckpointSequenceNumber>,
    ) -> Result<Vec<CertifiedCheckpointSummary>> {
        let summary_files = self.get_summary_files_for_list(cp_list.clone()).await?;
        let remote_object_store = self.remote_object_store.clone();
        let stream = futures::stream::iter(summary_files.iter())
            .map(|summary_metadata| {
                let remote_object_store = remote_object_store.clone();
                async move {
                    let summary_data =
                        get(&remote_object_store, &summary_metadata.file_path()).await?;
                    Ok::<Bytes, anyhow::Error>(summary_data)
                }
            })
            .boxed();

        stream
            .buffer_unordered(self.concurrency)
            .try_fold(Vec::new(), |mut acc, summary_data| async move {
                let summary_result: Result<Vec<CertifiedCheckpointSummary>, anyhow::Error> =
                    make_iterator::<CertifiedCheckpointSummary, Reader<Bytes>>(
                        SUMMARY_FILE_MAGIC,
                        summary_data.reader(),
                    )
                    .map(|summary_iter| summary_iter.collect::<Vec<_>>());

                match summary_result {
                    Ok(summaries) => {
                        acc.extend(summaries);
                        Ok(acc)
                    }
                    Err(e) => Err(e),
                }
            })
            .await
    }

    /// Load checkpoints+txns+effects from archive into the input store `S` for the given
    /// checkpoint range. If latest available checkpoint in archive is older than the start of the
    /// input range then this call fails with an error otherwise we load as many checkpoints as
    /// possible until the end of the provided checkpoint range.
    pub async fn read<S>(
        &self,
        store: S,
        checkpoint_range: Range<CheckpointSequenceNumber>,
        txn_counter: Arc<AtomicU64>,
        checkpoint_counter: Arc<AtomicU64>,
        verify: bool,
    ) -> Result<()>
    where
        S: WriteStore + Clone,
    {
        let manifest = self.manifest.lock().await.clone();

        let latest_available_checkpoint = manifest
            .next_checkpoint_seq_num()
            .checked_sub(1)
            .context("Checkpoint seq num underflow")?;

        if checkpoint_range.start > latest_available_checkpoint {
            return Err(anyhow!(
                "Latest available checkpoint is: {}",
                latest_available_checkpoint
            ));
        }

        let files: Vec<(FileMetadata, FileMetadata)> = self.verify_manifest(manifest).await?;

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
        futures::stream::iter(files.iter())
            .enumerate()
            .filter(|(index, (_s, _c))| future::ready(*index >= start_index && *index < end_index))
            .map(|(_, (summary_metadata, content_metadata))| {
                let remote_object_store = remote_object_store.clone();
                async move {
                    let summary_data =
                        get(&remote_object_store, &summary_metadata.file_path()).await?;
                    let content_data =
                        get(&remote_object_store, &content_metadata.file_path()).await?;
                    Ok::<(Bytes, Bytes), anyhow::Error>((summary_data, content_data))
                }
            })
            .boxed()
            .buffered(self.concurrency)
            .try_for_each(|(summary_data, content_data)| {
                let result: Result<(), anyhow::Error> = make_iterator::<
                    CertifiedCheckpointSummary,
                    Reader<Bytes>,
                >(
                    SUMMARY_FILE_MAGIC, summary_data.reader()
                )
                .and_then(|s| {
                    make_iterator::<CheckpointContents, Reader<Bytes>>(
                        CHECKPOINT_FILE_MAGIC,
                        content_data.reader(),
                    )
                    .map(|c| (s, c))
                })
                .and_then(|(summary_iter, content_iter)| {
                    summary_iter
                        .zip(content_iter)
                        .filter(|(s, _c)| {
                            s.sequence_number >= checkpoint_range.start
                                && s.sequence_number < checkpoint_range.end
                        })
                        .try_for_each(|(summary, contents)| {
                            let verified_checkpoint =
                                Self::get_or_insert_verified_checkpoint(&store, summary, verify)?;
                            // Verify content
                            let digest = verified_checkpoint.content_digest;
                            contents.verify_digests(digest)?;
                            let verified_contents =
                                VerifiedCheckpointContents::new_unchecked(contents.clone());
                            // Insert content
                            store
                                .insert_checkpoint_contents(&verified_checkpoint, verified_contents)
                                .map_err(|e| anyhow!("Failed to insert content: {e}"))?;
                            // Update highest synced watermark
                            store
                                .update_highest_synced_checkpoint(&verified_checkpoint)
                                .map_err(|e| anyhow!("Failed to update watermark: {e}"))?;
                            txn_counter.fetch_add(contents.size() as u64, Ordering::Relaxed);
                            self.archive_reader_metrics
                                .archive_txns_read
                                .with_label_values(&[&self.bucket])
                                .inc_by(contents.size() as u64);
                            checkpoint_counter.fetch_add(1, Ordering::Relaxed);
                            self.archive_reader_metrics
                                .archive_checkpoints_read
                                .with_label_values(&[&self.bucket])
                                .inc_by(1);
                            Ok::<(), anyhow::Error>(())
                        })
                });
                futures::future::ready(result)
            })
            .await
    }

    /// Return latest available checkpoint in archive
    pub async fn latest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber> {
        let manifest = self.manifest.lock().await.clone();
        manifest
            .next_checkpoint_seq_num()
            .checked_sub(1)
            .context("No checkpoint data in archive")
    }

    pub fn use_for_pruning_watermark(&self) -> bool {
        self.use_for_pruning_watermark
    }

    pub fn remote_store_identifier(&self) -> String {
        self.remote_object_store.to_string()
    }

    pub async fn sync_manifest_once(&self) -> Result<()> {
        Self::sync_manifest(self.remote_object_store.clone(), self.manifest.clone()).await?;
        Ok(())
    }

    pub async fn get_manifest(&self) -> Result<Manifest> {
        Ok(self.manifest.lock().await.clone())
    }

    async fn sync_manifest(
        remote_store: Arc<dyn ObjectStoreGetExt>,
        manifest: Arc<Mutex<Manifest>>,
    ) -> Result<()> {
        let new_manifest = read_manifest(remote_store.clone()).await?;
        let mut locked = manifest.lock().await;
        *locked = new_manifest;
        Ok(())
    }

    /// Insert checkpoint summary without verifying it
    fn insert_certified_checkpoint<S>(
        store: &S,
        certified_checkpoint: CertifiedCheckpointSummary,
    ) -> Result<()>
    where
        S: WriteStore + Clone,
    {
        store
            .insert_checkpoint(VerifiedCheckpoint::new_unchecked(certified_checkpoint).borrow())
            .map_err(|e| anyhow!("Failed to insert checkpoint: {e}"))
    }

    /// Insert checkpoint summary if it doesn't already exist after verifying it
    fn get_or_insert_verified_checkpoint<S>(
        store: &S,
        certified_checkpoint: CertifiedCheckpointSummary,
        verify: bool,
    ) -> Result<VerifiedCheckpoint>
    where
        S: WriteStore + Clone,
    {
        store
            .get_checkpoint_by_sequence_number(certified_checkpoint.sequence_number)
            .map(Ok::<VerifiedCheckpoint, anyhow::Error>)
            .unwrap_or_else(|| {
                let verified_checkpoint = if verify {
                    // Verify checkpoint summary
                    let prev_checkpoint_seq_num = certified_checkpoint
                        .sequence_number
                        .checked_sub(1)
                        .context("Checkpoint seq num underflow")?;
                    let prev_checkpoint = store
                        .get_checkpoint_by_sequence_number(prev_checkpoint_seq_num)
                        .context(format!(
                            "Missing previous checkpoint {} in store",
                            prev_checkpoint_seq_num
                        ))?;

                    verify_checkpoint(&prev_checkpoint, store, certified_checkpoint)
                        .map_err(|_| anyhow!("Checkpoint verification failed"))?
                } else {
                    VerifiedCheckpoint::new_unchecked(certified_checkpoint)
                };
                // Insert checkpoint summary
                store
                    .insert_checkpoint(&verified_checkpoint)
                    .map_err(|e| anyhow!("Failed to insert checkpoint: {e}"))?;
                // Update highest verified checkpoint watermark
                store
                    .update_highest_verified_checkpoint(&verified_checkpoint)
                    .expect("store operation should not fail");
                Ok::<VerifiedCheckpoint, anyhow::Error>(verified_checkpoint)
            })
            .map_err(|e| anyhow!("Failed to get verified checkpoint: {:?}", e))
    }

    async fn get_summary_files_for_range(
        &self,
        checkpoint_range: Range<CheckpointSequenceNumber>,
    ) -> Result<(Vec<FileMetadata>, usize, usize)> {
        let manifest = self.manifest.lock().await.clone();

        let latest_available_checkpoint = manifest
            .next_checkpoint_seq_num()
            .checked_sub(1)
            .context("Checkpoint seq num underflow")?;

        if checkpoint_range.start > latest_available_checkpoint {
            return Err(anyhow!(
                "Latest available checkpoint is: {}",
                latest_available_checkpoint
            ));
        }

        let summary_files: Vec<FileMetadata> = self
            .verify_manifest(manifest)
            .await?
            .iter()
            .map(|(s, _)| s.clone())
            .collect();

        let start_index = match summary_files
            .binary_search_by_key(&checkpoint_range.start, |s| s.checkpoint_seq_range.start)
        {
            Ok(index) => index,
            Err(index) => index - 1,
        };

        let end_index = match summary_files
            .binary_search_by_key(&checkpoint_range.end, |s| s.checkpoint_seq_range.start)
        {
            Ok(index) => index,
            Err(index) => index,
        };

        Ok((summary_files, start_index, end_index))
    }

    async fn get_summary_files_for_list(
        &self,
        checkpoints: Vec<CheckpointSequenceNumber>,
    ) -> Result<Vec<FileMetadata>> {
        assert!(!checkpoints.is_empty());
        let manifest = self.manifest.lock().await.clone();
        let latest_available_checkpoint = manifest
            .next_checkpoint_seq_num()
            .checked_sub(1)
            .context("Checkpoint seq num underflow")?;

        let mut ordered_checkpoints = checkpoints;
        ordered_checkpoints.sort();
        if *ordered_checkpoints.first().unwrap() > latest_available_checkpoint {
            return Err(anyhow!(
                "Latest available checkpoint is: {}",
                latest_available_checkpoint
            ));
        }

        let summary_files: Vec<FileMetadata> = self
            .verify_manifest(manifest)
            .await?
            .iter()
            .map(|(s, _)| s.clone())
            .collect();

        let mut summaries_filtered = vec![];
        for checkpoint in ordered_checkpoints.iter() {
            let index = summary_files
                .binary_search_by(|s| {
                    if checkpoint < &s.checkpoint_seq_range.start {
                        std::cmp::Ordering::Greater
                    } else if checkpoint >= &s.checkpoint_seq_range.end {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .expect("Archive does not contain checkpoint {checkpoint}");
            summaries_filtered.push(summary_files[index].clone());
        }

        Ok(summaries_filtered)
    }

    fn spawn_manifest_sync_task<S: ObjectStoreGetExt + Clone>(
        remote_store: S,
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
}
