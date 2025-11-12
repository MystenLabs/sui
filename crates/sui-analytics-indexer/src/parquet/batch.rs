// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, anyhow};
use object_store::path::Path as ObjectPath;
use serde::Serialize;
use std::path::PathBuf;
use sui_types::base_types::EpochId;
use tempfile::TempDir;

use crate::parquet::writer::ParquetWriter;
use crate::{FileFormat, FileType, ParquetSchema};

/// Batch type for accumulating Parquet rows and managing file lifecycle
pub struct ParquetBatch<S: Serialize + ParquetSchema> {
    writer: ParquetWriter,
    temp_dir: TempDir,
    file_type: FileType,
    current_file_path: Option<PathBuf>,
    current_epoch: EpochId,
    checkpoint_range_start: u64,
    last_checkpoint: u64,
    _phantom: std::marker::PhantomData<S>,
}

impl<S: Serialize + ParquetSchema + 'static> ParquetBatch<S> {
    pub fn new(file_type: FileType, start_checkpoint: u64) -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let writer = ParquetWriter::new(temp_dir.path(), file_type, start_checkpoint)?;

        Ok(Self {
            writer,
            temp_dir,
            file_type,
            current_file_path: None,
            current_epoch: 0,
            checkpoint_range_start: start_checkpoint,
            last_checkpoint: start_checkpoint,
            _phantom: std::marker::PhantomData,
        })
    }

    /// Write rows directly to the ParquetWriter
    pub fn write_rows<I>(&mut self, rows: I) -> Result<()>
    where
        I: Iterator<Item = S>,
        S: Send + Sync + 'static,
    {
        let collected: Vec<S> = rows.collect();
        self.writer.write(Box::new(collected.into_iter()))
    }

    /// Get current row count
    pub fn row_count(&self) -> Result<usize> {
        self.writer.rows()
    }

    /// Update the last checkpoint seen (for tracking the range)
    pub fn update_last_checkpoint(&mut self, checkpoint: u64) {
        self.last_checkpoint = checkpoint;
    }

    /// Update the current epoch
    pub fn set_epoch(&mut self, epoch: EpochId) {
        self.current_epoch = epoch;
    }

    /// Flush accumulated rows to a Parquet file in the temp directory
    /// Returns the file path if successful
    pub fn flush(&mut self) -> Result<Option<PathBuf>> {
        let end_checkpoint = self.last_checkpoint + 1;
        if !self.writer.flush::<S>(end_checkpoint)? {
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

        self.current_file_path = Some(file_path.clone());
        Ok(Some(file_path))
    }

    /// Get the object store path for the current file
    pub fn object_store_path(&self) -> ObjectPath {
        let checkpoint_range = self.checkpoint_range_start..(self.last_checkpoint + 1);
        self.file_type
            .file_path(FileFormat::PARQUET, self.current_epoch, checkpoint_range)
    }

    /// Get the current file path for uploading
    pub fn current_file_path(&self) -> Option<&PathBuf> {
        self.current_file_path.as_ref()
    }

    /// Reset the batch after successful upload
    pub fn reset(&mut self, start_checkpoint: u64) -> Result<()> {
        self.writer.reset(self.current_epoch, start_checkpoint)?;
        self.current_file_path = None;
        self.checkpoint_range_start = start_checkpoint;
        self.last_checkpoint = start_checkpoint;
        Ok(())
    }
}
