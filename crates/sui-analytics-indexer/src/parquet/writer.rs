// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ParquetSchema, ParquetValue};
use anyhow::{Result, anyhow};
use arrow_array::{
    ArrayRef, RecordBatch,
    builder::{ArrayBuilder, BooleanBuilder, GenericStringBuilder, Int64Builder, UInt64Builder},
};
use serde::Serialize;
use std::ops::Range;
use std::sync::Arc;
use sui_types::base_types::EpochId;

use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

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

// Save table entries to parquet files.
pub struct ParquetWriter {
    epoch: EpochId,
    checkpoint_range: Range<u64>,
    builders: Vec<ColumnBuilder>,
    row_count: usize,
}

impl ParquetWriter {
    pub fn new(start_checkpoint_seq_num: u64) -> Result<Self> {
        Ok(Self {
            epoch: 0,
            checkpoint_range: start_checkpoint_seq_num..u64::MAX,
            builders: vec![],
            row_count: 0,
        })
    }
}

impl ParquetWriter {
    /// Persist given rows into a file
    pub fn write<S: Serialize + ParquetSchema>(
        &mut self,
        rows: Box<dyn Iterator<Item = S> + Send + Sync>,
    ) -> Result<()> {
        // Make the iterator peekable
        let mut row_iter = rows.peekable();

        // Check if iterator is empty
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
                    ParquetValue::U64(_) | ParquetValue::OptionU64(_) => {
                        ColumnBuilder::U64(UInt64Builder::new())
                    }
                    ParquetValue::I64(_) => ColumnBuilder::I64(Int64Builder::new()),
                    ParquetValue::Bool(_) => ColumnBuilder::Bool(BooleanBuilder::new()),
                    ParquetValue::Str(_) | ParquetValue::OptionStr(_) => {
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
                    (ColumnBuilder::U64(b), ParquetValue::U64(v)) => b.append_value(v),
                    (ColumnBuilder::I64(b), ParquetValue::I64(v)) => b.append_value(v),
                    (ColumnBuilder::Bool(b), ParquetValue::Bool(v)) => b.append_value(v),
                    (ColumnBuilder::Str(b), ParquetValue::Str(v)) => b.append_value(&v),

                    (ColumnBuilder::U64(b), ParquetValue::OptionU64(opt)) => match opt {
                        Some(v) => b.append_value(v),
                        None => b.append_null(),
                    },
                    (ColumnBuilder::Str(b), ParquetValue::OptionStr(opt)) => match opt {
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

    /// Flush accumulated rows to an in-memory Parquet buffer
    pub fn flush<S: Serialize + ParquetSchema>(
        &mut self,
        end_checkpoint_seq_num: u64,
    ) -> Result<Option<Vec<u8>>> {
        // Nothing to flush if builders aren't initialized or are empty
        if self.builders.is_empty()
            || self
                .builders
                .iter_mut()
                .all(|b| b.as_any_builder().is_empty())
        {
            return Ok(None);
        }

        self.checkpoint_range.end = end_checkpoint_seq_num;

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

    /// Reset internal state with given epoch and checkpoint sequence number
    pub fn reset(&mut self, epoch_num: EpochId, start_checkpoint_seq_num: u64) -> Result<()> {
        self.epoch = epoch_num;
        self.checkpoint_range = start_checkpoint_seq_num..u64::MAX;
        self.builders.clear();
        self.row_count = 0;
        Ok(())
    }

    /// Number of rows accumulated since last flush
    pub fn rows(&self) -> Result<usize> {
        Ok(self.row_count)
    }
}
