// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{AnalyticsWriter, FileFormat, FileType, ParquetSchema, ParquetValue};
use anyhow::{anyhow, Result};
use arrow_array::{
    builder::{ArrayBuilder, BooleanBuilder, GenericStringBuilder, Int64Builder, UInt64Builder},
    ArrayRef, RecordBatch,
};
use serde::Serialize;
use std::fs::{create_dir_all, remove_file, File};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sui_storage::object_store::util::path_to_filesystem;
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
pub(crate) struct ParquetWriter {
    root_dir_path: PathBuf,
    file_type: FileType,
    epoch: EpochId,
    checkpoint_range: Range<u64>,
    builders: Vec<ColumnBuilder>,
    row_count: usize,
}

impl ParquetWriter {
    pub(crate) fn new(
        root_dir_path: &Path,
        file_type: FileType,
        start_checkpoint_seq_num: u64,
    ) -> Result<Self> {
        Ok(Self {
            root_dir_path: root_dir_path.to_path_buf(),
            file_type,
            epoch: 0,
            checkpoint_range: start_checkpoint_seq_num..u64::MAX,
            builders: vec![],
            row_count: 0,
        })
    }

    fn file(&self) -> Result<File> {
        let file_path = path_to_filesystem(
            self.root_dir_path.clone(),
            &self.file_type.file_path(
                FileFormat::PARQUET,
                self.epoch,
                self.checkpoint_range.clone(),
            ),
        )?;
        create_dir_all(file_path.parent().ok_or(anyhow!("Bad directory path"))?)?;
        if file_path.exists() {
            remove_file(&file_path)?;
        }
        Ok(File::create(&file_path)?)
    }
}

impl<S: Serialize + ParquetSchema> AnalyticsWriter<S> for ParquetWriter {
    fn file_format(&self) -> Result<FileFormat> {
        Ok(FileFormat::PARQUET)
    }

    fn write(&mut self, rows: &[S]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }

        self.row_count += rows.len();

        //  Lazily sample the first row to infer the schema and decide which concrete builder to instantiate.
        if self.builders.is_empty() {
            for col_idx in 0..S::schema().len() {
                let value = rows[0].get_column(col_idx);
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

        for row in rows {
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

        Ok(())
    }

    fn flush(&mut self, end_checkpoint_seq_num: u64) -> Result<bool> {
        // Nothing to flush if builders aren't initialized or are empty
        if self.builders.is_empty()
            || self
                .builders
                .iter_mut()
                .all(|b| b.as_any_builder().is_empty())
        {
            return Ok(false);
        }

        self.checkpoint_range.end = end_checkpoint_seq_num;

        // Turn builders into Arrow arrays.
        let arrays: Vec<ArrayRef> = std::mem::take(&mut self.builders)
            .into_iter()
            .map(|b| b.finish())
            .collect();

        let batch = RecordBatch::try_from_iter(S::schema().iter().zip(arrays))?;

        let propertiess = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build();
        let mut writer = ArrowWriter::try_new(self.file()?, batch.schema(), Some(propertiess))?;
        writer.write(&batch)?;
        writer.close()?;
        Ok(true)
    }

    fn reset(&mut self, epoch_num: EpochId, start_checkpoint_seq_num: u64) -> Result<()> {
        self.epoch = epoch_num;
        self.checkpoint_range = start_checkpoint_seq_num..u64::MAX;
        self.builders.clear();
        self.row_count = 0;
        Ok(())
    }

    fn file_size(&self) -> Result<Option<u64>> {
        // parquet writer doesn't write records in a temp staging file
        // and only flushes records after serializing and compressing them
        // when flush is invoked
        Ok(None)
    }

    fn rows(&self) -> Result<usize> {
        Ok(self.row_count)
    }
}
