// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use crate::{
    create_file_metadata, read_manifest, write_manifest, CheckpointUpdates, FileMetadata, FileType,
    Manifest, CHECKPOINT_FILE_MAGIC, CHECKPOINT_FILE_SUFFIX, EPOCH_DIR_PREFIX, MAGIC_BYTES,
    SUMMARY_FILE_MAGIC, SUMMARY_FILE_SUFFIX,
};
use anyhow::Context;
use anyhow::Result;
use byteorder::{BigEndian, ByteOrder, WriteBytesExt};
use object_store::DynObjectStore;
use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use sui_config::object_storage_config::ObjectStoreConfig;
use sui_storage::blob::{Blob, BlobEncoding};
use sui_storage::object_store::util::{copy_file, path_to_filesystem};
use sui_storage::{compress, FileCompression, StorageFormat};
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary as Checkpoint, CheckpointSequenceNumber,
    FullCheckpointContents as CheckpointContents,
};
use sui_types::storage::WriteStore;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::Instant;
use tracing::{debug, info};

pub struct ArchiveMetrics {
    pub latest_checkpoint_archived: IntGauge,
}

impl ArchiveMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        let this = Self {
            latest_checkpoint_archived: register_int_gauge_with_registry!(
                "latest_checkpoint_archived",
                "Latest checkpoint to have archived to the remote store",
                registry
            )
            .unwrap(),
        };
        Arc::new(this)
    }
}

/// CheckpointWriter writes checkpoints and summaries. It creates multiple *.chk and *.sum files
struct CheckpointWriter {
    root_dir_path: PathBuf,
    epoch_num: u64,
    checkpoint_range: Range<u64>,
    wbuf: BufWriter<File>,
    summary_wbuf: BufWriter<File>,
    sender: Sender<CheckpointUpdates>,
    checkpoint_buf_offset: usize,
    file_compression: FileCompression,
    storage_format: StorageFormat,
    manifest: Manifest,
    last_commit_instant: Instant,
    commit_duration: Duration,
    commit_file_size: usize,
}

impl CheckpointWriter {
    fn new(
        root_dir_path: PathBuf,
        file_compression: FileCompression,
        storage_format: StorageFormat,
        sender: Sender<CheckpointUpdates>,
        manifest: Manifest,
        commit_duration: Duration,
        commit_file_size: usize,
    ) -> Result<Self> {
        let epoch_num = manifest.epoch_num();
        let checkpoint_sequence_num = manifest.next_checkpoint_seq_num();
        let epoch_dir = root_dir_path.join(format!("{}{epoch_num}", EPOCH_DIR_PREFIX));
        if epoch_dir.exists() {
            fs::remove_dir_all(&epoch_dir)?;
        }
        fs::create_dir_all(&epoch_dir)?;
        let checkpoint_file = Self::next_file(
            &epoch_dir,
            checkpoint_sequence_num,
            CHECKPOINT_FILE_SUFFIX,
            CHECKPOINT_FILE_MAGIC,
            storage_format,
            file_compression,
        )?;
        let summary_file = Self::next_file(
            &epoch_dir,
            checkpoint_sequence_num,
            SUMMARY_FILE_SUFFIX,
            SUMMARY_FILE_MAGIC,
            storage_format,
            file_compression,
        )?;
        Ok(CheckpointWriter {
            root_dir_path,
            epoch_num,
            checkpoint_range: checkpoint_sequence_num..checkpoint_sequence_num,
            wbuf: BufWriter::new(checkpoint_file),
            summary_wbuf: BufWriter::new(summary_file),
            checkpoint_buf_offset: 0,
            sender,
            file_compression,
            storage_format,
            manifest,
            last_commit_instant: Instant::now(),
            commit_duration,
            commit_file_size,
        })
    }

    pub fn write(
        &mut self,
        checkpoint_contents: CheckpointContents,
        checkpoint_summary: Checkpoint,
    ) -> Result<()> {
        match self.storage_format {
            StorageFormat::Blob => self.write_as_blob(checkpoint_contents, checkpoint_summary),
        }
    }

    pub fn write_as_blob(
        &mut self,
        checkpoint_contents: CheckpointContents,
        checkpoint_summary: Checkpoint,
    ) -> Result<()> {
        assert_eq!(
            checkpoint_summary.sequence_number,
            self.checkpoint_range.end
        );

        if checkpoint_summary.epoch()
            == self
                .epoch_num
                .checked_add(1)
                .context("Epoch num overflow")?
        {
            self.cut()?;
            self.update_to_next_epoch();
            if self.epoch_dir().exists() {
                fs::remove_dir_all(self.epoch_dir())?;
            }
            fs::create_dir_all(self.epoch_dir())?;
            self.reset()?;
        }

        assert_eq!(checkpoint_summary.epoch, self.epoch_num);

        assert_eq!(
            checkpoint_summary.content_digest,
            *checkpoint_contents.checkpoint_contents().digest()
        );

        let contents_blob = Blob::encode(&checkpoint_contents, BlobEncoding::Bcs)?;
        let blob_size = contents_blob.size();
        let cut_new_checkpoint_file = (self.checkpoint_buf_offset + blob_size)
            > self.commit_file_size
            || (self.last_commit_instant.elapsed() > self.commit_duration);
        if cut_new_checkpoint_file {
            self.cut()?;
            self.reset()?;
        }

        self.checkpoint_buf_offset += contents_blob.write(&mut self.wbuf)?;

        let summary_blob = Blob::encode(&checkpoint_summary, BlobEncoding::Bcs)?;
        summary_blob.write(&mut self.summary_wbuf)?;

        self.checkpoint_range.end = self
            .checkpoint_range
            .end
            .checked_add(1)
            .context("Checkpoint sequence num overflow")?;
        Ok(())
    }
    fn finalize(&mut self) -> Result<FileMetadata> {
        self.wbuf.flush()?;
        self.wbuf.get_ref().sync_data()?;
        let off = self.wbuf.get_ref().stream_position()?;
        self.wbuf.get_ref().set_len(off)?;
        let file_path = self.epoch_dir().join(format!(
            "{}.{CHECKPOINT_FILE_SUFFIX}",
            self.checkpoint_range.start
        ));
        self.compress(&file_path)?;
        let file_metadata = create_file_metadata(
            &file_path,
            FileType::CheckpointContent,
            self.epoch_num,
            self.checkpoint_range.clone(),
        )?;
        Ok(file_metadata)
    }
    fn finalize_summary(&mut self) -> Result<FileMetadata> {
        self.summary_wbuf.flush()?;
        self.summary_wbuf.get_ref().sync_data()?;
        let off = self.summary_wbuf.get_ref().stream_position()?;
        self.summary_wbuf.get_ref().set_len(off)?;
        let file_path = self.epoch_dir().join(format!(
            "{}.{SUMMARY_FILE_SUFFIX}",
            self.checkpoint_range.start
        ));
        self.compress(&file_path)?;
        let file_metadata = create_file_metadata(
            &file_path,
            FileType::CheckpointSummary,
            self.epoch_num,
            self.checkpoint_range.clone(),
        )?;
        Ok(file_metadata)
    }
    fn cut(&mut self) -> Result<()> {
        if !self.checkpoint_range.is_empty() {
            let checkpoint_file_metadata = self.finalize()?;
            let summary_file_metadata = self.finalize_summary()?;
            let checkpoint_updates = CheckpointUpdates::new(
                self.epoch_num,
                self.checkpoint_range.end,
                checkpoint_file_metadata,
                summary_file_metadata,
                &mut self.manifest,
            );
            info!("Checkpoint file cut for: {:?}", checkpoint_updates);
            self.sender.blocking_send(checkpoint_updates)?;
        }
        Ok(())
    }
    fn compress(&self, source: &Path) -> Result<()> {
        if self.file_compression == FileCompression::None {
            return Ok(());
        }
        let mut input = File::open(source)?;
        let tmp_file_name = source.with_extension("tmp");
        let mut output = File::create(&tmp_file_name)?;
        compress(&mut input, &mut output)?;
        fs::rename(tmp_file_name, source)?;
        Ok(())
    }
    fn next_file(
        dir_path: &Path,
        checkpoint_sequence_num: u64,
        suffix: &str,
        magic_bytes: u32,
        storage_format: StorageFormat,
        file_compression: FileCompression,
    ) -> Result<File> {
        let next_file_path = dir_path.join(format!("{checkpoint_sequence_num}.{suffix}"));
        let mut f = File::create(next_file_path.clone())?;
        let mut metab = [0u8; MAGIC_BYTES];
        BigEndian::write_u32(&mut metab, magic_bytes);
        let n = f.write(&metab)?;
        drop(f);
        f = OpenOptions::new().append(true).open(next_file_path)?;
        f.seek(SeekFrom::Start(n as u64))?;
        f.write_u8(storage_format.into())?;
        f.write_u8(file_compression.into())?;
        Ok(f)
    }
    fn create_new_files(&mut self) -> Result<()> {
        let f = Self::next_file(
            &self.epoch_dir(),
            self.checkpoint_range.start,
            CHECKPOINT_FILE_SUFFIX,
            CHECKPOINT_FILE_MAGIC,
            self.storage_format,
            self.file_compression,
        )?;
        self.checkpoint_buf_offset = MAGIC_BYTES;
        self.wbuf = BufWriter::new(f);
        let f = Self::next_file(
            &self.epoch_dir(),
            self.checkpoint_range.start,
            SUMMARY_FILE_SUFFIX,
            SUMMARY_FILE_MAGIC,
            self.storage_format,
            self.file_compression,
        )?;
        self.summary_wbuf = BufWriter::new(f);
        Ok(())
    }
    fn reset(&mut self) -> Result<()> {
        self.reset_checkpoint_range();
        self.create_new_files()?;
        self.reset_last_commit_ts();
        Ok(())
    }
    fn reset_last_commit_ts(&mut self) {
        self.last_commit_instant = Instant::now();
    }
    fn reset_checkpoint_range(&mut self) {
        self.checkpoint_range = self.checkpoint_range.end..self.checkpoint_range.end
    }
    fn epoch_dir(&self) -> PathBuf {
        self.root_dir_path
            .join(format!("{}{}", EPOCH_DIR_PREFIX, self.epoch_num))
    }
    fn update_to_next_epoch(&mut self) {
        self.epoch_num = self.epoch_num.checked_add(1).unwrap();
    }
}

/// ArchiveWriter archives history by tailing checkpoints writing them to a local staging dir and
/// simultaneously uploading them to a remote object store
pub struct ArchiveWriter {
    file_compression: FileCompression,
    storage_format: StorageFormat,
    local_staging_dir_root: PathBuf,
    local_object_store: Arc<DynObjectStore>,
    remote_object_store: Arc<DynObjectStore>,
    commit_duration: Duration,
    commit_file_size: usize,
    archive_metrics: Arc<ArchiveMetrics>,
}

impl ArchiveWriter {
    pub async fn new(
        local_store_config: ObjectStoreConfig,
        remote_store_config: ObjectStoreConfig,
        file_compression: FileCompression,
        storage_format: StorageFormat,
        commit_duration: Duration,
        commit_file_size: usize,
        registry: &Registry,
    ) -> Result<Self> {
        Ok(ArchiveWriter {
            file_compression,
            storage_format,
            remote_object_store: remote_store_config.make()?,
            local_object_store: local_store_config.make()?,
            local_staging_dir_root: local_store_config.directory.context("Missing local dir")?,
            commit_duration,
            commit_file_size,
            archive_metrics: ArchiveMetrics::new(registry),
        })
    }

    pub async fn start<S>(&self, store: S) -> Result<tokio::sync::broadcast::Sender<()>>
    where
        S: WriteStore + Send + Sync + 'static,
    {
        let remote_archive_is_empty = self
            .remote_object_store
            .list_with_delimiter(None)
            .await
            .expect("Failed to read remote archive dir")
            .common_prefixes
            .is_empty();
        let manifest = if remote_archive_is_empty {
            // Start from genesis
            Manifest::new(0, 0)
        } else {
            read_manifest(self.remote_object_store.clone())
                .await
                .expect("Failed to read manifest")
        };
        let start_checkpoint_sequence_number = manifest.next_checkpoint_seq_num();
        let (sender, receiver) = mpsc::channel::<CheckpointUpdates>(100);
        let checkpoint_writer = CheckpointWriter::new(
            self.local_staging_dir_root.clone(),
            self.file_compression,
            self.storage_format,
            sender,
            manifest,
            self.commit_duration,
            self.commit_file_size,
        )
        .expect("Failed to create checkpoint writer");
        let (kill_sender, kill_receiver) = tokio::sync::broadcast::channel::<()>(1);
        tokio::spawn(Self::start_syncing_with_remote(
            self.remote_object_store.clone(),
            self.local_object_store.clone(),
            self.local_staging_dir_root.clone(),
            receiver,
            kill_sender.subscribe(),
            self.archive_metrics.clone(),
        ));
        tokio::task::spawn_blocking(move || {
            Self::start_tailing_checkpoints(
                start_checkpoint_sequence_number,
                checkpoint_writer,
                store,
                kill_receiver,
            )
        });
        Ok(kill_sender)
    }

    fn start_tailing_checkpoints<S>(
        start_checkpoint_sequence_number: CheckpointSequenceNumber,
        mut checkpoint_writer: CheckpointWriter,
        store: S,
        mut kill: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()>
    where
        S: WriteStore + Send + Sync + 'static,
    {
        let mut checkpoint_sequence_number = start_checkpoint_sequence_number;
        info!("Starting checkpoint tailing from sequence number: {checkpoint_sequence_number}");

        while kill.try_recv().is_err() {
            if let Some(checkpoint_summary) =
                store.get_checkpoint_by_sequence_number(checkpoint_sequence_number)
            {
                if let Some(checkpoint_contents) =
                    store.get_full_checkpoint_contents(&checkpoint_summary.content_digest)
                {
                    checkpoint_writer
                        .write(checkpoint_contents, checkpoint_summary.into_inner())?;
                    checkpoint_sequence_number = checkpoint_sequence_number
                        .checked_add(1)
                        .context("checkpoint seq number overflow")?;
                    // There is more checkpoints to tail, so continue without sleeping
                    continue;
                }
            }
            // Checkpoint with `checkpoint_sequence_number` is not available to read from store yet,
            // sleep for sometime and then retry
            sleep(Duration::from_secs(3));
        }
        Ok(())
    }

    async fn start_syncing_with_remote(
        remote_object_store: Arc<DynObjectStore>,
        local_object_store: Arc<DynObjectStore>,
        local_staging_root_dir: PathBuf,
        mut update_receiver: Receiver<CheckpointUpdates>,
        mut kill: tokio::sync::broadcast::Receiver<()>,
        metrics: Arc<ArchiveMetrics>,
    ) -> Result<()> {
        loop {
            tokio::select! {
                _ = kill.recv() => break,
                updates = update_receiver.recv() => {
                    if let Some(checkpoint_updates) = updates {
                        info!("Received checkpoint update: {:?}", checkpoint_updates);
                        let latest_checkpoint_seq_num = checkpoint_updates.manifest.next_checkpoint_seq_num();
                        let summary_file_path = checkpoint_updates.summary_file_path();
                        Self::sync_file_to_remote(
                            local_staging_root_dir.clone(),
                            summary_file_path,
                            local_object_store.clone(),
                            remote_object_store.clone()
                        )
                        .await
                        .expect("Syncing checkpoint summary should not fail");

                        let content_file_path = checkpoint_updates.content_file_path();
                        Self::sync_file_to_remote(
                            local_staging_root_dir.clone(),
                            content_file_path,
                            local_object_store.clone(),
                            remote_object_store.clone()
                        )
                        .await
                        .expect("Syncing checkpoint content should not fail");

                        write_manifest(
                            checkpoint_updates.manifest,
                            remote_object_store.clone()
                        )
                        .await
                        .expect("Updating manifest should not fail");
                        metrics.latest_checkpoint_archived.set(latest_checkpoint_seq_num as i64)
                    } else {
                        info!("Terminating archive sync loop");
                        break;
                    }
                },
            }
        }
        Ok(())
    }

    async fn sync_file_to_remote(
        dir: PathBuf,
        path: object_store::path::Path,
        from: Arc<DynObjectStore>,
        to: Arc<DynObjectStore>,
    ) -> Result<()> {
        debug!("Syncing archive file to remote: {:?}", path);
        copy_file(&path, &path, &from, &to).await?;
        fs::remove_file(path_to_filesystem(dir, &path)?)?;
        Ok(())
    }
}
