// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{AnalyticsWriter, FileFormat, FileType, ParquetSchema, ParquetValue};
use anyhow::{anyhow, Result};
use arrow::util::bit_util::ceil;
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

    fn size_bytes(&self) -> usize {
        match self {
            Self::U64(b) => b.len() * std::mem::size_of::<u64>(),
            Self::I64(b) => b.len() * std::mem::size_of::<i64>(),
            Self::Bool(b) => {
                // Boolean columns always store a data bitmap (1 bit per value),
                // packed into bytes. If any nulls are appended, a second bitmap
                // (the "validity" or null-bitmap) is also allocated.
                //
                // `b.len()` counts values; we round up to get the number of bytes.
                // `validity_slice()` returns None if no nulls were seen yet,
                // avoiding over-counting when the column is fully non-null.
                let data_bytes = ceil(b.len(), 8);
                let null_bytes = b.validity_slice().map_or(0, |buf| buf.len());
                data_bytes + null_bytes
            }
            Self::Str(b) => b.values_slice().len(),
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
        Ok(())
    }

    /// Give the caller a rough idea of "how big is the batch in memory?"
    fn estimate_file_size(&self) -> Result<Option<u64>> {
        if self.builders.is_empty() {
            return Ok(Some(0));
        }

        let bytes: u64 = self.builders.iter().map(|b| b.size_bytes() as u64).sum();
        Ok(Some(bytes))
    }
}

#[cfg(test)]
mod size_estimate_tests {
    use super::*;
    use serde::Serialize;
    use tempfile::tempdir;

    // ─────────────────────────────────────────────────────────────
    // 1.  A minimal row type that exercises every ColumnBuilder:
    //     * u64, i64, bool, string
    // ─────────────────────────────────────────────────────────────
    #[derive(Serialize)]
    struct Row {
        u: u64,
        i: i64,
        b: bool,
        s: String,
    }

    impl ParquetSchema for Row {
        fn schema() -> &'static [ParquetValue] {
            use ParquetValue::*;
            const SCHEMA: &[ParquetValue] = &[U64, I64, Bool, Str];
            SCHEMA
        }
        fn get_column(&self, idx: usize) -> ParquetValue<'_> {
            use ParquetValue::*;
            match idx {
                0 => U64(self.u),
                1 => I64(self.i),
                2 => Bool(self.b),
                3 => Str(&self.s),
                _ => unreachable!(),
            }
        }
    }

    // ─────────────────────────────────────────────────────────────
    // 2.  The actual test
    // ─────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn parquet_size_estimate_reasonable() -> anyhow::Result<()> {
        // temp dir for output files
        let dir = tempdir()?;
        let mut writer = ParquetWriter::new(
            dir.path(),
            FileType::Other("test".into()), // any FileType variant works
            0,
        )?;

        // build 10_000 rows (~ 1 MiB uncompressed)
        let rows: Vec<Row> = (0..10_000)
            .map(|i| Row {
                u: i as u64,
                i: i as i64 * -1,
                b: i % 2 == 0,
                s: format!("string-{}", i),
            })
            .collect();

        writer.write(&rows)?;
        let estimate = writer.estimate_file_size()?.expect("size estimate");

        // flush to disk
        writer.flush(0)?;

        // locate the single file we just wrote
        let mut file_bytes = 0;
        for entry in std::fs::read_dir(dir.path())? {
            let path = entry?.path();
            if path.extension().and_then(|s| s.to_str()) == Some("parquet") {
                file_bytes += path.metadata()?.len();
            }
        }

        // ─────────────────────────────────────────────────────────
        // Assertions
        // ─────────────────────────────────────────────────────────
        assert!(
            estimate >= file_bytes,
            "in‑memory estimate ({estimate}) should be ≥ on‑disk size ({file_bytes})"
        );
        assert!(
            file_bytes >= estimate / 2,
            "estimate is more than 2× the actual file — likely mis‑counting bits"
        );

        Ok(())
    }
}
