// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use crate::{
    errors::AnalyticsIndexerError,
    tables::{CheckpointEntry, EventEntry, ObjectEntry, TransactionEntry, TransactionObjectEntry},
    writer::TableWriter,
    AnalyticsIndexerConfig,
};
use csv::Writer;
use std::{env, fs::File, path::PathBuf};

// Save table entries to csv files.
pub(crate) struct CSVWriter {
    checkpoint_csv: Writer<File>,
    transaction_csv: Writer<File>,
    transaction_object_csv: Writer<File>,
    object_csv: Writer<File>,
    event_csv: Writer<File>,
    move_call_csv: Writer<File>,
}
use crate::tables::MoveCallEntry;
use tracing::info;

impl CSVWriter {
    pub(crate) fn new(
        config: &AnalyticsIndexerConfig,
        starting_checkpoint: u64,
    ) -> Result<Self, AnalyticsIndexerError> {
        let dir = if let Some(checkpoint_dir) = &config.checkpoint_dir {
            PathBuf::from(checkpoint_dir)
        } else {
            env::current_dir().map_err(|_| AnalyticsIndexerError::CurrentDirError)?
        };
        let dir = dir.as_path();
        let checkpoint = dir.join(format!("checkpoint_{}.csv", starting_checkpoint));
        let transaction = dir.join(format!("transaction_{}.csv", starting_checkpoint));
        let transaction_object =
            dir.join(format!("transaction_object_{}.csv", starting_checkpoint));
        let object_csv = dir.join(format!("object_{}.csv", starting_checkpoint));
        let event_csv = dir.join(format!("event_{}.csv", starting_checkpoint));
        let move_call_csv = dir.join(format!("move_call_{}.csv", starting_checkpoint));
        Ok(Self {
            checkpoint_csv: Writer::from_path(checkpoint).unwrap(),
            transaction_csv: Writer::from_path(transaction).unwrap(),
            transaction_object_csv: Writer::from_path(transaction_object).unwrap(),
            object_csv: Writer::from_path(object_csv).unwrap(),
            event_csv: Writer::from_path(event_csv).unwrap(),
            move_call_csv: Writer::from_path(move_call_csv).unwrap(),
        })
    }
}

impl TableWriter for CSVWriter {
    fn write_checkpoints(&mut self, checkpoint_entries: &[CheckpointEntry]) {
        info!("Write checkpoints");
        for entry in checkpoint_entries {
            self.checkpoint_csv.serialize(entry).unwrap();
        }
        self.checkpoint_csv.flush().unwrap();
    }

    fn write_transactions(&mut self, transaction_entries: &[TransactionEntry]) {
        info!("Write transactions");
        for entry in transaction_entries {
            self.transaction_csv.serialize(entry).unwrap();
        }
        self.transaction_csv.flush().unwrap();
    }

    fn write_transaction_objects(&mut self, transaction_object_entries: &[TransactionObjectEntry]) {
        info!("Write transaction objects");
        for entry in transaction_object_entries {
            self.transaction_object_csv.serialize(entry).unwrap();
        }
        self.transaction_object_csv.flush().unwrap();
    }

    fn write_objects(&mut self, object_entries: &[ObjectEntry]) {
        info!("Write objects");
        for entry in object_entries {
            self.object_csv.serialize(entry).unwrap();
        }
        self.object_csv.flush().unwrap();
    }

    fn write_events(&mut self, event_entries: &[EventEntry]) {
        info!("Write events");
        for entry in event_entries {
            self.event_csv.serialize(entry).unwrap();
        }
        self.event_csv.flush().unwrap();
    }

    fn write_move_calls(&mut self, move_call_entries: &[MoveCallEntry]) {
        info!("Write move calls");
        for entry in move_call_entries {
            self.move_call_csv.serialize(entry).unwrap();
        }
        self.move_call_csv.flush().unwrap();
    }
}
