// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use byteorder::BigEndian;
use byteorder::ByteOrder;
use bytes::Bytes;
use object_store::path::Path;
use object_store::ObjectStore;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::ops::Range;
use sui_archival::{
    create_file_metadata_from_bytes, finalize_manifest, read_manifest_from_bytes, FileType,
    Manifest, CHECKPOINT_FILE_MAGIC, SUMMARY_FILE_MAGIC,
};
use sui_data_ingestion_core::{create_remote_store_client, Worker};
use sui_storage::blob::{Blob, BlobEncoding};
use sui_storage::{compress, FileCompression, StorageFormat};
use sui_types::base_types::{EpochId, ExecutionData};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::{CheckpointSequenceNumber, FullCheckpointContents};
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ArchivalConfig {
    pub remote_url: String,
    pub remote_store_options: Vec<(String, String)>,
    pub commit_file_size: usize,
    pub commit_duration_seconds: u64,
}

struct AccumulatedState {
    epoch: EpochId,
    checkpoint_range: Range<u64>,
    buffer: Vec<u8>,
    summary_buffer: Vec<u8>,
    last_commit_ms: u64,
    should_update_progress: bool,
}

pub struct ArchivalWorker {
    remote_store: Box<dyn ObjectStore>,
    state: Mutex<AccumulatedState>,
    commit_file_size: usize,
    commit_duration_ms: u64,
}

impl ArchivalWorker {
    pub async fn new(config: ArchivalConfig) -> Result<Self> {
        let remote_store =
            create_remote_store_client(config.remote_url, config.remote_store_options, 10)?;
        let manifest = Self::read_manifest(&remote_store).await?;
        let state = AccumulatedState {
            epoch: manifest.epoch_num(),
            checkpoint_range: manifest.next_checkpoint_seq_num()
                ..manifest.next_checkpoint_seq_num(),
            buffer: vec![],
            summary_buffer: vec![],
            last_commit_ms: 0,
            should_update_progress: false,
        };
        Ok(Self {
            remote_store,
            state: Mutex::new(state),
            commit_file_size: config.commit_file_size,
            commit_duration_ms: config.commit_duration_seconds * 1000,
        })
    }

    async fn read_manifest(remote_store: &dyn ObjectStore) -> Result<Manifest> {
        Ok(match remote_store.get(&Path::from("MANIFEST")).await {
            Ok(resp) => read_manifest_from_bytes(resp.bytes().await?.to_vec())?,
            Err(err) if err.to_string().contains("404") => Manifest::new(0, 0),
            Err(err) => Err(err)?,
        })
    }

    async fn upload(&self, state: &AccumulatedState) -> Result<()> {
        let checkpoint_file_path =
            format!("epoch_{}/{}.chk", state.epoch, state.checkpoint_range.start);
        let chk_bytes = self
            .upload_file(
                Path::from(checkpoint_file_path.clone()),
                CHECKPOINT_FILE_MAGIC,
                &state.buffer,
            )
            .await?;
        let summary_file_path =
            format!("epoch_{}/{}.sum", state.epoch, state.checkpoint_range.start);
        let sum_bytes = self
            .upload_file(
                Path::from(summary_file_path.clone()),
                SUMMARY_FILE_MAGIC,
                &state.summary_buffer,
            )
            .await?;
        let mut manifest = Self::read_manifest(&self.remote_store).await?;
        let checkpoint_file_metadata = create_file_metadata_from_bytes(
            chk_bytes,
            FileType::CheckpointContent,
            state.epoch,
            state.checkpoint_range.clone(),
        )?;
        let summary_file_metadata = create_file_metadata_from_bytes(
            sum_bytes,
            FileType::CheckpointSummary,
            state.epoch,
            state.checkpoint_range.clone(),
        )?;
        manifest.update(
            state.epoch,
            state.checkpoint_range.end,
            checkpoint_file_metadata,
            summary_file_metadata,
        );

        let bytes = finalize_manifest(manifest)?;
        self.remote_store
            .put(&Path::from("MANIFEST"), bytes.into())
            .await?;
        Ok(())
    }

    async fn upload_file(&self, location: Path, magic: u32, content: &[u8]) -> Result<Bytes> {
        let mut buffer = vec![0; 4];
        BigEndian::write_u32(&mut buffer, magic);
        buffer.push(StorageFormat::Blob.into());
        buffer.push(FileCompression::Zstd.into());
        buffer.extend_from_slice(content);
        let mut compressed_buffer = vec![];
        let mut cursor = Cursor::new(buffer);
        compress(&mut cursor, &mut compressed_buffer)?;
        self.remote_store
            .put(&location, Bytes::from(compressed_buffer.clone()).into())
            .await?;
        Ok(Bytes::from(compressed_buffer))
    }

    pub async fn initial_checkpoint_number(&self) -> CheckpointSequenceNumber {
        self.state.lock().await.checkpoint_range.start
    }
}

#[async_trait]
impl Worker for ArchivalWorker {
    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> Result<()> {
        let mut state = self.state.lock().await;
        let sequence_number = checkpoint.checkpoint_summary.sequence_number;
        if sequence_number < state.checkpoint_range.start {
            return Ok(());
        }
        let epoch = checkpoint.checkpoint_summary.epoch;
        if state.buffer.is_empty() {
            assert!(epoch == state.epoch || epoch == state.epoch + 1);
            state.epoch = epoch;
            state.last_commit_ms = checkpoint.checkpoint_summary.timestamp_ms;
        }
        let full_checkpoint_contents = FullCheckpointContents::from_contents_and_execution_data(
            checkpoint.checkpoint_contents.clone(),
            checkpoint
                .transactions
                .iter()
                .map(|t| ExecutionData::new(t.transaction.clone(), t.effects.clone())),
        );
        let contents_blob = Blob::encode(&full_checkpoint_contents, BlobEncoding::Bcs)?;
        let blob_size = contents_blob.size();
        let summary_blob = Blob::encode(&checkpoint.checkpoint_summary, BlobEncoding::Bcs)?;

        if !state.buffer.is_empty()
            && (((state.buffer.len() + blob_size) > self.commit_file_size)
                || state.epoch != epoch
                || checkpoint.checkpoint_summary.timestamp_ms
                    > (self.commit_duration_ms + state.last_commit_ms))
        {
            self.upload(&state).await?;
            state.epoch = epoch;
            state.checkpoint_range = sequence_number..sequence_number;
            state.buffer = vec![];
            state.summary_buffer = vec![];
            state.last_commit_ms = checkpoint.checkpoint_summary.timestamp_ms;
            state.should_update_progress = true;
        }
        contents_blob.write(&mut state.buffer)?;
        summary_blob.write(&mut state.summary_buffer)?;
        state.checkpoint_range.end += 1;
        Ok(())
    }

    async fn save_progress(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointSequenceNumber> {
        let mut state = self.state.lock().await;
        let should_update_progress = state.should_update_progress;
        state.should_update_progress = false;
        if should_update_progress && sequence_number > 0 {
            Some(sequence_number - 1)
        } else {
            None
        }
    }
}
