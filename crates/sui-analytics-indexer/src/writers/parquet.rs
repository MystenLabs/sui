// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, anyhow};
use arrow_array::{
    ArrayRef, RecordBatch,
    builder::{ArrayBuilder, BooleanBuilder, GenericStringBuilder, Int64Builder, UInt64Builder},
};
use serde::Serialize;
use std::sync::Arc;

use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use crate::schema::{ColumnValue, RowSchema};

type StrBuilder = GenericStringBuilder<i32>;

enum ColumnBuilder {
    U64(UInt64Builder),
    I64(Int64Builder),
    Bool(BooleanBuilder),
    Str(StrBuilder),
}

impl ColumnBuilder {
    fn as_any_builder(&mut self) -> &mut dyn ArrayBuilder {
        match self {
            Self::U64(b) => b,
            Self::I64(b) => b,
            Self::Bool(b) => b,
            Self::Str(b) => b,
        }
    }

    fn finish(self) -> ArrayRef {
        match self {
            Self::U64(mut b) => Arc::new(b.finish()),
            Self::I64(mut b) => Arc::new(b.finish()),
            Self::Bool(mut b) => Arc::new(b.finish()),
            Self::Str(mut b) => Arc::new(b.finish()),
        }
    }
}

/// Writes table entries to Parquet format in memory.
pub struct ParquetWriter {
    builders: Vec<ColumnBuilder>,
    row_count: usize,
}

impl ParquetWriter {
    pub fn new() -> Result<Self> {
        Ok(Self {
            builders: vec![],
            row_count: 0,
        })
    }

    /// Writes the given rows to the Parquet buffer.
    pub fn write<S: Serialize + RowSchema>(
        &mut self,
        rows: Box<dyn Iterator<Item = S> + Send + Sync>,
    ) -> Result<()> {
        let mut row_iter = rows.peekable();

        if row_iter.peek().is_none() {
            return Ok(());
        }

        // Lazily sample the first row to infer the schema and decide which concrete builder to instantiate
        if self.builders.is_empty()
            && let Some(first_row) = row_iter.peek()
        {
            for col_idx in 0..S::schema().len() {
                let value = first_row.get_column(col_idx);
                self.builders.push(match value {
                    ColumnValue::U64(_) | ColumnValue::OptionU64(_) => {
                        ColumnBuilder::U64(UInt64Builder::new())
                    }
                    ColumnValue::I64(_) => ColumnBuilder::I64(Int64Builder::new()),
                    ColumnValue::Bool(_) => ColumnBuilder::Bool(BooleanBuilder::new()),
                    ColumnValue::Str(_) | ColumnValue::OptionStr(_) => {
                        ColumnBuilder::Str(StrBuilder::new())
                    }
                });
            }
        }

        let mut count = 0;
        for row in row_iter {
            count += 1;
            for (col_idx, value) in (0..S::schema().len()).map(|i| (i, row.get_column(i))) {
                match (&mut self.builders[col_idx], value) {
                    (ColumnBuilder::U64(b), ColumnValue::U64(v)) => b.append_value(v),
                    (ColumnBuilder::I64(b), ColumnValue::I64(v)) => b.append_value(v),
                    (ColumnBuilder::Bool(b), ColumnValue::Bool(v)) => b.append_value(v),
                    (ColumnBuilder::Str(b), ColumnValue::Str(v)) => b.append_value(&v),

                    (ColumnBuilder::U64(b), ColumnValue::OptionU64(opt)) => match opt {
                        Some(v) => b.append_value(v),
                        None => b.append_null(),
                    },
                    (ColumnBuilder::Str(b), ColumnValue::OptionStr(opt)) => match opt {
                        Some(v) => b.append_value(&v),
                        None => b.append_null(),
                    },

                    _ => return Err(anyhow!("type mismatch on column {}", col_idx)),
                }
            }
        }

        self.row_count += count;
        Ok(())
    }

    /// Flushes accumulated rows to an in-memory Parquet buffer.
    pub fn flush<S: Serialize + RowSchema>(&mut self) -> Result<Option<Vec<u8>>> {
        // Nothing to flush if builders aren't initialized or are empty
        if self.builders.is_empty()
            || self
                .builders
                .iter_mut()
                .all(|b| b.as_any_builder().is_empty())
        {
            return Ok(None);
        }

        // Turn builders into Arrow arrays.
        let arrays: Vec<ArrayRef> = std::mem::take(&mut self.builders)
            .into_iter()
            .map(|b| b.finish())
            .collect();

        let batch = RecordBatch::try_from_iter(S::schema().iter().zip(arrays))?;

        let properties = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build();

        // Write to in-memory buffer
        let mut buffer = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut buffer, batch.schema(), Some(properties))?;
        writer.write(&batch)?;
        writer.close()?;

        Ok(Some(buffer))
    }

    /// Returns the number of rows accumulated since the last flush.
    pub fn rows(&self) -> Result<usize> {
        Ok(self.row_count)
    }
}
