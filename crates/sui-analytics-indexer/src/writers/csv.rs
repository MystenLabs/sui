// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use csv::WriterBuilder;
use serde::Serialize;

use crate::schema::RowSchema;

/// Writes table entries to CSV format in memory.
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

    /// Writes rows to the in-memory CSV buffer.
    pub fn write<S: Serialize + RowSchema>(&mut self, rows: &[S]) -> Result<()> {
        let mut writer = WriterBuilder::new()
            .has_headers(false)
            .delimiter(b'|')
            .from_writer(&mut self.buffer);

        for row in rows {
            writer.serialize(row)?;
        }

        writer.flush()?;
        self.row_count += rows.len();
        Ok(())
    }

    /// Flushes accumulated rows to a CSV byte buffer.
    pub fn flush<S: Serialize + RowSchema>(&mut self) -> Result<Option<Vec<u8>>> {
        if self.buffer.is_empty() {
            return Ok(None);
        }

        let bytes = std::mem::take(&mut self.buffer);
        Ok(Some(bytes))
    }

    /// Returns the number of rows accumulated since the last flush.
    pub fn rows(&self) -> Result<usize> {
        Ok(self.row_count)
    }
}
