// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use crate::{
    errors::AnalyticsIndexerError,
    tables::{CheckpointEntry, EventEntry, ObjectEntry, TransactionEntry, TransactionObjectEntry},
    writer::TableWriter,
    FileFormat, FileType,
};
use anyhow::{anyhow, Result};
use csv::{Writer, WriterBuilder};
use std::fs::{create_dir_all, remove_file};
use std::path::Path;
use std::{fs::File, path::PathBuf};

use crate::tables::MoveCallEntry;
use sui_storage::object_store::util::path_to_filesystem;
use sui_types::base_types::EpochId;

// Save table entries to csv files.
pub(crate) struct CSVWriter {
    root_dir_path: PathBuf,
    checkpoint_csv: Writer<File>,
    transaction_csv: Writer<File>,
    transaction_object_csv: Writer<File>,
    object_csv: Writer<File>,
    event_csv: Writer<File>,
    move_call_csv: Writer<File>,
}

impl CSVWriter {
    pub(crate) fn new(
        root_dir: &Path,
        epoch_num: EpochId,
        starting_checkpoint: u64,
    ) -> Result<Self, AnalyticsIndexerError> {
        Self::init(root_dir, epoch_num, starting_checkpoint)
            .map_err(|e| AnalyticsIndexerError::GenericError(e.to_string()))
    }

    fn init(
        root_dir_path: &Path,
        epoch_num: EpochId,
        checkpoint_seq_num: u64,
    ) -> Result<CSVWriter> {
        let transaction_object_csv = Self::make_writer(
            root_dir_path.to_path_buf(),
            FileType::TransactionObjects,
            epoch_num,
            checkpoint_seq_num,
        )?;
        let checkpoint_csv = Self::make_writer(
            root_dir_path.to_path_buf(),
            FileType::Checkpoint,
            epoch_num,
            checkpoint_seq_num,
        )?;
        let object_csv = Self::make_writer(
            root_dir_path.to_path_buf(),
            FileType::Object,
            epoch_num,
            checkpoint_seq_num,
        )?;
        let event_csv = Self::make_writer(
            root_dir_path.to_path_buf(),
            FileType::Event,
            epoch_num,
            checkpoint_seq_num,
        )?;
        let transaction_csv = Self::make_writer(
            root_dir_path.to_path_buf(),
            FileType::Transaction,
            epoch_num,
            checkpoint_seq_num,
        )?;
        let move_call_csv = Self::make_writer(
            root_dir_path.to_path_buf(),
            FileType::MoveCall,
            epoch_num,
            checkpoint_seq_num,
        )?;

        Ok(CSVWriter {
            root_dir_path: root_dir_path.to_path_buf(),
            checkpoint_csv,
            transaction_csv,
            transaction_object_csv,
            object_csv,
            event_csv,
            move_call_csv,
        })
    }

    fn make_writer(
        root_dir_path: PathBuf,
        file_type: FileType,
        epoch_num: EpochId,
        checkpoint_seq_num: u64,
    ) -> Result<Writer<File>> {
        let file_path = path_to_filesystem(
            root_dir_path,
            &file_type.file_path(FileFormat::CSV, epoch_num, checkpoint_seq_num),
        )?;
        create_dir_all(file_path.parent().ok_or(anyhow!("Bad directory path"))?)?;
        if file_path.exists() {
            remove_file(&file_path)?;
        }
        let writer = WriterBuilder::new()
            .has_headers(false)
            .from_path(file_path)?;
        Ok(writer)
    }
}

impl TableWriter for CSVWriter {
    fn write_checkpoints(&mut self, checkpoint_entries: &[CheckpointEntry]) -> Result<()> {
        for entry in checkpoint_entries {
            self.checkpoint_csv.serialize(entry)?;
        }
        Ok(())
    }

    fn write_transactions(&mut self, transaction_entries: &[TransactionEntry]) -> Result<()> {
        for entry in transaction_entries {
            self.transaction_csv.serialize(entry)?;
        }
        Ok(())
    }

    fn write_transaction_objects(
        &mut self,
        transaction_object_entries: &[TransactionObjectEntry],
    ) -> Result<()> {
        for entry in transaction_object_entries {
            self.transaction_object_csv.serialize(entry)?;
        }
        Ok(())
    }

    fn write_objects(&mut self, object_entries: &[ObjectEntry]) -> Result<()> {
        for entry in object_entries {
            self.object_csv.serialize(entry)?;
        }
        Ok(())
    }

    fn write_events(&mut self, event_entries: &[EventEntry]) -> Result<()> {
        for entry in event_entries {
            self.event_csv.serialize(entry)?;
        }
        Ok(())
    }

    fn write_move_calls(&mut self, move_call_entries: &[MoveCallEntry]) -> Result<()> {
        for entry in move_call_entries {
            self.move_call_csv.serialize(entry)?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.checkpoint_csv.flush()?;
        self.object_csv.flush()?;
        self.transaction_csv.flush()?;
        self.transaction_object_csv.flush()?;
        self.event_csv.flush()?;
        self.move_call_csv.flush()?;
        Ok(())
    }

    fn reset(&mut self, epoch_num: EpochId, checkpoint_seq_num: u64) -> Result<()> {
        let new_csv_writer = CSVWriter::init(&self.root_dir_path, epoch_num, checkpoint_seq_num)?;
        self.checkpoint_csv = new_csv_writer.checkpoint_csv;
        self.object_csv = new_csv_writer.object_csv;
        self.transaction_csv = new_csv_writer.transaction_csv;
        self.event_csv = new_csv_writer.event_csv;
        self.transaction_object_csv = new_csv_writer.transaction_object_csv;
        self.move_call_csv = new_csv_writer.move_call_csv;
        Ok(())
    }
}
