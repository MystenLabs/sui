// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Read-through store skeleton with live object caching.

use std::io::Write;

use anyhow::{Error, Result};
use sui_types::{
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    object::Object,
    supported_protocol_versions::ProtocolConfig,
};

use crate::{
    CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore, EpochStoreWriter,
    FullCheckpointData, ObjectKey, ObjectStore, ObjectStoreWriter, SetupStore, StoreSummary,
    TransactionInfo, TransactionStore, TransactionStoreWriter,
};

/// Read-through cache over a primary and secondary store.
#[derive(Debug)]
pub struct ReadThroughStore<P, S> {
    primary: P,
    secondary: S,
}

impl<P, S> ReadThroughStore<P, S> {
    /// Create a new read-through store.
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

impl<P, S> TransactionStore for ReadThroughStore<P, S>
where
    P: TransactionStoreWriter,
    S: TransactionStore,
{
    fn transaction_data_and_effects(
        &self,
        _tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        todo!("read-through transaction reads are not implemented in the PR2 slice")
    }
}

impl<P, S> TransactionStoreWriter for ReadThroughStore<P, S>
where
    P: TransactionStoreWriter,
    S: TransactionStore,
{
    fn write_transaction(
        &self,
        _tx_digest: &str,
        _transaction_info: TransactionInfo,
    ) -> Result<(), Error> {
        todo!("read-through transaction writes are not implemented in the PR2 slice")
    }
}

impl<P, S> EpochStore for ReadThroughStore<P, S>
where
    P: EpochStoreWriter,
    S: EpochStore,
{
    fn epoch_info(&self, _epoch: u64) -> Result<Option<EpochData>, Error> {
        todo!("read-through epoch reads are not implemented in the PR2 slice")
    }

    fn protocol_config(&self, _epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        todo!("read-through protocol-config reads are not implemented in the PR2 slice")
    }
}

impl<P, S> EpochStoreWriter for ReadThroughStore<P, S>
where
    P: EpochStoreWriter,
    S: EpochStore,
{
    fn write_epoch_info(&self, _epoch: u64, _epoch_data: EpochData) -> Result<(), Error> {
        todo!("read-through epoch writes are not implemented in the PR2 slice")
    }
}

impl<P, S> ObjectStore for ReadThroughStore<P, S>
where
    P: ObjectStoreWriter,
    S: ObjectStore,
{
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        let cached_objects = self.primary.get_objects(keys)?;

        let mut keys_to_fetch = Vec::new();
        let mut missing_indexes = Vec::new();
        for (index, object) in cached_objects.iter().enumerate() {
            if object.is_none() {
                keys_to_fetch.push(keys[index].clone());
                missing_indexes.push(index);
            }
        }

        let mut objects = cached_objects;
        if !keys_to_fetch.is_empty() {
            let fetched_objects = self.secondary.get_objects(&keys_to_fetch)?;

            assert_eq!(missing_indexes.len(), keys_to_fetch.len());
            assert_eq!(fetched_objects.len(), keys_to_fetch.len());

            for ((index, key), fetched) in missing_indexes
                .iter()
                .zip(keys_to_fetch.iter())
                .zip(fetched_objects.iter())
            {
                if let Some((object, actual_version)) = fetched {
                    self.primary
                        .write_object(key, object.clone(), *actual_version)?;
                    objects[*index] = Some((object.clone(), *actual_version));
                }
            }
        }

        Ok(objects)
    }
}

impl<P, S> ObjectStoreWriter for ReadThroughStore<P, S>
where
    P: ObjectStoreWriter,
    S: ObjectStore,
{
    fn write_object(
        &self,
        key: &ObjectKey,
        object: Object,
        actual_version: u64,
    ) -> Result<(), Error> {
        self.primary.write_object(key, object, actual_version)
    }
}

impl<P, S> CheckpointStore for ReadThroughStore<P, S>
where
    P: CheckpointStoreWriter,
    S: CheckpointStore,
{
    fn get_checkpoint_by_sequence_number(
        &self,
        _sequence: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointData>, Error> {
        todo!("read-through checkpoint reads are not implemented in the PR2 slice")
    }

    fn get_latest_checkpoint(&self) -> Result<Option<FullCheckpointData>, Error> {
        todo!("read-through latest-checkpoint lookups are not implemented in the PR2 slice")
    }

    fn get_sequence_by_checkpoint_digest(
        &self,
        _digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("read-through checkpoint-digest lookups are not implemented in the PR2 slice")
    }

    fn get_sequence_by_contents_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("read-through contents-digest lookups are not implemented in the PR2 slice")
    }
}

impl<P, S> CheckpointStoreWriter for ReadThroughStore<P, S>
where
    P: CheckpointStoreWriter,
    S: CheckpointStore,
{
    fn write_checkpoint(&self, _checkpoint: &FullCheckpointData) -> Result<(), Error> {
        todo!("read-through checkpoint writes are not implemented in the PR2 slice")
    }
}

impl<P, S> SetupStore for ReadThroughStore<P, S>
where
    P: SetupStore,
    S: SetupStore,
{
    fn setup(&self, chain_id: Option<String>) -> Result<Option<String>, Error> {
        let resolved_chain_id = self.secondary.setup(chain_id.clone())?.or(chain_id);
        self.primary.setup(resolved_chain_id.clone())?;
        Ok(resolved_chain_id)
    }
}

impl<P, S> StoreSummary for ReadThroughStore<P, S>
where
    P: StoreSummary,
    S: StoreSummary,
{
    fn summary<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(writer, "ReadThroughStore")?;
        self.primary.summary(writer)?;
        self.secondary.summary(writer)
    }
}
