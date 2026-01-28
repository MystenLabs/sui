// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Analytics store implementation with TransactionalStore support.
//!
//! This store supports two modes:
//!
//! ## Live Mode
//! Derives watermarks from file names via bucket iteration at startup,
//! rather than storing them separately. File uploads inherently update the watermark
//! since file names encode checkpoint ranges.
//!
//! ## Migration Mode
//! When `migration_id` is set, the store operates in migration mode:
//! - Existing file ranges are loaded at startup and updated in-place.
//! - Watermark is stored in a separate file: `_metadata/watermarks/{pipeline}@migration_{id}.json`
//! - Conditional PUT with etag is used to prevent concurrent modification of data files

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;
use std::time::Instant;

use anyhow::Result;
use anyhow::bail;
use async_trait::async_trait;
use object_store::PutPayload;
use object_store::path::Path as ObjectPath;
use scoped_futures::ScopedBoxFuture;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::store::Connection;
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_framework::store::TransactionalStore;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_indexer_alt_framework_store_traits::PrunerWatermark;
use sui_indexer_alt_framework_store_traits::ReaderWatermark;
use sui_types::base_types::EpochId;
use tokio::sync::mpsc;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::config::FileFormat;
use crate::config::IndexerConfig;
use crate::handlers::CheckpointRows;
use crate::metrics::Metrics;
use crate::schema::RowSchema;

/// Rows accumulated across commits, waiting to be flushed to a file.
#[derive(Clone)]
pub struct Batch {
    pub(crate) checkpoints_rows: Vec<CheckpointRows>,
    row_count: usize,
    /// When the batch was created.
    created_at: Instant,
    /// Explicit checkpoint range (migration mode only). If set, used for file naming.
    /// If None, checkpoint_range() computes it from the data.
    pub(crate) explicit_range: Option<Range<u64>>,
}

impl Default for Batch {
    fn default() -> Self {
        Self {
            checkpoints_rows: Vec::new(),
            row_count: 0,
            created_at: Instant::now(),
            explicit_range: None,
        }
    }
}

impl Batch {
    pub(crate) fn first_checkpoint(&self) -> Option<u64> {
        self.checkpoints_rows.first().map(|c| c.checkpoint)
    }

    pub(crate) fn last_checkpoint(&self) -> Option<u64> {
        self.checkpoints_rows.last().map(|c| c.checkpoint)
    }

    pub(crate) fn epoch(&self) -> Option<EpochId> {
        self.checkpoints_rows.last().map(|c| c.epoch)
    }

    pub(crate) fn row_count(&self) -> usize {
        self.row_count
    }

    pub(crate) fn checkpoint_count(&self) -> usize {
        self.checkpoints_rows.len()
    }

    /// Returns the checkpoint range for this batch.
    /// Uses explicit_range if set (migration mode), otherwise computes from data.
    pub(crate) fn checkpoint_range(&self) -> Option<Range<u64>> {
        self.explicit_range.clone().or_else(|| {
            match (self.first_checkpoint(), self.last_checkpoint()) {
                (Some(first), Some(last)) => Some(first..last + 1),
                _ => None,
            }
        })
    }

    pub(crate) fn add(&mut self, checkpoint_rows: CheckpointRows) {
        self.row_count += checkpoint_rows.len();
        self.checkpoints_rows.push(checkpoint_rows);
    }

    /// Time elapsed since the batch was created.
    pub(crate) fn elapsed(&self) -> Duration {
        self.created_at.elapsed()
    }
}

mod live;
mod migration;
mod uploader;

pub use live::LiveStore;
pub use migration::FileRangeEntry;
pub use migration::FileRangeIndex;
pub use migration::MigrationStore;
pub use migration::WatermarkUpdateError;

use uploader::PendingFileUpload;

/// The operational mode of the analytics store.
#[derive(Clone)]
pub enum StoreMode {
    Live(LiveStore),
    Migration(MigrationStore),
}

use crate::config::PipelineConfig;

/// Analytics store wrapper that delegates to an inner store mode.
#[derive(Clone)]
pub struct AnalyticsStore {
    mode: StoreMode,
    /// Accumulated rows per pipeline, keyed by pipeline name.
    pending_by_pipeline: Arc<RwLock<HashMap<String, Batch>>>,
    /// Shared metrics for all pipelines.
    metrics: Metrics,
    /// Per-pipeline upload senders. Sender is Clone so we can share it.
    uploader_senders: Arc<RwLock<HashMap<String, mpsc::Sender<PendingFileUpload>>>>,
    /// Worker handles for graceful shutdown.
    worker_handles: Arc<tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    /// Indexer configuration.
    config: IndexerConfig,
    /// Schema for each pipeline, registered during pipeline setup.
    schemas_by_pipeline: Arc<RwLock<HashMap<String, &'static [&'static str]>>>,
}

/// Connection to the analytics store.
///
/// Provides access to the underlying object store for file uploads.
pub struct AnalyticsConnection<'a> {
    store: &'a AnalyticsStore,
    /// Watermark set by the framework before commit.
    /// Used to detect file boundary crossings and end-of-processing.
    watermark: Option<CommitterWatermark>,
}

impl StoreMode {
    /// Split a batch of checkpoints into files.
    ///
    /// Delegates to mode-specific splitting logic:
    /// - Live: cuts at epoch boundaries and batch size thresholds
    /// - Migration: cuts at existing file boundaries using watermark
    pub(crate) fn split_framework_batch_into_files(
        &self,
        pipeline_config: &PipelineConfig,
        batch_from_framework: &[CheckpointRows],
        pending_batch: Batch,
        watermark: &CommitterWatermark,
    ) -> (Batch, Vec<Batch>) {
        match self {
            StoreMode::Live(store) => store.split_framework_batch_into_files(
                pipeline_config,
                batch_from_framework,
                pending_batch,
            ),
            StoreMode::Migration(store) => store.split_framework_batch_into_files(
                pipeline_config,
                batch_from_framework,
                pending_batch,
                watermark,
            ),
        }
    }

    /// Write a file to the object store.
    ///
    /// Delegates to mode-specific logic:
    /// - Live mode: simple `put`
    /// - Migration mode: verifies range matches expected, uses conditional PUT with etag/version
    pub(crate) async fn write_to_object_store(
        &self,
        pipeline: &str,
        path: &ObjectPath,
        checkpoint_range: &Range<u64>,
        payload: PutPayload,
    ) -> anyhow::Result<()> {
        match self {
            StoreMode::Live(store) => store.write_to_object_store(path, payload).await,
            StoreMode::Migration(store) => {
                store
                    .write_to_object_store(pipeline, path, checkpoint_range, payload)
                    .await
            }
        }
    }

    /// Update watermark after a successful file upload.
    ///
    /// In migration mode, writes the watermark to the metadata file.
    /// In live mode, this is a no-op (watermarks are derived from files).
    pub(crate) async fn update_watermark_after_upload(
        &self,
        pipeline: &str,
        epoch: u64,
        checkpoint_hi_inclusive: u64,
    ) -> Result<(), WatermarkUpdateError> {
        match self {
            StoreMode::Live(_) => Ok(()),
            StoreMode::Migration(store) => {
                store
                    .update_watermark(pipeline, epoch, checkpoint_hi_inclusive)
                    .await
            }
        }
    }

    /// Spawn an upload worker for this mode.
    ///
    /// Returns the sender for queueing files and the worker's JoinHandle.
    pub(crate) fn spawn_uploader(
        &self,
        pipeline_name: String,
        output_prefix: String,
        metrics: Metrics,
        config: &IndexerConfig,
    ) -> (mpsc::Sender<PendingFileUpload>, tokio::task::JoinHandle<()>) {
        uploader::spawn_uploader(pipeline_name, output_prefix, self.clone(), metrics, config)
    }
}

impl AnalyticsStore {
    /// Create a new analytics store.
    ///
    /// The mode (live vs migration) is determined by `config.migration_id`:
    /// - None: Live mode for streaming ingestion, sequential uploads
    /// - Some(id): Migration mode for rewriting existing files, concurrent uploads
    pub fn new(
        object_store: Arc<dyn object_store::ObjectStore>,
        config: IndexerConfig,
        metrics: Metrics,
    ) -> Self {
        let mode = if let Some(ref migration_id) = config.migration_id {
            info!(migration_id, "Enabling migration mode");
            StoreMode::Migration(MigrationStore::new(object_store, migration_id.clone()))
        } else {
            StoreMode::Live(LiveStore::new(object_store))
        };
        Self {
            mode,
            pending_by_pipeline: Arc::new(RwLock::new(HashMap::new())),
            metrics,
            uploader_senders: Arc::new(RwLock::new(HashMap::new())),
            worker_handles: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            config,
            schemas_by_pipeline: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Find the checkpoint range for ingestion, snapping to file boundaries in migration mode.
    ///
    /// In migration mode, loads file ranges and snaps both `first_checkpoint` and
    /// `last_checkpoint` to file boundaries:
    /// - `first_checkpoint` snaps to the start of the containing file
    /// - `last_checkpoint` snaps to the end of the containing file (exclusive)
    ///
    /// Returns (adjusted_first, adjusted_last) across all pipelines.
    ///
    /// In live mode, returns the checkpoints unchanged.
    pub async fn find_checkpoint_range(
        &self,
        first_checkpoint: Option<u64>,
        last_checkpoint: Option<u64>,
    ) -> Result<(Option<u64>, Option<u64>)> {
        match &self.mode {
            StoreMode::Live(_) => Ok((first_checkpoint, last_checkpoint)),
            StoreMode::Migration(store) => {
                // Pass (pipeline_name, output_prefix) pairs.
                // pipeline_name is used as the key in the file_ranges map.
                // output_prefix is the path in the object store where files are located.
                let pipelines: Vec<_> = self
                    .config
                    .pipeline_configs()
                    .iter()
                    .map(|p| (p.pipeline.name(), p.output_prefix()))
                    .collect();
                store
                    .find_checkpoint_range(
                        pipelines.iter().map(|(name, prefix)| (*name, *prefix)),
                        first_checkpoint,
                        last_checkpoint,
                    )
                    .await
            }
        }
    }

    /// Register the schema for a pipeline. Called during pipeline setup.
    pub fn register_schema<P: Processor, T: RowSchema>(&self) {
        self.schemas_by_pipeline
            .write()
            .unwrap()
            .insert(P::NAME.to_string(), T::schema());
    }

    /// Get the schema for a pipeline.
    fn get_schema(&self, pipeline: &str) -> Option<&'static [&'static str]> {
        self.schemas_by_pipeline
            .read()
            .unwrap()
            .get(pipeline)
            .copied()
    }

    /// Get or create an uploader for a pipeline.
    ///
    /// Lazily spawns a background worker on first access.
    fn get_or_create_uploader(&self, pipeline: &str) -> mpsc::Sender<PendingFileUpload> {
        // Check if uploader already exists
        {
            let uploaders = self.uploader_senders.read().unwrap();
            if let Some(tx) = uploaders.get(pipeline) {
                return tx.clone();
            }
        }

        // Create new uploader
        let mut uploaders = self.uploader_senders.write().unwrap();
        // Double-check in case another thread created it
        if let Some(tx) = uploaders.get(pipeline) {
            return tx.clone();
        }

        let output_prefix = self
            .config
            .get_pipeline_config(pipeline)
            .expect("Pipeline not configured")
            .output_prefix();

        let (tx, handle) = self.mode.spawn_uploader(
            pipeline.to_string(),
            output_prefix.to_string(),
            self.metrics.clone(),
            &self.config,
        );
        uploaders.insert(pipeline.to_string(), tx.clone());

        // Track the handle for shutdown
        // Note: We can't block here, so we spawn a task to add the handle
        let handles = self.worker_handles.clone();
        tokio::spawn(async move {
            handles.lock().await.push(handle);
        });

        tx
    }

    /// Flush all pending batches before shutdown.
    ///
    /// This ensures any buffered data that hasn't reached batch thresholds
    /// is written to the object store before the indexer shuts down.
    async fn flush_pending_batches(&self) {
        // Take all pending batches
        let pending: HashMap<String, Batch> = {
            let mut pending_map = self.pending_by_pipeline.write().unwrap();
            std::mem::take(&mut *pending_map)
        };

        match &self.mode {
            StoreMode::Live(_) => {
                for (pipeline_name, batch) in pending {
                    if batch.checkpoint_count() == 0 {
                        continue;
                    }

                    let pipeline_config = self
                        .config
                        .get_pipeline_config(&pipeline_name)
                        .expect("Pipeline config must exist for pending batch");

                    let schema = self
                        .get_schema(&pipeline_name)
                        .expect("Schema must be registered for pending batch");

                    info!(
                        pipeline = %pipeline_name,
                        checkpoints = batch.checkpoint_count(),
                        rows = batch.row_count(),
                        "Flushing pending batch on shutdown"
                    );

                    let pending_upload = PendingFileUpload {
                        epoch: batch.epoch().unwrap(),
                        checkpoint_range: batch.checkpoint_range().unwrap(),
                        file_format: pipeline_config.file_format,
                        checkpoints_rows: batch.checkpoints_rows,
                        schema,
                    };

                    let tx = self.get_or_create_uploader(&pipeline_name);
                    if tx.send(pending_upload).await.is_err() {
                        warn!(pipeline = %pipeline_name, "Failed to send final batch to uploader");
                    }
                }
            }
            StoreMode::Migration(_) => {
                // Migration mode only modifies existing files at known boundaries.
                // Any pending data that doesn't align with file boundaries is intentionally dropped.
            }
        }
    }

    /// Shutdown all upload workers, waiting for pending uploads to complete.
    pub async fn shutdown(&self) {
        // Flush any pending batches before closing channels (live mode only)
        self.flush_pending_batches().await;

        // Clear senders to signal workers to stop
        self.uploader_senders.write().unwrap().clear();

        // Wait for all workers to finish
        let mut handles = self.worker_handles.lock().await;
        for handle in handles.drain(..) {
            let _ = handle.await;
        }
    }
}

impl<'a> AnalyticsConnection<'a> {
    /// Get the store mode for split_batch operations.
    pub fn mode(&self) -> &StoreMode {
        &self.store.mode
    }

    /// Get a clone of the pending rows for a pipeline.
    /// Returns default FileRows if pipeline has no pending rows.
    pub fn get_pending_batch(&self, pipeline: &str) -> Batch {
        self.store
            .pending_by_pipeline
            .read()
            .unwrap()
            .get(pipeline)
            .cloned()
            .unwrap_or_default()
    }

    /// Set the pending rows for a pipeline after successful upload.
    pub fn set_pending_batch(&self, pipeline: &str, rows: Batch) {
        self.store
            .pending_by_pipeline
            .write()
            .unwrap()
            .insert(pipeline.to_string(), rows);
    }

    /// Get the pipeline config for a pipeline.
    fn pipeline_config(&self, pipeline: &str) -> &PipelineConfig {
        self.store
            .config
            .get_pipeline_config(pipeline)
            .unwrap_or_else(|| panic!("Pipeline '{}' not configured", pipeline))
    }

    /// Write a file to the object store.
    ///
    /// Constructs the path from the provided parameters and delegates to the store mode.
    pub async fn write_to_object_store(
        &self,
        pipeline: &str,
        epoch: EpochId,
        checkpoint_range: Range<u64>,
        file_format: FileFormat,
        payload: PutPayload,
    ) -> anyhow::Result<()> {
        let path = construct_object_store_path(pipeline, epoch, &checkpoint_range, file_format);
        self.store
            .mode
            .write_to_object_store(pipeline, &path, &checkpoint_range, payload)
            .await
    }

    /// Commit a batch of rows to the object store.
    ///
    /// # Background
    ///
    /// The indexer framework has limitations that require us to handle batching
    /// and serialization in the store layer:
    ///
    /// 1. **No minimum batch size**: The framework supports max batch size but not
    ///    min batch size, so there's no way to defer commits until a batch reaches
    ///    a certain size or to control which checkpoints end up in which output files.
    ///
    /// 2. **No fan-out/fan-in for batch processing**: The framework provides no way
    ///    to fan out processing of a completed batch (e.g., CPU-intensive serialization)
    ///    before committing it sequentially.
    ///
    /// To work around these limitations, this store accumulates rows across
    /// checkpoint commits and manages its own batching logic (by checkpoint count
    /// or row count). Serialization is offloaded to background workers via
    /// `spawn_blocking`, allowing multiple batches to serialize in parallel while
    /// maintaining strict checkpoint ordering for uploads.
    ///
    /// # Commit Lifecycle
    ///
    /// 1. Accumulates rows in pending buffer
    /// 2. When batch threshold is reached, sends to background upload worker
    /// 3. Worker serializes (parallel) and uploads (sequential by checkpoint order)
    ///
    /// Backpressure: If the upload channel is full, this method blocks.
    ///
    /// # Error Handling
    ///
    /// The framework assumes commit_batch is atomic - if an error is returned,
    /// the transaction is rolled back and retried. This method _never_ returns
    /// an error; object store write failures are retried internally. The
    /// implementation is idempotent, so framework retries would be safe anyway.
    pub async fn commit_batch<P: Processor>(
        &mut self,
        batch_from_framework: &[CheckpointRows],
    ) -> Result<usize> {
        let pipeline = P::NAME;
        let pipeline_config = self.pipeline_config(pipeline);

        // Split batch from framework into batches that we can upload as files.
        // The watermark is passed to detect file boundary completion in migration mode.
        let (pending_batch, complete_batches) = {
            let pending_batch = self.get_pending_batch(pipeline);
            self.store.mode.split_framework_batch_into_files(
                pipeline_config,
                batch_from_framework,
                pending_batch,
                self.watermark
                    .as_ref()
                    .expect("watermark should be set on connection."),
            )
        };

        debug!(
            pipeline = pipeline,
            files_to_upload = complete_batches.len(),
            pending_checkpoints = pending_batch.checkpoint_count(),
            "Commit starting"
        );

        // Get the uploader for this pipeline (lazily created)
        let tx = self.store.get_or_create_uploader(pipeline);

        let mut total_rows = 0;
        for batch in complete_batches {
            total_rows += batch.row_count();

            let pending_upload = PendingFileUpload {
                epoch: batch.epoch().unwrap(),
                checkpoint_range: batch.checkpoint_range().unwrap(),
                file_format: pipeline_config.file_format,
                checkpoints_rows: batch.checkpoints_rows,
                schema: self
                    .store
                    .get_schema(pipeline)
                    .unwrap_or_else(|| panic!("Schema not registered for pipeline: {}", pipeline)),
            };

            // Send to worker - BLOCKS IF CHANNEL FULL (backpressure)
            tx.send(pending_upload)
                .await
                .unwrap_or_else(|e| panic!("Upload channel closed: {}", e));
        }

        self.set_pending_batch(pipeline, pending_batch);

        debug!(
            pipeline = pipeline,
            total_rows = total_rows,
            "Commit complete, files queued for upload"
        );

        Ok(total_rows)
    }
}

#[async_trait]
impl Store for AnalyticsStore {
    type Connection<'c> = AnalyticsConnection<'c>;

    async fn connect<'c>(&'c self) -> anyhow::Result<Self::Connection<'c>> {
        Ok(AnalyticsConnection {
            store: self,
            watermark: None,
        })
    }
}

#[async_trait]
impl TransactionalStore for AnalyticsStore {
    async fn transaction<'a, R, F>(&self, f: F) -> anyhow::Result<R>
    where
        R: Send + 'a,
        F: Send + 'a,
        F: for<'r> FnOnce(
            &'r mut Self::Connection<'_>,
        ) -> ScopedBoxFuture<'a, 'r, anyhow::Result<R>>,
    {
        let mut conn = self.connect().await?;
        f(&mut conn).await
    }
}

#[async_trait]
impl Connection for AnalyticsConnection<'_> {
    /// Initialize watermark.
    ///
    /// In live mode: Watermarks are derived from file names, so just delegates to `committer_watermark`.
    /// In migration mode: If no watermark exists and `default_next_checkpoint > 0`, initializes
    /// the watermark to `default_next_checkpoint - 1` so migration starts from the configured
    /// `first_checkpoint`.
    async fn init_watermark(
        &mut self,
        pipeline_task: &str,
        default_next_checkpoint: u64,
    ) -> anyhow::Result<Option<u64>> {
        match &self.store.mode {
            StoreMode::Live(_) => {
                // Live mode: derive from file names
                Ok(self
                    .committer_watermark(pipeline_task)
                    .await?
                    .map(|w| w.checkpoint_hi_inclusive))
            }
            StoreMode::Migration(store) => {
                let output_prefix = self.pipeline_config(pipeline_task).output_prefix();
                store
                    .init_watermark(output_prefix, default_next_checkpoint)
                    .await
            }
        }
    }

    /// Determine the watermark.
    ///
    /// In live mode: scans file names in the object store.
    /// In migration mode: reads from watermark metadata file.
    async fn committer_watermark(
        &mut self,
        pipeline: &str,
    ) -> anyhow::Result<Option<CommitterWatermark>> {
        let output_prefix = self.pipeline_config(pipeline).output_prefix().to_string();
        match &self.store.mode {
            StoreMode::Live(store) => store.committer_watermark(&output_prefix).await,
            StoreMode::Migration(store) => store.committer_watermark(&output_prefix).await,
        }
    }

    async fn reader_watermark(
        &mut self,
        _pipeline: &'static str,
    ) -> anyhow::Result<Option<ReaderWatermark>> {
        // Reader watermark not supported - no pruning in analytics indexer
        Ok(None)
    }

    async fn pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        _delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark>> {
        // Pruning not supported in analytics indexer
        Ok(None)
    }

    /// Store the watermark for use in commit_batch.
    ///
    /// Note: This doesn't persist the watermark - that's done by the upload worker
    /// after successful file uploads. This just captures the watermark so commit_batch
    /// can use it to detect file boundary crossings and end-of-processing.
    async fn set_committer_watermark(
        &mut self,
        _pipeline_task: &str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool> {
        self.watermark = Some(watermark);
        Ok(true)
    }

    async fn set_reader_watermark(
        &mut self,
        _pipeline: &'static str,
        _reader_lo: u64,
    ) -> anyhow::Result<bool> {
        bail!("Pruning not supported by analytics store");
    }

    async fn set_pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        _pruner_hi: u64,
    ) -> anyhow::Result<bool> {
        bail!("Pruning not supported by analytics store");
    }
}

/// Construct the object store path for an analytics file.
/// Path format: {pipeline}/epoch_{epoch}/{start}_{end}.{ext}
pub(crate) fn construct_object_store_path(
    pipeline: &str,
    epoch: EpochId,
    checkpoint_range: &Range<u64>,
    file_format: FileFormat,
) -> ObjectPath {
    let extension = match file_format {
        FileFormat::Csv => "csv",
        FileFormat::Parquet => "parquet",
    };
    ObjectPath::from(format!(
        "{}/epoch_{}/{}_{}.{}",
        pipeline, epoch, checkpoint_range.start, checkpoint_range.end, extension
    ))
}

/// Parse checkpoint range from filename.
/// Expected format: `{start}_{end}.{format}` (e.g., `0_100.parquet`)
pub(crate) fn parse_checkpoint_range(filename: &str) -> Option<Range<u64>> {
    let base = filename.split('.').next()?;
    let (start_str, end_str) = base.split_once('_')?;
    let start: u64 = start_str.parse().ok()?;
    let end: u64 = end_str.parse().ok()?;
    Some(start..end)
}
