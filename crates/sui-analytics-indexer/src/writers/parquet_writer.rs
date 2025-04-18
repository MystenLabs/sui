// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{AnalyticsWriter, FileFormat, FileType};
use crate::{ParquetSchema, ParquetValue};
use anyhow::{anyhow, Result};
use arrow_array::RecordBatch;
use serde::Serialize;
use std::fs::File;
use std::fs::{create_dir_all, remove_file};
use std::ops::Range;
use std::path::{Path, PathBuf};
use sui_types::base_types::EpochId;
use tracing::{info, debug};
use once_cell::sync::Lazy;

use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use sui_storage::object_store::util::path_to_filesystem;

/// Environment variable to override the default maximum string size in bytes.
/// This is used to prevent Arrow byte array offset overflow errors.
const MAX_STRING_SIZE_VAR_NAME: &str = "SUI_ANALYTICS_MAX_STRING_SIZE";

/// Default maximum string size in bytes (16MB)
const DEFAULT_MAX_STRING_SIZE: usize = 16 * 1024 * 1024;

/// Maximum string size in bytes for serialization to Parquet.
/// 
/// If the environment variable `SUI_ANALYTICS_MAX_STRING_SIZE` is unset, 
/// we default to `DEFAULT_MAX_STRING_SIZE` which is 16MB.
/// 
/// This is read only once and after that the value is cached.
static MAX_STRING_SIZE: Lazy<usize> = Lazy::new(|| {
    let max_size_opt = std::env::var(MAX_STRING_SIZE_VAR_NAME)
        .ok()
        .and_then(|s| s.parse().ok());
    if let Some(max_size) = max_size_opt {
        info!(
            "Using custom value for '{}': {} bytes",
            MAX_STRING_SIZE_VAR_NAME, max_size
        );
        max_size
    } else {
        info!(
            "Using default value for '{}': {} bytes",
            MAX_STRING_SIZE_VAR_NAME, DEFAULT_MAX_STRING_SIZE
        );
        DEFAULT_MAX_STRING_SIZE
    }
});

/// Replaces a string that exceeds the maximum size limit with a placeholder message.
/// This ensures that potential JSON content remains valid JSON.
fn handle_oversized_string(s: &str) -> String {
    if s.len() <= *MAX_STRING_SIZE {
        s.to_string()
    } else {
        // Use a simple placeholder that would be valid in JSON
        "String exceeds maximum allowed size".to_string()
    }
}

// Arrow buffer size constants to prevent byte array offset overflow
const MAX_ARROW_BYTES: usize = i32::MAX as usize;          // 2,147,483,647
const SAFETY_MARGIN: usize = 10 * 1024 * 1024;            // 10 MB
const BATCH_LIMIT: usize = MAX_ARROW_BYTES - SAFETY_MARGIN;

// Need this for string conversion
use arrow_array::{ArrayRef, BooleanArray, Int64Array, StringArray, UInt64Array};
use std::sync::Arc;

// Macro to convert ParquetValue columns into Arrow arrays
macro_rules! convert_to_arrow_array {
    ($column:ident, $target_vector:ident, $($variant:path => $types:ty),*) => {{
        use anyhow::anyhow;

        // Check for empty column vec to prevent index out of bounds errors.
        if $column.is_empty() {
            tracing::error!("Empty column data encountered");
            return Err(anyhow!("Empty column data"));
        }

        // Match the variant of the first row
        match &$column[0] {
            // Handle OptionStr separately - it needs special handling for null values
            ParquetValue::OptionStr(_) => {
                let mut values = Vec::with_capacity($column.len());

                for (i, val) in $column.into_iter().enumerate() {
                    if let ParquetValue::OptionStr(v) = val {
                        // Handle oversized string values
                        values.push(v.map(|s| {
                            if s.len() > *MAX_STRING_SIZE {
                                handle_oversized_string(&s)
                            } else {
                                s
                            }
                        }));
                    } else {
                        // Found a type mismatch
                        let error_msg = format!(
                            "Type mismatch in column at row {}: expected OptionStr, got {:?}",
                            i, val
                        );
                        tracing::error!("{}", error_msg);
                        return Err(anyhow!(error_msg));
                    }
                }

                let array = StringArray::from(values);
                $target_vector.push(Arc::new(array) as ArrayRef);
            },

            // Handle OptionU64 separately - it needs special handling for null values
            ParquetValue::OptionU64(_) => {
                let mut values = Vec::with_capacity($column.len());

                for (i, val) in $column.into_iter().enumerate() {
                    if let ParquetValue::OptionU64(v) = val {
                        values.push(v);
                    } else {
                        // Found a type mismatch
                        let error_msg = format!(
                            "Type mismatch in column at row {}: expected OptionU64, got {:?}",
                            i, val
                        );
                        tracing::error!("{}", error_msg);
                        return Err(anyhow!(error_msg));
                    }
                }

                let array = UInt64Array::from(values);
                $target_vector.push(Arc::new(array) as ArrayRef);
            },

            // Handle Str separately to apply truncation
            ParquetValue::Str(_) => {
                let mut values = Vec::with_capacity($column.len());

                for (i, val) in $column.into_iter().enumerate() {
                    if let ParquetValue::Str(v) = val {
                        // Handle oversized string values
                        if v.len() > *MAX_STRING_SIZE {
                            values.push(handle_oversized_string(&v));
                        } else {
                            values.push(v);
                        }
                    } else {
                        // Found a type mismatch
                        let error_msg = format!(
                            "Type mismatch in column at row {}: expected Str, got {:?}",
                            i, val
                        );
                        tracing::error!("{}", error_msg);
                        return Err(anyhow!(error_msg));
                    }
                }

                let array = StringArray::from(values);
                $target_vector.push(Arc::new(array) as ArrayRef);
            },

            // Process remaining variants using the standard pattern
            $(
                $variant(_) => {
                    // Convert and validate in a single pass
                    let mut values = Vec::with_capacity($column.len());

                    for (i, val) in $column.into_iter().enumerate() {
                        if let $variant(v) = val {
                            values.push(v);
                        } else {
                            // Found a type mismatch
                            let error_msg = format!(
                                "Type mismatch in column at row {}: expected {}, got {:?}",
                                i,
                                stringify!($variant),
                                val
                            );
                            tracing::error!("{}", error_msg);
                            return Err(anyhow!(error_msg));
                        }
                    }

                    let array = <$types>::from(values);
                    $target_vector.push(Arc::new(array) as ArrayRef);
                }
            )*
        }
    }};
}

// Save table entries to parquet files.
pub(crate) struct ParquetWriter {
    root_dir_path: PathBuf,
    file_type: FileType,
    epoch: EpochId,
    checkpoint_range: Range<u64>,
    data: Vec<Vec<ParquetValue>>,
    // Track an estimate of the memory used by each column to avoid Arrow buffer overflow
    column_size_estimates: Vec<usize>,
}

impl ParquetWriter {
    // Estimate the size of a single value in bytes based on its type
    fn estimate_value_size(value: &ParquetValue) -> usize {
        match value {
            // Strings: 4-byte offset + payload
            ParquetValue::Str(s)               => s.len() + 4,
            ParquetValue::OptionStr(Some(s))   => s.len() + 4,
            ParquetValue::OptionStr(None)      => 4,  // offset still written

            // 64-bit ints
            ParquetValue::U64(_)               | 
            ParquetValue::I64(_)               => 8,

            ParquetValue::OptionU64(Some(_))   => 8,  // value slot + validity bit
            ParquetValue::OptionU64(None)      => 8,

            // Booleans (safe upper bound)
            ParquetValue::Bool(_)              => 1,
        }
    }
    
    pub(crate) fn new(
        root_dir_path: &Path,
        file_type: FileType,
        start_checkpoint_seq_num: u64,
    ) -> Result<Self> {
        let checkpoint_range = start_checkpoint_seq_num..u64::MAX;
        Ok(Self {
            root_dir_path: root_dir_path.to_path_buf(),
            file_type,
            epoch: 0,
            checkpoint_range,
            data: vec![],
            column_size_estimates: vec![],
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
    
    // Check if any column is approaching Arrow's buffer limit and flush if needed
    fn maybe_flush<S: Serialize + ParquetSchema>(&mut self) -> Result<()> {
        if self.column_size_estimates.iter().any(|&b| b >= BATCH_LIMIT) {
            debug!(
                "Flushing current batch due to large column(s): {}MB of {}MB limit",
                self.column_size_estimates.iter().max().unwrap_or(&0) / (1024 * 1024),
                BATCH_LIMIT / (1024 * 1024)
            );
            self.flush_current_batch::<S>()?;
            self.column_size_estimates.fill(0);
        }
        Ok(())
    }
    
    // Flushes the current batch of data to a Parquet file without updating checkpoint range
    fn flush_current_batch<S: Serialize + ParquetSchema>(&mut self) -> Result<bool> {
        if self.data.is_empty() {
            return Ok(false);
        }
        
        let schema_len = S::schema().len();
        let mut batch_data = vec![];
        for column in std::mem::take(&mut self.data) {
            convert_to_arrow_array!(column, batch_data,
                ParquetValue::U64 => UInt64Array, ParquetValue::Bool => BooleanArray, ParquetValue::I64 => Int64Array
            );
        }
        
        let batch = RecordBatch::try_from_iter(S::schema().iter().zip(batch_data.into_iter()))?;
        
        let properties = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build();
        
        let mut writer = ArrowWriter::try_new(self.file()?, batch.schema(), Some(properties))?;
        writer.write(&batch)?;
        writer.close()?;
        
        // Reset data but keep column structure
        self.data = vec![];
        for _ in 0..schema_len {
            self.data.push(vec![]);
        }
        
        Ok(true)
    }
}

impl<S: Serialize + ParquetSchema> AnalyticsWriter<S> for ParquetWriter {
    fn file_format(&self) -> Result<FileFormat> {
        Ok(FileFormat::PARQUET)
    }

    fn write(&mut self, rows: &[S]) -> Result<()> {
        // Ensure column_size_estimates has the right size
        if self.column_size_estimates.len() < S::schema().len() {
            self.column_size_estimates.resize(S::schema().len(), 0);
        }
        
        for row in rows {
            for col_idx in 0..S::schema().len() {
                if col_idx == self.data.len() {
                    self.data.push(vec![]);
                }
                
                let value = row.get_column(col_idx);
                
                // Update size estimate for this column
                self.column_size_estimates[col_idx] += Self::estimate_value_size(&value);
                
                // Add the value to the data
                self.data[col_idx].push(value);
            }
            
            // Check if we need to flush after adding each row
            self.maybe_flush::<S>()?;
        }
        Ok(())
    }

    fn flush(&mut self, end_checkpoint_seq_num: u64) -> Result<bool> {
        if self.data.is_empty() {
            return Ok(false);
        }
        
        // Update checkpoint range
        self.checkpoint_range.end = end_checkpoint_seq_num;
        
        // Flush any remaining data
        let result = self.flush_current_batch::<S>()?;
        
        // Reset size estimates
        self.column_size_estimates.fill(0);
        
        Ok(result)
    }

    fn reset(&mut self, epoch_num: EpochId, start_checkpoint_seq_num: u64) -> Result<()> {
        self.checkpoint_range.start = start_checkpoint_seq_num;
        self.checkpoint_range.end = u64::MAX;
        self.epoch = epoch_num;
        self.data = vec![];
        self.column_size_estimates = vec![];
        Ok(())
    }

    fn file_size(&self) -> Result<Option<u64>> {
        // parquet writer doesn't write records in a temp staging file
        // and only flushes records after serializing and compressing them
        // when flush is invoked
        Ok(None)
    }
}
