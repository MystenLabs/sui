// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Migration mode store - uses explicit watermark files and conditional PUT.
//!
//! In migration mode, we rewrite existing files (e.g., adding new columns to parquet files).
//! This module provides the store implementation and utilities to track existing file ranges.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::RwLock;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use object_store::Error as ObjectStoreError;
use object_store::ObjectStore;
use object_store::PutMode;
use object_store::PutOptions;
use object_store::PutPayload;
use object_store::UpdateVersion;
use object_store::path::Path as ObjectPath;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_storage::object_store::util::find_all_dirs_with_epoch_prefix;
use thiserror::Error;
use tracing::debug;
use tracing::info;

use crate::config::PipelineConfig;
use crate::handlers::CheckpointRows;
use crate::store::Batch;

/// Error type for watermark updates.
#[derive(Error, Debug)]
pub enum WatermarkUpdateError {
    /// Precondition failure - concurrent writer detected. This is fatal.
    #[error("Concurrent writer detected on watermark {path}: {message}")]
    ConcurrentWriter { path: String, message: String },

    /// Transient error - can be retried.
    #[error("Transient error updating watermark: {0}")]
    Transient(#[from] anyhow::Error),
}

/// Version info (etag, version) for conditional PUT operations.
type VersionInfo = (Option<String>, Option<String>);

/// Simple watermark struct for JSON serialization.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct MigrationWatermark {
    pub checkpoint_hi_inclusive: u64,
    /// Epoch of the watermark - used to skip scanning earlier epochs on restart.
    pub epoch_hi_inclusive: u64,
}

/// Migration mode - uses explicit watermark files and conditional PUT.
///
/// Used for rewriting existing files (e.g., adding new columns to parquet files).
/// Tracks progress separately via watermark files and uses conditional PUT to
/// ensure we are overwriting the files we expect.
#[derive(Clone)]
pub struct MigrationStore {
    object_store: Arc<dyn ObjectStore>,
    /// Migration identifier.
    migration_id: String,
    /// Pipeline -> FileRangeIndex (target ranges).
    file_ranges: Arc<RwLock<HashMap<String, FileRangeIndex>>>,
    /// Pipeline -> (etag, version) for conditional PUT on watermark files.
    watermark_versions: Arc<RwLock<HashMap<String, VersionInfo>>>,
    /// Pre-computed per-pipeline adjusted starting checkpoints.
    /// Set during pre-loading in build_analytics_indexer.
    adjusted_start_checkpoints: Arc<RwLock<HashMap<String, u64>>>,
}

/// Entry for a single file range in the index.
#[derive(Debug, Clone)]
pub struct FileRangeEntry {
    /// Start checkpoint (inclusive).
    pub start: u64,
    /// End checkpoint (exclusive).
    pub end: u64,
    /// Epoch this file belongs to.
    pub epoch: u64,
}

/// Index of existing file ranges for a pipeline.
///
/// In migration mode, this is loaded at startup to track target file ranges.
/// Progress is tracked separately via a watermark file.
#[derive(Debug, Default, Clone)]
pub struct FileRangeIndex {
    /// Map from start_checkpoint -> FileRangeEntry.
    /// Sorted by start checkpoint for efficient lookups.
    ranges: BTreeMap<u64, FileRangeEntry>,
}

impl MigrationStore {
    /// Create a new migration store.
    pub fn new(object_store: Arc<dyn ObjectStore>, migration_id: String) -> Self {
        Self {
            object_store,
            migration_id,
            file_ranges: Arc::new(RwLock::new(HashMap::new())),
            watermark_versions: Arc::new(RwLock::new(HashMap::new())),
            adjusted_start_checkpoints: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load file ranges and find the starting/ending checkpoints for migration.
    ///
    /// This snaps `first_checkpoint` to file boundaries:
    /// - If checkpoint is inside a file → snap to file start
    /// - If checkpoint is in a gap → snap to next file start
    /// - If no files at or after checkpoint → error
    ///
    /// This also snaps `last_checkpoint` to file boundaries:
    /// - If checkpoint is inside a file → snap to file end (exclusive)
    /// - If checkpoint is in a gap → snap to previous file end (exclusive)
    /// - If no files at or before checkpoint → error
    ///
    /// Returns (adjusted_first, adjusted_last) where:
    /// - adjusted_first is the minimum adjusted start across all pipelines
    /// - adjusted_last is the maximum adjusted end across all pipelines
    ///
    /// Each pipeline is specified as (pipeline_name, output_prefix) where:
    /// - pipeline_name is used as the key in file_ranges map (for lookup by handlers)
    /// - output_prefix is the path prefix in the object store where files are located
    pub async fn find_checkpoint_range(
        &self,
        pipelines: impl Iterator<Item = (&str, &str)>,
        first_checkpoint: Option<u64>,
        last_checkpoint: Option<u64>,
    ) -> Result<(Option<u64>, Option<u64>)> {
        let mut file_ranges = HashMap::new();
        let mut adjusted_starts = HashMap::new();
        let mut min_adjusted_start: Option<u64> = None;
        let mut max_adjusted_end: Option<u64> = None;

        for (pipeline_name, output_prefix) in pipelines {
            // Load file ranges from the output_prefix path in the object store
            let index =
                FileRangeIndex::load_from_store(&self.object_store, output_prefix, None).await?;

            // Snap starting checkpoint to file boundary.
            // If first_checkpoint is not specified, snap from 0 to find the first available file.
            let first_cp = first_checkpoint.unwrap_or(0);
            let adjusted = index.snap_to_boundary(first_cp).ok_or_else(|| {
                anyhow!(
                    "No files at or after checkpoint {} for pipeline '{}'. Nothing to migrate.",
                    first_cp,
                    pipeline_name
                )
            })?;
            // Key by pipeline_name for lookup by handlers
            adjusted_starts.insert(pipeline_name.to_string(), adjusted);
            min_adjusted_start = Some(min_adjusted_start.map_or(adjusted, |m| m.min(adjusted)));

            info!(
                pipeline = pipeline_name,
                output_prefix,
                requested_checkpoint = first_cp,
                adjusted_checkpoint = adjusted,
                "Snapped first_checkpoint to file boundary"
            );

            // Compute adjusted ending checkpoint if last_checkpoint specified
            if let Some(last_cp) = last_checkpoint {
                let adjusted = index.snap_end_to_boundary(last_cp).ok_or_else(|| {
                    anyhow!(
                        "No files at or before checkpoint {} for pipeline '{}'. Nothing to migrate.",
                        last_cp,
                        pipeline_name
                    )
                })?;
                // Use max across pipelines so all pipelines get all their data
                max_adjusted_end = Some(max_adjusted_end.map_or(adjusted, |m| m.max(adjusted)));

                info!(
                    pipeline = pipeline_name,
                    requested_checkpoint = last_cp,
                    adjusted_checkpoint = adjusted,
                    "Snapped last_checkpoint to file boundary"
                );
            }

            // Key by pipeline_name for lookup by handlers
            file_ranges.insert(pipeline_name.to_string(), index);
        }

        // Store the loaded data
        debug!(
            pipeline_keys = ?file_ranges.keys().collect::<Vec<_>>(),
            "Loaded file ranges for migration"
        );
        *self.file_ranges.write().unwrap() = file_ranges;
        *self.adjusted_start_checkpoints.write().unwrap() = adjusted_starts;

        // Return adjusted checkpoints.
        // min_adjusted_start is always set (we snap from 0 if first_checkpoint not specified).
        // last_checkpoint is inclusive in framework (uses ..=), and snap_end_to_boundary
        // returns end - 1 which is also inclusive, so they match.
        Ok((min_adjusted_start, max_adjusted_end.or(last_checkpoint)))
    }

    pub fn migration_id(&self) -> &str {
        &self.migration_id
    }

    pub fn file_ranges(&self) -> &Arc<RwLock<HashMap<String, FileRangeIndex>>> {
        &self.file_ranges
    }

    /// Read watermark from metadata file and cache its etag/version.
    pub(crate) async fn committer_watermark(
        &self,
        pipeline: &str,
    ) -> anyhow::Result<Option<CommitterWatermark>> {
        let path = migration_watermark_path(pipeline, &self.migration_id);
        match self.object_store.get(&path).await {
            Ok(result) => {
                // Capture etag and version for conditional PUT
                let e_tag = result.meta.e_tag.clone();
                let version = result.meta.version.clone();
                self.watermark_versions
                    .write()
                    .unwrap()
                    .insert(pipeline.to_string(), (e_tag, version));

                let bytes = result.bytes().await?;
                let watermark: MigrationWatermark = serde_json::from_slice(&bytes)
                    .context("Failed to parse migration watermark from object store")?;
                info!(
                    pipeline,
                    migration_id = self.migration_id,
                    epoch = watermark.epoch_hi_inclusive,
                    checkpoint = watermark.checkpoint_hi_inclusive,
                    "Migration mode: found progress from watermark file"
                );
                Ok(Some(CommitterWatermark {
                    epoch_hi_inclusive: watermark.epoch_hi_inclusive,
                    checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
                    tx_hi: 0,
                    timestamp_ms_hi_inclusive: 0,
                }))
            }
            Err(ObjectStoreError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Initialize migration mode for a pipeline.
    ///
    /// Reads existing watermark file if present, and ensures file ranges are loaded.
    ///
    /// Returns the last processed checkpoint if a watermark exists, or None if starting fresh.
    /// The watermark file is created/updated by `update_watermark` after file uploads.
    pub(crate) async fn init_watermark(
        &self,
        pipeline: &str,
        _default_next_checkpoint: u64,
    ) -> anyhow::Result<Option<u64>> {
        // Check existing watermark
        let (checkpoint_hi, epoch_hi) =
            if let Some(watermark) = self.committer_watermark(pipeline).await? {
                (
                    Some(watermark.checkpoint_hi_inclusive),
                    Some(watermark.epoch_hi_inclusive),
                )
            } else {
                // No existing watermark - framework will use default_next_checkpoint
                // Watermark will be created by update_watermark after first file upload
                (None, None)
            };

        // Load file ranges if not already pre-loaded
        if !self.file_ranges.read().unwrap().contains_key(pipeline) {
            let index =
                FileRangeIndex::load_from_store(&self.object_store, pipeline, epoch_hi).await?;
            self.file_ranges
                .write()
                .unwrap()
                .insert(pipeline.to_string(), index);
        }

        Ok(checkpoint_hi)
    }

    /// Update watermark for a single pipeline after successful file upload.
    ///
    /// Called by the upload worker after each file is successfully uploaded.
    /// This provides incremental progress tracking for crash recovery.
    ///
    /// Returns `WatermarkUpdateError::ConcurrentWriter` on precondition failure (fatal),
    /// or `WatermarkUpdateError::Transient` on other errors (can be retried).
    pub(crate) async fn update_watermark(
        &self,
        pipeline: &str,
        epoch_hi_inclusive: u64,
        checkpoint_hi_inclusive: u64,
    ) -> std::result::Result<(), WatermarkUpdateError> {
        let path = migration_watermark_path(pipeline, &self.migration_id);
        let json = serde_json::to_vec(&MigrationWatermark {
            checkpoint_hi_inclusive,
            epoch_hi_inclusive,
        })
        .map_err(|e| WatermarkUpdateError::Transient(e.into()))?;

        // Look up cached etag/version for conditional PUT
        let (e_tag, version) = self
            .watermark_versions
            .read()
            .unwrap()
            .get(pipeline)
            .cloned()
            .unwrap_or((None, None));

        let mode = if e_tag.is_some() || version.is_some() {
            PutMode::Update(UpdateVersion { e_tag, version })
        } else {
            PutMode::Create
        };

        let result = self
            .object_store
            .put_opts(
                &path,
                json.into(),
                PutOptions {
                    mode,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| match e {
                ObjectStoreError::Precondition { path, source } => {
                    WatermarkUpdateError::ConcurrentWriter {
                        path: path.to_string(),
                        message: source.to_string(),
                    }
                }
                other => WatermarkUpdateError::Transient(other.into()),
            })?;

        // Update cached etag/version
        self.watermark_versions
            .write()
            .unwrap()
            .insert(pipeline.to_string(), (result.e_tag, result.version));

        tracing::debug!(
            pipeline,
            migration_id = self.migration_id,
            checkpoint = checkpoint_hi_inclusive,
            epoch = epoch_hi_inclusive,
            "Updated migration watermark"
        );

        Ok(())
    }

    /// Split a batch of checkpoints into files based on existing file boundaries.
    ///
    /// In migration mode, we match the boundaries of existing files to ensure
    /// we can use conditional PUT with the correct e_tag/version.
    ///
    /// The watermark indicates the highest checkpoint processed by the framework,
    /// including empty checkpoints. It's used to detect when the final file is
    /// complete (when there's no "next row" to trigger boundary detection).
    pub(crate) fn split_framework_batch_into_files(
        &self,
        pipeline_config: &PipelineConfig,
        batch_from_framework: &[CheckpointRows],
        mut pending_batch: Batch,
        watermark: &CommitterWatermark,
    ) -> (Batch, Vec<Batch>) {
        let mut complete_batches: Vec<Batch> = Vec::new();
        let pipeline = pipeline_config.pipeline.name();

        let ranges = self
            .file_ranges
            .read()
            .unwrap()
            .get(pipeline)
            .cloned()
            .expect("migration ranges not loaded for pipeline");

        for checkpoint_rows in batch_from_framework {
            let cp = checkpoint_rows.checkpoint;

            // Check if we've crossed a file boundary BEFORE adding this checkpoint.
            // This handles sparse checkpoints - the framework only sends checkpoints with rows,
            // so we might skip from e.g. 6500 to 6600, missing the exact boundary at 6543.
            if let Some(first) = pending_batch.first_checkpoint()
                && let Some(entry) = ranges.find_containing(first)
                && cp >= entry.end
            {
                debug!(
                    pipeline,
                    current_cp = cp,
                    file_start = entry.start,
                    file_end = entry.end,
                    "Crossed file boundary - completing batch"
                );
                pending_batch.explicit_range = Some(entry.start..entry.end);
                complete_batches.push(pending_batch);
                pending_batch = Batch::default();
            }

            pending_batch.add(checkpoint_rows.clone());

            // Validate that checkpoints with rows are in a known file range
            if let Some(first) = pending_batch.first_checkpoint()
                && ranges.find_containing(first).is_none()
                && !checkpoint_rows.is_empty()
            {
                panic!(
                    "Migration error: checkpoint {} has {} rows but is not in any existing file range for pipeline '{}'. \
                     File ranges cover checkpoints {:?} to {:?}.",
                    first,
                    checkpoint_rows.len(),
                    pipeline,
                    ranges.first_checkpoint(),
                    ranges.last_checkpoint_exclusive(),
                );
            }
        }

        // Check if the pending batch's file is complete based on watermark.
        // This handles the final batch when there's no "next row" to trigger
        // the boundary detection above.
        if let Some(first) = pending_batch.first_checkpoint()
            && let Some(entry) = ranges.find_containing(first)
            && watermark.checkpoint_hi_inclusive >= entry.end - 1
        {
            debug!(
                pipeline,
                watermark_cp = watermark.checkpoint_hi_inclusive,
                file_start = entry.start,
                file_end = entry.end,
                "File complete per watermark - completing batch"
            );
            pending_batch.explicit_range = Some(entry.start..entry.end);
            complete_batches.push(pending_batch);
            pending_batch = Batch::default();
        }

        (pending_batch, complete_batches)
    }

    /// Write a file to the object store with conditional update.
    ///
    /// Verifies the checkpoint range matches an existing file in the index,
    /// then fetches current metadata via HEAD for conditional PUT.
    ///
    /// Errors if the file doesn't exist (migration mode requires existing files to replace).
    pub(crate) async fn write_to_object_store(
        &self,
        pipeline: &str,
        path: &ObjectPath,
        checkpoint_range: &Range<u64>,
        payload: PutPayload,
    ) -> anyhow::Result<()> {
        // Verify this range exists in our index
        {
            let ranges = self.file_ranges.read().unwrap();
            let pipeline_ranges = ranges.get(pipeline).expect("migration ranges not loaded");

            let entry = pipeline_ranges
                .find_containing(checkpoint_range.start)
                .ok_or_else(|| {
                    anyhow!(
                        "No file in index for checkpoint range {:?} - migration requires existing files",
                        checkpoint_range
                    )
                })?;

            // Verify the range matches exactly
            anyhow::ensure!(
                entry.start == checkpoint_range.start,
                "checkpoint range start mismatch: expected {}, got {}",
                entry.start,
                checkpoint_range.start
            );
            anyhow::ensure!(
                entry.end == checkpoint_range.end,
                "checkpoint range end mismatch: expected {}, got {}",
                entry.end,
                checkpoint_range.end
            );
        }

        // Fetch current metadata for conditional PUT (GCS requires version/generation)
        let meta = self.object_store.head(path).await.map_err(|e| {
            anyhow!(
                "Failed to get metadata for {} (file should exist for migration): {}",
                path,
                e
            )
        })?;

        self.put_conditional(
            path,
            payload,
            meta.e_tag.as_deref(),
            meta.version.as_deref(),
        )
        .await
    }

    /// Put a file with conditional update for migration mode.
    ///
    /// Uses `PutMode::Update` with etag/version for atomic replacement to prevent
    /// concurrent modification. GCS requires version (generation), other stores use e_tag.
    async fn put_conditional(
        &self,
        path: &ObjectPath,
        payload: PutPayload,
        expected_etag: Option<&str>,
        expected_version: Option<&str>,
    ) -> anyhow::Result<()> {
        let mode = if expected_version.is_some() || expected_etag.is_some() {
            PutMode::Update(UpdateVersion {
                e_tag: expected_etag.map(String::from),
                version: expected_version.map(String::from),
            })
        } else {
            PutMode::Create
        };

        self.object_store
            .put_opts(
                path,
                payload,
                PutOptions {
                    mode,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| match e {
                ObjectStoreError::Precondition { path, source } => {
                    anyhow!(
                        "Concurrent writer detected - etag mismatch for {}: {}",
                        path,
                        source
                    )
                }
                ObjectStoreError::AlreadyExists { path, source } => {
                    anyhow!(
                        "File already exists (expected for conditional create): {}: {}",
                        path,
                        source
                    )
                }
                _ => e.into(),
            })?;
        Ok(())
    }
}

impl FileRangeIndex {
    /// Create a new empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a file range entry.
    pub fn insert(&mut self, entry: FileRangeEntry) {
        self.ranges.insert(entry.start, entry);
    }

    /// Get the number of file ranges.
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Check if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Find the file range that contains the given checkpoint.
    pub fn find_containing(&self, checkpoint: u64) -> Option<&FileRangeEntry> {
        // Find the largest start <= checkpoint
        self.ranges
            .range(..=checkpoint)
            .next_back()
            .filter(|(_, entry)| checkpoint < entry.end)
            .map(|(_, entry)| entry)
    }

    /// Find the next file boundary at or after the given checkpoint.
    pub fn find_next_boundary(&self, checkpoint: u64) -> Option<&FileRangeEntry> {
        self.ranges
            .range(checkpoint..)
            .next()
            .map(|(_, entry)| entry)
    }

    /// Snap a start checkpoint to file boundaries for migration.
    ///
    /// - If checkpoint is inside a file → returns file start
    /// - If checkpoint is in a gap → returns next file start
    /// - If no files at or after checkpoint → returns None (error case)
    pub fn snap_to_boundary(&self, checkpoint: u64) -> Option<u64> {
        // Check if checkpoint is inside a file
        if let Some(entry) = self.find_containing(checkpoint) {
            return Some(entry.start);
        }
        // Check for next file after checkpoint
        if let Some(entry) = self.find_next_boundary(checkpoint) {
            return Some(entry.start);
        }
        // No files at or after checkpoint
        None
    }

    /// Snap an end checkpoint to file boundaries for migration.
    ///
    /// Returns the last checkpoint (inclusive) that should be processed to complete
    /// the file containing the given checkpoint:
    /// - If checkpoint is inside a file → returns file's last checkpoint (end - 1)
    /// - If checkpoint is in a gap → returns previous file's last checkpoint (end - 1)
    /// - If no files at or before checkpoint → returns None (error case)
    pub fn snap_end_to_boundary(&self, checkpoint: u64) -> Option<u64> {
        // Check if checkpoint is inside a file
        if let Some(entry) = self.find_containing(checkpoint) {
            // Return last checkpoint in file (end is exclusive, so end - 1 is last inclusive)
            return Some(entry.end - 1);
        }
        // Checkpoint is in a gap - find the previous file's end
        // Look for the largest start < checkpoint
        if let Some((_, entry)) = self.ranges.range(..checkpoint).next_back() {
            return Some(entry.end - 1);
        }
        // No files at or before checkpoint
        None
    }

    /// Get the first checkpoint across all ranges.
    pub fn first_checkpoint(&self) -> Option<u64> {
        self.ranges.keys().next().copied()
    }

    /// Get the last checkpoint across all ranges (exclusive).
    pub fn last_checkpoint_exclusive(&self) -> Option<u64> {
        self.ranges.values().map(|e| e.end).max()
    }

    /// Load file range index from object store.
    ///
    /// This lists all files in the pipeline directory to build the index of
    /// target ranges for migration. Progress is tracked via a separate watermark file.
    ///
    /// If `min_epoch` is provided, only epochs >= min_epoch are scanned, which
    /// reduces startup time when resuming a migration.
    pub async fn load_from_store(
        store: &Arc<dyn ObjectStore>,
        pipeline: &str,
        min_epoch: Option<u64>,
    ) -> Result<Self> {
        let mut index = Self::new();

        // Find all epoch directories under {pipeline}/epoch_*
        let prefix = ObjectPath::from(pipeline);
        let epoch_dirs = find_all_dirs_with_epoch_prefix(store, Some(&prefix)).await?;

        let skipped_epochs = min_epoch
            .map(|min| epoch_dirs.range(..min).count())
            .unwrap_or(0);

        for (epoch, epoch_path) in epoch_dirs {
            // Skip epochs before the watermark
            if let Some(min) = min_epoch
                && epoch < min
            {
                continue;
            }

            // List files in this epoch directory
            let list_result = store.list_with_delimiter(Some(&epoch_path)).await?;

            for obj in list_result.objects {
                // Parse checkpoint range from filename: {start}_{end}.{format}
                let Some(filename) = obj.location.filename() else {
                    continue;
                };
                let Some(range) = super::parse_checkpoint_range(filename) else {
                    continue;
                };

                index.insert(FileRangeEntry {
                    start: range.start,
                    end: range.end,
                    epoch,
                });
            }
        }

        info!(
            pipeline,
            num_files = index.len(),
            skipped_epochs,
            min_epoch,
            first_checkpoint = ?index.first_checkpoint(),
            last_checkpoint = ?index.last_checkpoint_exclusive(),
            "Loaded existing file ranges"
        );

        Ok(index)
    }
}

/// Construct the path for a migration watermark file.
///
/// Format: `_metadata/watermarks/{pipeline}@migration_{migration_id}.json`
pub(crate) fn migration_watermark_path(pipeline: &str, migration_id: &str) -> ObjectPath {
    ObjectPath::from(format!(
        "_metadata/watermarks/{}@migration_{}.json",
        pipeline, migration_id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    #[tokio::test]
    async fn test_migration_mode_watermark() {
        let object_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());

        // Create migration store directly to test watermark updates
        let migration_store = MigrationStore::new(object_store.clone(), "test_migration".into());

        // No watermark file yet
        let watermark = migration_store
            .committer_watermark("test_pipeline")
            .await
            .unwrap();
        assert!(watermark.is_none());

        // Update watermark (simulating what uploader does after upload)
        migration_store
            .update_watermark("test_pipeline", 5, 500)
            .await
            .unwrap();

        // Read it back
        let watermark = migration_store
            .committer_watermark("test_pipeline")
            .await
            .unwrap();
        assert!(watermark.is_some());
        let watermark = watermark.unwrap();
        assert_eq!(watermark.epoch_hi_inclusive, 5);
        assert_eq!(watermark.checkpoint_hi_inclusive, 500);
    }

    #[test]
    fn test_parse_checkpoint_range() {
        use super::super::parse_checkpoint_range;
        assert_eq!(parse_checkpoint_range("0_100.parquet"), Some(0..100));
        assert_eq!(parse_checkpoint_range("100_200.csv"), Some(100..200));
        assert_eq!(
            parse_checkpoint_range("1234_5678.parquet"),
            Some(1234..5678)
        );
        assert_eq!(parse_checkpoint_range("invalid"), None);
        assert_eq!(parse_checkpoint_range("no_extension"), None);
        assert_eq!(parse_checkpoint_range("a_b.parquet"), None);
    }

    #[test]
    fn test_find_containing() {
        let mut index = FileRangeIndex::new();
        index.insert(FileRangeEntry {
            start: 0,
            end: 100,
            epoch: 0,
        });
        index.insert(FileRangeEntry {
            start: 100,
            end: 200,
            epoch: 0,
        });
        index.insert(FileRangeEntry {
            start: 200,
            end: 300,
            epoch: 1,
        });

        // Test checkpoint in first range
        let result = index.find_containing(50);
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.start, 0);
        assert_eq!(entry.end, 100);

        // Test checkpoint at boundary (should be in second range)
        let result = index.find_containing(100);
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.start, 100);
        assert_eq!(entry.end, 200);

        // Test checkpoint not in any range
        let result = index.find_containing(300);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_next_boundary() {
        let mut index = FileRangeIndex::new();
        index.insert(FileRangeEntry {
            start: 0,
            end: 100,
            epoch: 0,
        });
        index.insert(FileRangeEntry {
            start: 100,
            end: 200,
            epoch: 0,
        });

        // From checkpoint 0, next boundary is at 0
        let result = index.find_next_boundary(0);
        assert!(result.is_some());
        assert_eq!(result.unwrap().start, 0);

        // From checkpoint 50, next boundary is at 100
        let result = index.find_next_boundary(50);
        assert!(result.is_some());
        assert_eq!(result.unwrap().start, 100);

        // From checkpoint 200, no more boundaries
        let result = index.find_next_boundary(200);
        assert!(result.is_none());
    }

    #[test]
    fn test_snap_to_boundary() {
        let mut index = FileRangeIndex::new();
        // Files: 0-100, 200-300 (gap at 100-200)
        index.insert(FileRangeEntry {
            start: 0,
            end: 100,
            epoch: 0,
        });
        index.insert(FileRangeEntry {
            start: 200,
            end: 300,
            epoch: 1,
        });

        // Checkpoint inside first file → snap to file start
        assert_eq!(index.snap_to_boundary(50), Some(0));
        assert_eq!(index.snap_to_boundary(0), Some(0));
        assert_eq!(index.snap_to_boundary(99), Some(0));

        // Checkpoint in gap → snap to next file start
        assert_eq!(index.snap_to_boundary(100), Some(200));
        assert_eq!(index.snap_to_boundary(150), Some(200));
        assert_eq!(index.snap_to_boundary(199), Some(200));

        // Checkpoint inside second file → snap to file start
        assert_eq!(index.snap_to_boundary(200), Some(200));
        assert_eq!(index.snap_to_boundary(250), Some(200));
        assert_eq!(index.snap_to_boundary(299), Some(200));

        // Checkpoint beyond all files → None (error case)
        assert_eq!(index.snap_to_boundary(300), None);
        assert_eq!(index.snap_to_boundary(1000), None);
    }

    #[test]
    fn test_snap_end_to_boundary() {
        let mut index = FileRangeIndex::new();
        // Files: 0-100, 200-300 (gap at 100-200)
        // File ranges are [start, end) so file 0-100 contains checkpoints 0-99
        index.insert(FileRangeEntry {
            start: 0,
            end: 100,
            epoch: 0,
        });
        index.insert(FileRangeEntry {
            start: 200,
            end: 300,
            epoch: 1,
        });

        // Checkpoint inside first file → snap to last checkpoint in file (99)
        assert_eq!(index.snap_end_to_boundary(0), Some(99));
        assert_eq!(index.snap_end_to_boundary(50), Some(99));
        assert_eq!(index.snap_end_to_boundary(99), Some(99));

        // Checkpoint in gap → snap to previous file's last checkpoint (99)
        assert_eq!(index.snap_end_to_boundary(100), Some(99));
        assert_eq!(index.snap_end_to_boundary(150), Some(99));
        assert_eq!(index.snap_end_to_boundary(199), Some(99));

        // Checkpoint inside second file → snap to last checkpoint in file (299)
        assert_eq!(index.snap_end_to_boundary(200), Some(299));
        assert_eq!(index.snap_end_to_boundary(250), Some(299));
        assert_eq!(index.snap_end_to_boundary(299), Some(299));

        // Checkpoint at or beyond second file end → snap to last checkpoint (299)
        assert_eq!(index.snap_end_to_boundary(300), Some(299));
        assert_eq!(index.snap_end_to_boundary(1000), Some(299));
    }
}
