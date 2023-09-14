// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use crate::tables::{
    CheckpointEntry, EventEntry, MoveCallEntry, ObjectEntry, TransactionEntry,
    TransactionObjectEntry,
};
use anyhow::Result;
use sui_types::base_types::EpochId;

// Trait for writing entries to a temporary store (e.g. csv files).
// The entries are collected and written in batches.
// Eventually, they are uploaded to the database.
pub(crate) trait TableWriter: Send + Sync + 'static {
    fn write_checkpoints(&mut self, checkpoint_entries: &[CheckpointEntry]) -> Result<()>;
    fn write_transactions(&mut self, transaction_entries: &[TransactionEntry]) -> Result<()>;
    fn write_transaction_objects(
        &mut self,
        transaction_object_entries: &[TransactionObjectEntry],
    ) -> Result<()>;
    fn write_objects(&mut self, object_entries: &[ObjectEntry]) -> Result<()>;
    fn write_events(&mut self, event_entries: &[EventEntry]) -> Result<()>;
    fn write_move_calls(&mut self, move_call_entries: &[MoveCallEntry]) -> Result<()>;
    fn flush(&mut self) -> Result<()>;
    fn reset(&mut self, epoch_num: EpochId, checkpoint_seq_num: u64) -> Result<()>;
}

const INITIAL_CAPACITY: usize = 10_000;

// A writer for all entries related to a checkpoint.
// Collect all the entries and write them to temporary files in batches.
// Once trigger conditions are met (e.g. the file reaches a certain size,
// a number of entries are collected, a certain amount of time has passed, etc.)
// upload it to the database.
pub(crate) struct CheckpointWriter {
    checkpoint_entries: Vec<CheckpointEntry>,
    transaction_entries: Vec<TransactionEntry>,
    transaction_object_entries: Vec<TransactionObjectEntry>,
    object_entries: Vec<ObjectEntry>,
    event_entries: Vec<EventEntry>,
    move_call_entries: Vec<MoveCallEntry>,
}

impl CheckpointWriter {
    pub(crate) fn new() -> Self {
        Self {
            checkpoint_entries: Vec::with_capacity(INITIAL_CAPACITY),
            transaction_entries: Vec::with_capacity(INITIAL_CAPACITY),
            transaction_object_entries: Vec::with_capacity(INITIAL_CAPACITY),
            object_entries: Vec::with_capacity(INITIAL_CAPACITY),
            event_entries: Vec::with_capacity(INITIAL_CAPACITY),
            move_call_entries: Vec::with_capacity(INITIAL_CAPACITY),
        }
    }

    // Write all collected entries to files, via the given writer. Reset the entries after writing.
    pub(crate) fn write(&mut self, writer: &mut Box<dyn TableWriter>) -> Result<()> {
        writer.write_checkpoints(&self.checkpoint_entries)?;
        writer.write_transactions(&self.transaction_entries)?;
        writer.write_transaction_objects(&self.transaction_object_entries)?;
        writer.write_objects(&self.object_entries)?;
        writer.write_events(&self.event_entries)?;
        writer.write_move_calls(&self.move_call_entries)?;
        self.checkpoint_entries.clear();
        self.transaction_entries.clear();
        self.transaction_object_entries.clear();
        self.object_entries.clear();
        self.event_entries.clear();
        self.move_call_entries.clear();
        Ok(())
    }

    pub(crate) fn write_checkpoint(&mut self, entry: CheckpointEntry) {
        self.checkpoint_entries.push(entry);
    }

    pub(crate) fn write_transaction(&mut self, entry: TransactionEntry) {
        self.transaction_entries.push(entry);
    }

    pub(crate) fn write_transaction_object(&mut self, entry: TransactionObjectEntry) {
        self.transaction_object_entries.push(entry);
    }

    pub(crate) fn write_objects(&mut self, entry: ObjectEntry) {
        self.object_entries.push(entry);
    }

    pub(crate) fn write_events(&mut self, entry: EventEntry) {
        self.event_entries.push(entry);
    }

    pub(crate) fn write_move_calls(&mut self, entry: MoveCallEntry) {
        self.move_call_entries.push(entry);
    }
}
