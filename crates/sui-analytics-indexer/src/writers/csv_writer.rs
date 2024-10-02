// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::fs::{create_dir_all, remove_file};
use std::ops::Range;
use std::path::Path;
use std::{fs, fs::File, path::PathBuf};

use anyhow::{anyhow, Result};
use csv::{Writer, WriterBuilder};
use serde::Serialize;

use sui_storage::object_store::util::path_to_filesystem;
use sui_types::base_types::EpochId;

use crate::writers::AnalyticsWriter;
use crate::{FileFormat, FileType, ParquetSchema};

// Save table entries to csv files.
pub(crate) struct CSVWriter {
    root_dir_path: PathBuf,
    file_type: FileType,
    writer: Writer<File>,
    epoch: EpochId,
    checkpoint_range: Range<u64>,
}

impl CSVWriter {
    pub(crate) fn new(
        root_dir_path: &Path,
        file_type: FileType,
        start_checkpoint_seq_num: u64,
    ) -> Result<Self> {
        let checkpoint_range = start_checkpoint_seq_num..u64::MAX;
        let writer = Self::make_writer(
            root_dir_path.to_path_buf(),
            file_type,
            0,
            checkpoint_range.clone(),
        )?;
        Ok(CSVWriter {
            root_dir_path: root_dir_path.to_path_buf(),
            file_type,
            writer,
            epoch: 0,
            checkpoint_range,
        })
    }

    fn make_writer(
        root_dir_path: PathBuf,
        file_type: FileType,
        epoch_num: EpochId,
        checkpoint_range: Range<u64>,
    ) -> Result<Writer<File>> {
        let file_path = path_to_filesystem(
            root_dir_path,
            &file_type.file_path(FileFormat::CSV, epoch_num, checkpoint_range),
        )?;
        create_dir_all(file_path.parent().ok_or(anyhow!("Bad directory path"))?)?;
        if file_path.exists() {
            remove_file(&file_path)?;
        }
        let writer = WriterBuilder::new()
            .has_headers(false)
            .delimiter(b'|')
            .from_path(file_path)?;
        Ok(writer)
    }

    fn file_path(&self, epoch: EpochId, range: Range<u64>) -> Result<PathBuf> {
        path_to_filesystem(
            self.root_dir_path.clone(),
            &self.file_type.file_path(FileFormat::CSV, epoch, range),
        )
    }
}

impl<S: Serialize + ParquetSchema> AnalyticsWriter<S> for CSVWriter {
    fn file_format(&self) -> Result<FileFormat> {
        Ok(FileFormat::CSV)
    }

    fn write(&mut self, rows: &[S]) -> Result<()> {
        for row in rows {
            self.writer.serialize(row)?;
        }
        Ok(())
    }

    fn flush(&mut self, end_checkpoint_seq_num: u64) -> Result<bool> {
        self.writer.flush()?;
        let old_file_path = self.file_path(self.epoch, self.checkpoint_range.clone())?;
        let new_file_path = self.file_path(
            self.epoch,
            self.checkpoint_range.start..end_checkpoint_seq_num,
        )?;
        fs::rename(old_file_path, new_file_path)?;
        Ok(true)
    }

    fn reset(&mut self, epoch_num: EpochId, start_checkpoint_seq_num: u64) -> Result<()> {
        self.checkpoint_range.start = start_checkpoint_seq_num;
        self.checkpoint_range.end = u64::MAX;
        self.epoch = epoch_num;
        self.writer = CSVWriter::make_writer(
            self.root_dir_path.clone(),
            self.file_type,
            self.epoch,
            self.checkpoint_range.clone(),
        )?;
        Ok(())
    }

    fn file_size(&self) -> Result<Option<u64>> {
        let file_path = self.file_path(self.epoch, self.checkpoint_range.clone())?;
        let len = fs::metadata(file_path)?.len();
        Ok(Some(len))
    }
}
