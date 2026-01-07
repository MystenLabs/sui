// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Async upload worker for analytics files.
//!
//! This module provides a background worker that handles file serialization and upload
//! asynchronously. The worker is split into two concurrent tasks:
//! - Serialization task: receives batches, serializes in parallel, sends to uploader
//! - Upload task: receives serialized files, reorders them, uploads sequentially
//!
//! This separation allows serialization to continue while uploads are in progress.

use std::collections::BTreeMap;
use std::ops::Range;
use std::time::Duration;

use anyhow::Result;
use bytes::Bytes;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use object_store::PutPayload;
use object_store::path::Path as ObjectPath;
use sui_types::base_types::EpochId;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

use crate::config::FileFormat;
use crate::config::IndexerConfig;
use crate::handlers::CheckpointRows;
use crate::handlers::record_file_metrics;
use crate::metrics::Metrics;
use crate::store::StoreMode;
use crate::store::WatermarkUpdateError;
use crate::store::construct_object_store_path;
use crate::writers::CsvWriter;
use crate::writers::ParquetWriter;

/// Initial backoff delay for retries.
const INITIAL_BACKOFF: Duration = Duration::from_millis(100);
/// Maximum backoff delay for retries.
const MAX_BACKOFF: Duration = Duration::from_secs(60 * 5);

/// Helper for exponential backoff with jitter.
pub(crate) struct Backoff {
    current: Duration,
}

impl Backoff {
    pub(crate) fn new() -> Self {
        Self {
            current: INITIAL_BACKOFF,
        }
    }

    /// Sleep for the current backoff duration, then increase it.
    pub(crate) async fn sleep_and_advance(&mut self) {
        tokio::time::sleep(self.current).await;
        self.current = (self.current * 2).min(MAX_BACKOFF);
    }

    /// Get the current backoff duration (for logging).
    pub(crate) fn current_ms(&self) -> u128 {
        self.current.as_millis()
    }
}

/// The rows for a checkpoint, ready to be serialized and uploaded.
pub struct PendingFileUpload {
    pub epoch: EpochId,
    pub checkpoint_range: Range<u64>,
    pub file_format: FileFormat,
    pub checkpoints_rows: Vec<CheckpointRows>,
    pub schema: &'static [&'static str],
}

/// A serialized file ready for upload, tagged with sequence number for ordering.
struct SerializedFile {
    seq: u64,
    epoch: EpochId,
    checkpoint_range: Range<u64>,
    file_format: FileFormat,
    bytes: Bytes,
}

/// Spawn an upload worker for a pipeline.
///
/// Returns the sender for queueing files and the worker's JoinHandle for shutdown.
pub fn spawn_uploader(
    pipeline_name: String,
    output_prefix: String,
    mode: StoreMode,
    metrics: Metrics,
    config: &IndexerConfig,
) -> (mpsc::Sender<PendingFileUpload>, JoinHandle<()>) {
    // Size 1: backpressure to handler, limits unserialized batches in memory
    let (tx, rx) = mpsc::channel(1);

    // Channel between serialization and upload tasks.
    // When full, serialization blocks until uploads complete.
    let (upload_tx, upload_rx) = mpsc::channel(config.max_pending_uploads);

    let dispatcher = Dispatcher::new(
        rx,
        upload_tx,
        pipeline_name.clone(),
        config.max_concurrent_serialization,
    );

    let uploader = SequentialUploader::new(
        upload_rx,
        pipeline_name,
        output_prefix,
        mode,
        metrics,
        Duration::from_secs(config.watermark_update_interval_secs),
    );

    // Spawn both tasks, join them together
    let worker_handle = tokio::spawn(async move {
        let dispatcher_handle = tokio::spawn(dispatcher.run());
        let uploader_handle = tokio::spawn(uploader.run());

        // Wait for both to complete
        let _ = dispatcher_handle.await;
        let _ = uploader_handle.await;
    });

    (tx, worker_handle)
}

/// Dispatches serialization tasks and forwards results to the uploader.
/// Serialization runs in parallel on the blocking thread pool.
/// Ordering is handled by the uploader via sequence numbers.
struct Dispatcher {
    rx: mpsc::Receiver<PendingFileUpload>,
    upload_tx: mpsc::Sender<SerializedFile>,
    /// Next sequence number to assign (used by uploader for ordering)
    next_seq: u64,
    /// Serialization tasks in flight
    serializing: FuturesUnordered<JoinHandle<Result<SerializedFile>>>,
    /// Pipeline name (for logging)
    pipeline: String,
    /// Maximum concurrent serialization tasks
    max_concurrent_serialization: usize,
}

impl Dispatcher {
    fn new(
        rx: mpsc::Receiver<PendingFileUpload>,
        upload_tx: mpsc::Sender<SerializedFile>,
        pipeline: String,
        max_concurrent_serialization: usize,
    ) -> Self {
        Self {
            rx,
            upload_tx,
            next_seq: 0,
            serializing: FuturesUnordered::new(),
            pipeline,
            max_concurrent_serialization,
        }
    }

    async fn run(mut self) {
        debug!(pipeline = %self.pipeline, "Dispatcher starting");

        loop {
            let serializing_inflight = self.serializing.len();

            tokio::select! {
                // Receive new batch to serialize (only if below concurrency limit)
                Some(pending) = self.rx.recv(), if serializing_inflight < self.max_concurrent_serialization => {
                    self.spawn_serialization(pending);
                }

                // Serialization task completed - send to uploader
                Some(join_result) = self.serializing.next(), if !self.serializing.is_empty() => {
                    if self.forward_to_uploader(join_result).await.is_err() {
                        return;
                    }
                }

                // Channel closed and no more serialization work
                else => {
                    if self.serializing.is_empty() {
                        break;
                    }
                }
            }
        }

        // Drain remaining serialization tasks
        while let Some(join_result) = self.serializing.next().await {
            if let Ok(Ok(serialized)) = join_result {
                let _ = self.upload_tx.send(serialized).await;
            }
        }

        debug!(pipeline = %self.pipeline, "Dispatcher finished");
        // upload_tx drops here, signaling uploader to finish
    }

    fn spawn_serialization(&mut self, pending: PendingFileUpload) {
        let seq = self.next_seq;
        self.next_seq += 1;

        let pipeline = self.pipeline.clone();

        let handle = tokio::task::spawn_blocking(move || {
            let bytes = serialize_rows(
                &pending.checkpoints_rows,
                pending.schema,
                pending.file_format,
            )?;
            debug!(
                pipeline = %pipeline,
                seq,
                checkpoint_range = ?pending.checkpoint_range,
                bytes = bytes.len(),
                "Serialized file"
            );
            Ok(SerializedFile {
                seq,
                epoch: pending.epoch,
                checkpoint_range: pending.checkpoint_range,
                file_format: pending.file_format,
                bytes,
            })
        });
        self.serializing.push(handle);
    }

    /// Forward a completed serialization result to the uploader.
    /// Returns Err(()) if the dispatcher should stop.
    async fn forward_to_uploader(
        &mut self,
        join_result: Result<Result<SerializedFile, anyhow::Error>, tokio::task::JoinError>,
    ) -> Result<(), ()> {
        let serialized = match join_result {
            Ok(Ok(file)) => file,
            Ok(Err(err)) => {
                error!(pipeline = %self.pipeline, %err, "Serialization failed, stopping");
                return Err(());
            }
            Err(err) => {
                error!(pipeline = %self.pipeline, %err, "Serialization task panicked, stopping");
                return Err(());
            }
        };
        if self.upload_tx.send(serialized).await.is_err() {
            error!(pipeline = %self.pipeline, "Upload channel closed, stopping");
            return Err(());
        }
        Ok(())
    }
}

/// Worker that receives serialized files, reorders them, and uploads sequentially.
struct SequentialUploader {
    rx: mpsc::Receiver<SerializedFile>,
    /// Files received out of order, waiting to be uploaded
    pending: BTreeMap<u64, SerializedFile>,
    /// Next sequence number to upload
    next_upload_seq: u64,
    pipeline_name: String,
    output_prefix: String,
    mode: StoreMode,
    metrics: Metrics,
    /// Last time we wrote watermark to object store (for rate limiting)
    last_watermark_update: Option<std::time::Instant>,
    /// Minimum interval between watermark writes
    watermark_update_interval: Duration,
    /// Latest uploaded watermark (epoch, checkpoint_hi_inclusive).
    latest_watermark: Option<(EpochId, u64)>,
}

impl SequentialUploader {
    fn new(
        rx: mpsc::Receiver<SerializedFile>,
        pipeline_name: String,
        output_prefix: String,
        mode: StoreMode,
        metrics: Metrics,
        watermark_update_interval: Duration,
    ) -> Self {
        Self {
            rx,
            pending: BTreeMap::new(),
            next_upload_seq: 0,
            pipeline_name,
            output_prefix,
            mode,
            metrics,
            last_watermark_update: None,
            watermark_update_interval,
            latest_watermark: None,
        }
    }

    async fn run(mut self) {
        debug!(pipeline = %self.pipeline_name, "Upload worker starting");

        // Receive files and upload in sequence order
        while let Some(file) = self.rx.recv().await {
            self.pending.insert(file.seq, file);
            self.drain_and_upload().await;
        }

        // Drain any remaining pending files
        self.drain_and_upload().await;

        // Final watermark flush on shutdown
        if let Some((epoch, checkpoint_hi)) = self.latest_watermark {
            self.update_watermark_with_retry(epoch, checkpoint_hi).await;
            info!(
                pipeline = %self.pipeline_name,
                epoch,
                checkpoint_hi,
                "Flushed watermark on shutdown"
            );
        }

        debug!(pipeline = %self.pipeline_name, "Upload worker finished");
    }

    async fn drain_and_upload(&mut self) {
        // Upload files in sequence order
        while let Some(file) = self.pending.remove(&self.next_upload_seq) {
            self.upload_with_retry(&file).await;
            self.next_upload_seq += 1;
        }
    }

    async fn upload_with_retry(&mut self, file: &SerializedFile) {
        let mut backoff = Backoff::new();

        let path = construct_object_store_path(
            &self.output_prefix,
            file.epoch,
            &file.checkpoint_range,
            file.file_format,
        );

        loop {
            match self
                .do_upload(&path, &file.checkpoint_range, file.bytes.clone())
                .await
            {
                Ok(()) => {
                    let checkpoint_hi = file.checkpoint_range.end - 1;

                    record_file_metrics(&self.metrics, &self.pipeline_name, file.bytes.len());
                    self.metrics
                        .latest_uploaded_checkpoint
                        .with_label_values(&[&self.pipeline_name])
                        .set(checkpoint_hi as i64);
                    self.metrics
                        .latest_uploaded_epoch
                        .with_label_values(&[&self.pipeline_name])
                        .set(file.epoch as i64);

                    info!(
                        pipeline = %self.pipeline_name,
                        epoch = file.epoch,
                        checkpoint_range = ?file.checkpoint_range,
                        bytes = file.bytes.len(),
                        "Uploaded file"
                    );

                    // Always track latest watermark in memory (for shutdown flush)
                    self.latest_watermark = Some((file.epoch, checkpoint_hi));

                    // Rate-limit writes to object store
                    let should_update = self
                        .last_watermark_update
                        .map(|last| last.elapsed() >= self.watermark_update_interval)
                        .unwrap_or(true);

                    if should_update {
                        self.update_watermark_with_retry(file.epoch, checkpoint_hi)
                            .await;
                        self.last_watermark_update = Some(std::time::Instant::now());
                    }

                    return;
                }
                Err(e) => {
                    warn!(
                        pipeline = %self.pipeline_name,
                        checkpoint_range = ?file.checkpoint_range,
                        error = %e,
                        backoff_ms = backoff.current_ms(),
                        "Upload failed, retrying"
                    );
                    backoff.sleep_and_advance().await;
                }
            }
        }
    }

    async fn do_upload(
        &self,
        path: &ObjectPath,
        checkpoint_range: &Range<u64>,
        bytes: Bytes,
    ) -> Result<()> {
        self.mode
            .write_to_object_store(
                &self.output_prefix,
                path,
                checkpoint_range,
                PutPayload::from(bytes),
            )
            .await
    }

    /// Update watermark with retry on transient errors, panic on concurrent writer.
    async fn update_watermark_with_retry(&self, epoch: EpochId, checkpoint_hi: u64) {
        let mut backoff = Backoff::new();

        loop {
            match self
                .mode
                .update_watermark_after_upload(&self.output_prefix, epoch, checkpoint_hi)
                .await
            {
                Ok(()) => return,
                Err(WatermarkUpdateError::ConcurrentWriter { path, message }) => {
                    panic!(
                        "Concurrent writer detected on watermark {}: {}. \
                         Only one instance should run at a time per pipeline.",
                        path, message
                    );
                }
                Err(WatermarkUpdateError::Transient(e)) => {
                    warn!(
                        pipeline = %self.pipeline_name,
                        epoch,
                        checkpoint_hi,
                        error = %e,
                        backoff_ms = backoff.current_ms(),
                        "Transient error updating watermark, retrying"
                    );
                    backoff.sleep_and_advance().await;
                }
            }
        }
    }
}

/// Serialize rows grouped by checkpoint to the appropriate file format.
fn serialize_rows(
    checkpoints: &[CheckpointRows],
    schema: &[&str],
    format: FileFormat,
) -> Result<Bytes> {
    match format {
        FileFormat::Csv => {
            let mut writer = CsvWriter::new()?;
            for checkpoint in checkpoints {
                writer.write(checkpoint)?;
            }
            writer
                .flush()
                .map(|opt| opt.unwrap_or_default())
                .map(Bytes::from)
        }
        FileFormat::Parquet => {
            let mut writer = ParquetWriter::new()?;
            for checkpoint in checkpoints {
                writer.write(checkpoint)?;
            }
            writer
                .flush(schema)
                .map(|opt| opt.unwrap_or_default())
                .map(Bytes::from)
        }
    }
}
