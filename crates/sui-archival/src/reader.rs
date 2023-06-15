// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    read_manifest, FileMetadata, FileType, Manifest, CHECKPOINT_FILE_MAGIC, SUMMARY_FILE_MAGIC,
};
use anyhow::{anyhow, Context, Result};
use bytes::buf::Reader;
use bytes::{Buf, Bytes};
use futures::{StreamExt, TryStreamExt};
use object_store::DynObjectStore;
use std::future;
use std::num::NonZeroUsize;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sui_storage::object_store::util::get;
use sui_storage::object_store::ObjectStoreConfig;
use sui_storage::{make_iterator, verify_checkpoint};
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointSequenceNumber,
    FullCheckpointContents as CheckpointContents, VerifiedCheckpoint, VerifiedCheckpointContents,
};
use sui_types::storage::{ReadStore, WriteStore};
use tokio::sync::oneshot::Sender;
use tokio::sync::{oneshot, Mutex};
use tracing::info;

pub struct ArchiveReader {
    concurrency: usize,
    sender: Sender<()>,
    manifest: Arc<Mutex<Manifest>>,
    remote_object_store: Arc<DynObjectStore>,
}

impl ArchiveReader {
    pub fn new(
        remote_store_config: ObjectStoreConfig,
        download_concurrency: NonZeroUsize,
    ) -> Result<Self> {
        let remote_object_store = remote_store_config.make()?;
        let (sender, recv) = oneshot::channel();
        let manifest = Arc::new(Mutex::new(Manifest::new(0, 0)));
        // Start a background tokio task to keep local manifest in sync with remote
        Self::spawn_manifest_sync_task(remote_object_store.clone(), manifest.clone(), recv);
        Ok(ArchiveReader {
            manifest,
            sender,
            remote_object_store,
            concurrency: download_concurrency.get(),
        })
    }

    /// Load checkpoints+txns+effects from archive into the input store `S` for the given
    /// checkpoint range. If latest available checkpoint in archive is older than the start of the
    /// input range then this call fails with an error otherwise we load as many checkpoints as
    /// possible until the end of the provided checkpoint range.
    pub async fn read<S>(
        &mut self,
        store: S,
        checkpoint_range: Range<CheckpointSequenceNumber>,
        txn_counter: Arc<AtomicU64>,
    ) -> Result<()>
    where
        S: WriteStore + Clone,
        <S as ReadStore>::Error: std::error::Error,
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
                        get(&summary_metadata.file_path(), remote_object_store.clone()).await?;
                    let content_data =
                        get(&content_metadata.file_path(), remote_object_store.clone()).await?;
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
                                Self::get_or_insert_verified_checkpoint(&store, summary)?;
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

    /// Insert checkpoint summary if it doesn't already exist after verifying it
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
}
