// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, anyhow};
use object_store::path::Path as ObjectPath;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use sui_types::base_types::EpochId;
use tempfile::TempDir;

use crate::parquet::writer::ParquetWriter;
use crate::writers::AnalyticsWriter;
use crate::{FileFormat, FileType, ParquetSchema};

/// Configuration for Parquet file cutting behavior
#[derive(Clone)]
pub struct ParquetBatchConfig {
    /// Number of rows before flushing to disk
    pub min_rows: usize,
    /// File size in MB before uploading to remote
    pub max_file_size_mb: u64,
}

impl Default for ParquetBatchConfig {
    fn default() -> Self {
        Self {
            min_rows: 100_000,
            max_file_size_mb: 100,
        }
    }
}

/// Batch type for accumulating Parquet rows and managing file lifecycle
pub struct ParquetBatch<S: Serialize + ParquetSchema> {
    writer: ParquetWriter,
    temp_dir: TempDir,
    file_type: FileType,
    config: ParquetBatchConfig,
    current_file_path: Option<PathBuf>,
    current_epoch: EpochId,
    checkpoint_range_start: u64,
    _phantom: std::marker::PhantomData<S>,
}

impl<S: Serialize + ParquetSchema> ParquetBatch<S> {
    pub fn new(
        file_type: FileType,
        start_checkpoint: u64,
        config: ParquetBatchConfig,
    ) -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let writer = ParquetWriter::new(temp_dir.path(), file_type, start_checkpoint)?;

        Ok(Self {
            writer,
            temp_dir,
            file_type,
            config,
            current_file_path: None,
            current_epoch: 0,
            checkpoint_range_start: start_checkpoint,
            _phantom: std::marker::PhantomData,
        })
    }

    /// Add rows to the batch
    #[allow(dead_code)]
    pub fn add_rows(
        &mut self,
        rows: impl Iterator<Item = S> + Send + Sync + 'static,
    ) -> Result<()> {
        AnalyticsWriter::<S>::write(&mut self.writer, Box::new(rows))
    }

    /// Get current row count
    #[allow(dead_code)]
    pub fn row_count(&self) -> Result<usize> {
        AnalyticsWriter::<S>::rows(&self.writer)
    }

    /// Flush accumulated rows to a Parquet file in the temp directory
    /// Returns the file size in bytes if a file was written
    #[allow(dead_code)]
    pub fn flush(&mut self, end_checkpoint: u64) -> Result<Option<u64>> {
        if !AnalyticsWriter::<S>::flush(&mut self.writer, end_checkpoint)? {
            return Ok(None);
        }

        // Build the file path where ParquetWriter wrote the file
        let checkpoint_range = self.checkpoint_range_start..end_checkpoint;
        let file_name = format!(
            "{}_{}.{}",
            checkpoint_range.start,
            checkpoint_range.end,
            FileFormat::PARQUET.file_suffix()
        );
        let epoch_dir = format!("epoch_{}", self.current_epoch);
        let file_path = self
            .temp_dir
            .path()
            .join(self.file_type.dir_prefix().as_ref())
            .join(epoch_dir)
            .join(&file_name);

        if !file_path.exists() {
            return Err(anyhow!(
                "Expected file not found after flush: {}",
                file_path.display()
            ));
        }

        let file_size = fs::metadata(&file_path)?.len();
        self.current_file_path = Some(file_path);
        Ok(Some(file_size))
    }

    /// Check if the current file should be uploaded based on size threshold
    pub fn should_upload(&self, file_size_bytes: u64) -> bool {
        let max_bytes = self.config.max_file_size_mb * 1024 * 1024;
        file_size_bytes >= max_bytes
    }

    /// Get the current file path for uploading
    pub fn current_file_path(&self) -> Option<&PathBuf> {
        self.current_file_path.as_ref()
    }

    /// Get the object store path for the current file
    #[allow(dead_code)]
    pub fn object_store_path(&self) -> Result<ObjectPath> {
        let checkpoint_range = self.checkpoint_range_start
            ..AnalyticsWriter::<S>::rows(&self.writer).map(|_| self.checkpoint_range_start)?;
        Ok(self
            .file_type
            .file_path(FileFormat::PARQUET, self.current_epoch, checkpoint_range))
    }

    /// Reset the batch after successful upload
    #[allow(dead_code)]
    pub fn reset(&mut self, epoch: EpochId, start_checkpoint: u64) -> Result<()> {
        AnalyticsWriter::<S>::reset(&mut self.writer, epoch, start_checkpoint)?;
        self.current_file_path = None;
        self.current_epoch = epoch;
        self.checkpoint_range_start = start_checkpoint;
        Ok(())
    }

    /// Update the current epoch
    pub fn set_epoch(&mut self, epoch: EpochId) {
        self.current_epoch = epoch;
    }
}

impl<S: Serialize + ParquetSchema> Default for ParquetBatch<S> {
    fn default() -> Self {
        Self::new(FileType::Checkpoint, 0, ParquetBatchConfig::default())
            .expect("Failed to create default ParquetBatch")
    }
}
