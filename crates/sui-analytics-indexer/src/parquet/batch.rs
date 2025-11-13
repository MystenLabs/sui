// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use bytes::Bytes;
use object_store::path::Path as ObjectPath;
use serde::Serialize;
use sui_types::base_types::EpochId;

use crate::ParquetSchema;
use crate::parquet::writer::ParquetWriter;

/// Batch type for accumulating Parquet rows in memory
pub struct ParquetBatch<S: Serialize + ParquetSchema> {
    writer: ParquetWriter,
    dir_prefix: String,
    current_file_bytes: Option<Bytes>,
    current_epoch: EpochId,
    checkpoint_range_start: u64,
    last_checkpoint: u64,
    _phantom: std::marker::PhantomData<S>,
}

impl<S: Serialize + ParquetSchema + 'static> ParquetBatch<S> {
    pub fn new(dir_prefix: String, start_checkpoint: u64) -> Result<Self> {
        let writer = ParquetWriter::new(start_checkpoint)?;

        Ok(Self {
            writer,
            dir_prefix,
            current_file_bytes: None,
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

    /// Flush accumulated rows to an in-memory Parquet buffer
    /// Returns the Parquet bytes if successful
    pub fn flush(&mut self) -> Result<Option<Bytes>> {
        let end_checkpoint = self.last_checkpoint + 1;
        let buffer = self.writer.flush::<S>(end_checkpoint)?;

        if let Some(bytes_vec) = buffer {
            let bytes = Bytes::from(bytes_vec);
            self.current_file_bytes = Some(bytes.clone());
            Ok(Some(bytes))
        } else {
            Ok(None)
        }
    }

    /// Get the object store path for the current file
    pub fn object_store_path(&self) -> ObjectPath {
        let checkpoint_range = self.checkpoint_range_start..(self.last_checkpoint + 1);
        let path_buf =
            crate::construct_file_path(&self.dir_prefix, self.current_epoch, checkpoint_range);
        // Convert PathBuf to ObjectPath (always uses forward slashes)
        ObjectPath::from_iter(
            path_buf
                .components()
                .map(|c| c.as_os_str().to_str().expect("path should be valid UTF-8")),
        )
    }

    /// Get the current file bytes for uploading
    pub fn current_file_bytes(&self) -> Option<&Bytes> {
        self.current_file_bytes.as_ref()
    }

    /// Reset the batch after successful upload
    pub fn reset(&mut self, start_checkpoint: u64) -> Result<()> {
        self.writer.reset(self.current_epoch, start_checkpoint)?;
        self.current_file_bytes = None;
        self.checkpoint_range_start = start_checkpoint;
        self.last_checkpoint = start_checkpoint;
        Ok(())
    }
}
