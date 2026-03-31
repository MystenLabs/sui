// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Write-through store skeleton.

use std::io::Write;

use anyhow::{Error, Result};

use sui_types::{
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    object::Object,
    supported_protocol_versions::ProtocolConfig,
};

use crate::{
    CheckpointData, CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore,
    EpochStoreWriter, ObjectKey, ObjectStore, ObjectStoreWriter, SetupStore, StoreSummary,
    TransactionInfo, TransactionStore, TransactionStoreWriter,
};

/// Write-through cache over a primary and secondary store.
#[derive(Debug)]
pub struct WriteThroughStore<P, S> {
    primary: P,
    secondary: S,
}

impl<P, S> WriteThroughStore<P, S> {
    /// Create a new write-through store.
    pub fn new(primary: P, secondary: S) -> Self {
        Self { primary, secondary }
    }

    /// Return the primary layer.
    pub fn primary(&self) -> &P {
        &self.primary
    }

    /// Return the secondary layer.
    pub fn secondary(&self) -> &S {
        &self.secondary
    }
}

impl<P, S> TransactionStore for WriteThroughStore<P, S>
where
    P: TransactionStoreWriter,
    S: TransactionStore,
{
    fn transaction_data_and_effects(
        &self,
        _tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        todo!("write-through transaction reads are not implemented in the skeleton")
    }
}

impl<P, S> TransactionStoreWriter for WriteThroughStore<P, S>
where
    P: TransactionStoreWriter,
    S: TransactionStoreWriter,
{
    fn write_transaction(
        &self,
        _tx_digest: &str,
        _transaction_info: TransactionInfo,
    ) -> Result<(), Error> {
        todo!("write-through transaction writes are not implemented in the skeleton")
    }
}

impl<P, S> EpochStore for WriteThroughStore<P, S>
where
    P: EpochStoreWriter,
    S: EpochStore,
{
    fn epoch_info(&self, _epoch: u64) -> Result<Option<EpochData>, Error> {
        todo!("write-through epoch reads are not implemented in the skeleton")
    }

    fn protocol_config(&self, _epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        todo!("write-through protocol-config reads are not implemented in the skeleton")
    }
}

impl<P, S> EpochStoreWriter for WriteThroughStore<P, S>
where
    P: EpochStoreWriter,
    S: EpochStoreWriter,
{
    fn write_epoch_info(&self, _epoch: u64, _epoch_data: EpochData) -> Result<(), Error> {
        todo!("write-through epoch writes are not implemented in the skeleton")
    }
}

impl<P, S> ObjectStore for WriteThroughStore<P, S>
where
    P: ObjectStoreWriter,
    S: ObjectStore,
{
    fn get_objects(&self, _keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        todo!("write-through object reads are not implemented in the skeleton")
    }
}

impl<P, S> ObjectStoreWriter for WriteThroughStore<P, S>
where
    P: ObjectStoreWriter,
    S: ObjectStoreWriter,
{
    fn write_object(
        &self,
        _key: &ObjectKey,
        _object: Object,
        _actual_version: u64,
    ) -> Result<(), Error> {
        todo!("write-through object writes are not implemented in the skeleton")
    }
}

impl<P, S> CheckpointStore for WriteThroughStore<P, S>
where
    P: CheckpointStoreWriter,
    S: CheckpointStore,
{
    fn get_checkpoint_by_sequence_number(
        &self,
        _sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointData>, Error> {
        todo!("write-through checkpoint reads are not implemented in the skeleton")
    }

    fn get_latest_checkpoint(&self) -> Result<Option<CheckpointData>, Error> {
        todo!("write-through latest-checkpoint lookup is not implemented in the skeleton")
    }

    fn get_sequence_by_checkpoint_digest(
        &self,
        _digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("write-through checkpoint-digest lookups are not implemented in the skeleton")
    }

    fn get_sequence_by_contents_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("write-through contents-digest lookups are not implemented in the skeleton")
    }
}

impl<P, S> CheckpointStoreWriter for WriteThroughStore<P, S>
where
    P: CheckpointStoreWriter,
    S: CheckpointStoreWriter,
{
    fn write_checkpoint(&self, _checkpoint: &CheckpointData) -> Result<(), Error> {
        todo!("write-through checkpoint writes are not implemented in the skeleton")
    }
}

impl<P, S> SetupStore for WriteThroughStore<P, S>
where
    P: SetupStore,
{
    fn setup(&self, _chain_id: Option<String>) -> Result<Option<String>, Error> {
        todo!("write-through setup is not implemented in the skeleton")
    }
}

impl<P, S> StoreSummary for WriteThroughStore<P, S>
where
    P: StoreSummary,
    S: StoreSummary,
{
    fn summary<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(writer, "WriteThroughStore")?;
        self.primary.summary(writer)?;
        self.secondary.summary(writer)
    }
}
