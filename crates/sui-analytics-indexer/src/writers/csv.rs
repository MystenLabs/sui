// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;

use anyhow::Result;
use csv::WriterBuilder;

use crate::handlers::CheckpointRows;
use crate::schema::ColumnValue;

/// Writes table entries to CSV format in memory.
pub struct CsvWriter {
    buffer: Vec<u8>,
}

impl CsvWriter {
    pub fn new() -> Result<Self> {
        Ok(Self { buffer: Vec::new() })
    }

    /// Writes rows to the in-memory CSV buffer using field-by-field serialization.
    ///
    /// Uses the csv crate's `write_field` API for proper delimiter escaping,
    /// iterating over columns via `RowSchema::get_column()`.
    pub fn write(&mut self, checkpoint: &CheckpointRows) -> Result<()> {
        let mut writer = WriterBuilder::new()
            .has_headers(false)
            .delimiter(b'|')
            .from_writer(&mut self.buffer);

        for row in checkpoint.iter() {
            for col_idx in 0..row.column_count() {
                let value = row.get_column(col_idx)?;
                writer.write_field(column_value_to_cow(&value).as_ref())?;
            }
            writer.write_record(None::<&[u8]>)?;
        }

        writer.flush()?;
        Ok(())
    }

    /// Flushes accumulated rows to a CSV byte buffer.
    pub fn flush(&mut self) -> Result<Option<Vec<u8>>> {
        if self.buffer.is_empty() {
            return Ok(None);
        }

        let bytes = std::mem::take(&mut self.buffer);
        Ok(Some(bytes))
    }
}

/// Convert a ColumnValue to its string representation for CSV output.
///
/// Returns `Cow<str>` to avoid allocating for string values that are already borrowed.
fn column_value_to_cow<'a>(value: &ColumnValue<'a>) -> Cow<'a, str> {
    match value {
        ColumnValue::U64(v) => Cow::Owned(v.to_string()),
        ColumnValue::I64(v) => Cow::Owned(v.to_string()),
        ColumnValue::Bool(v) => Cow::Owned(v.to_string()),
        ColumnValue::Str(v) => v.clone(),
        ColumnValue::OptionU64(Some(v)) => Cow::Owned(v.to_string()),
        ColumnValue::OptionU64(None) => Cow::Borrowed(""),
        ColumnValue::OptionStr(Some(v)) => v.clone(),
        ColumnValue::OptionStr(None) => Cow::Borrowed(""),
    }
}
