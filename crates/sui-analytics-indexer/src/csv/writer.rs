// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use csv::WriterBuilder;
use serde::Serialize;

use crate::ParquetSchema;

// Save table entries to CSV format in memory
pub struct CsvWriter {
    buffer: Vec<u8>,
    row_count: usize,
}

impl CsvWriter {
    pub fn new() -> Result<Self> {
        Ok(Self {
            buffer: Vec::new(),
            row_count: 0,
        })
    }

    /// Write rows to in-memory CSV buffer
    pub fn write<S: Serialize + ParquetSchema>(
        &mut self,
        rows: Box<dyn Iterator<Item = S> + Send + Sync>,
    ) -> Result<()> {
        let mut writer = WriterBuilder::new()
            .has_headers(false)
            .delimiter(b'|')
            .from_writer(&mut self.buffer);

        let mut count = 0;
        for row in rows {
            writer.serialize(row)?;
            count += 1;
        }

        writer.flush()?;
        self.row_count += count;
        Ok(())
    }

    /// Flush accumulated rows to a CSV byte buffer
    pub fn flush<S: Serialize + ParquetSchema>(&mut self) -> Result<Option<Vec<u8>>> {
        // Nothing to flush if buffer is empty
        if self.buffer.is_empty() {
            return Ok(None);
        }

        // Take ownership of the buffer and replace with a new one
        let bytes = std::mem::take(&mut self.buffer);

        Ok(Some(bytes))
    }

    /// Number of rows accumulated since last flush
    pub fn rows(&self) -> Result<usize> {
        Ok(self.row_count)
    }
}
