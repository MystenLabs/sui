// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use anyhow::anyhow;
use arrow_array::ArrayRef;
use arrow_array::RecordBatch;
use arrow_array::builder::ArrayBuilder;
use arrow_array::builder::BooleanBuilder;
use arrow_array::builder::GenericStringBuilder;
use arrow_array::builder::Int64Builder;
use arrow_array::builder::UInt64Builder;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use crate::handlers::CheckpointRows;
use crate::schema::ColumnValue;

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
}

impl ParquetWriter {
    pub fn new() -> Result<Self> {
        Ok(Self { builders: vec![] })
    }

    /// Writes the given rows to the Parquet buffer.
    ///
    /// Uses `RowSchema::get_column()` for dynamic column access via trait objects.
    pub fn write(&mut self, checkpoint: &CheckpointRows) -> Result<()> {
        if checkpoint.is_empty() {
            return Ok(());
        }

        for row in checkpoint.iter() {
            // Lazily sample the first row to infer the schema
            if self.builders.is_empty() {
                for col_idx in 0..row.column_count() {
                    let value = row.get_column(col_idx)?;
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

            for col_idx in 0..row.column_count() {
                let value = row.get_column(col_idx)?;
                match (&mut self.builders[col_idx], value) {
                    (ColumnBuilder::U64(b), ColumnValue::U64(v)) => b.append_value(v),
                    (ColumnBuilder::I64(b), ColumnValue::I64(v)) => b.append_value(v),
                    (ColumnBuilder::Bool(b), ColumnValue::Bool(v)) => b.append_value(v),
                    (ColumnBuilder::Str(b), ColumnValue::Str(v)) => b.append_value(v.as_ref()),

                    (ColumnBuilder::U64(b), ColumnValue::OptionU64(opt)) => match opt {
                        Some(v) => b.append_value(v),
                        None => b.append_null(),
                    },
                    (ColumnBuilder::Str(b), ColumnValue::OptionStr(opt)) => match opt {
                        Some(v) => b.append_value(v.as_ref()),
                        None => b.append_null(),
                    },

                    _ => return Err(anyhow!("type mismatch on column {}", col_idx)),
                }
            }
        }

        Ok(())
    }

    /// Flushes accumulated rows to an in-memory Parquet buffer.
    ///
    /// Takes schema as a parameter since `schema()` is not object-safe.
    pub fn flush(&mut self, schema: &[&str]) -> Result<Option<Vec<u8>>> {
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

        let batch = RecordBatch::try_from_iter(schema.iter().zip(arrays))?;

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
}
