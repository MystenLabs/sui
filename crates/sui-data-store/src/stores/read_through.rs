// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore, EpochStoreWriter,
    FullCheckpointData, ObjectKey, ObjectStore, ObjectStoreWriter, SetupStore, StoreSummary,
    TransactionInfo, TransactionStore, TransactionStoreWriter,
};
use anyhow::{Error, Result, anyhow};
use sui_types::{
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    object::Object,
    supported_protocol_versions::ProtocolConfig,
};

/// A read-through store that composes a primary cache and a secondary source.
///
/// Reads consult the primary first, fall back to the secondary on miss, and cache
/// successful secondary results back into the primary.
/// Direct writes only update the primary.
pub struct ReadThroughStore<P, S> {
    primary: P,
    secondary: S,
}

impl<P, S> ReadThroughStore<P, S> {
    /// Create a new read-through store.
    pub fn new(primary: P, secondary: S) -> Self {
        Self { primary, secondary }
    }

    fn cache_checkpoint_by_sequence(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointData>, Error>
    where
        P: CheckpointStoreWriter,
        S: CheckpointStore,
    {
        let Some(checkpoint) = self.secondary.get_checkpoint_by_sequence_number(sequence)? else {
            return Ok(None);
        };
        self.primary.write_checkpoint(&checkpoint)?;
        Ok(Some(checkpoint))
    }

    fn cache_checkpoint_for_checkpoint_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error>
    where
        P: CheckpointStoreWriter,
        S: CheckpointStore,
    {
        let Some(sequence) = self.secondary.get_sequence_by_checkpoint_digest(digest)? else {
            return Ok(None);
        };
        self.cache_checkpoint_by_sequence(sequence)?.ok_or_else(|| {
            anyhow!(
                "secondary store resolved checkpoint digest {digest} to sequence {sequence}, but the checkpoint payload was missing"
            )
        })?;
        Ok(Some(sequence))
    }

    fn cache_checkpoint_for_contents_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error>
    where
        P: CheckpointStoreWriter,
        S: CheckpointStore,
    {
        let Some(sequence) = self.secondary.get_sequence_by_contents_digest(digest)? else {
            return Ok(None);
        };
        self.cache_checkpoint_by_sequence(sequence)?.ok_or_else(|| {
            anyhow!(
                "secondary store resolved checkpoint contents digest {digest} to sequence {sequence}, but the checkpoint payload was missing"
            )
        })?;
        Ok(Some(sequence))
    }
}

impl<P, S> TransactionStore for ReadThroughStore<P, S>
where
    P: TransactionStoreWriter,
    S: TransactionStore,
{
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        match self.primary.transaction_data_and_effects(tx_digest)? {
            Some(transaction_info) => Ok(Some(transaction_info)),
            None => self
                .secondary
                .transaction_data_and_effects(tx_digest)?
                .map_or(Ok(None), |info| {
                    self.primary.write_transaction(tx_digest, info.clone())?;
                    Ok(Some(info))
                }),
        }
    }
}

impl<P, S> TransactionStoreWriter for ReadThroughStore<P, S>
where
    P: TransactionStoreWriter,
    S: TransactionStore,
{
    fn write_transaction(
        &self,
        tx_digest: &str,
        transaction_info: TransactionInfo,
    ) -> Result<(), Error> {
        self.primary.write_transaction(tx_digest, transaction_info)
    }
}

impl<P, S> EpochStore for ReadThroughStore<P, S>
where
    P: EpochStoreWriter,
    S: EpochStore,
{
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, Error> {
        match self.primary.epoch_info(epoch)? {
            Some(epoch_data) => Ok(Some(epoch_data)),
            None => match self.secondary.epoch_info(epoch)? {
                Some(epoch_data) => {
                    self.primary.write_epoch_info(epoch, epoch_data.clone())?;
                    Ok(Some(epoch_data))
                }
                None => Ok(None),
            },
        }
    }

    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        match self.primary.protocol_config(epoch)? {
            Some(config) => Ok(Some(config)),
            None => self.secondary.protocol_config(epoch),
        }
    }
}

impl<P, S> EpochStoreWriter for ReadThroughStore<P, S>
where
    P: EpochStoreWriter,
    S: EpochStore,
{
    fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), Error> {
        self.primary.write_epoch_info(epoch, epoch_data)
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
        let mut none_object_idx = Vec::new();
        for (i, object) in cached_objects.iter().enumerate() {
            if object.is_none() {
                keys_to_fetch.push(keys[i].clone());
                none_object_idx.push(i);
            }
        }

        let mut objects = cached_objects;
        if !keys_to_fetch.is_empty() {
            let fetched_objects = self.secondary.get_objects(&keys_to_fetch)?;

            assert_eq!(none_object_idx.len(), keys_to_fetch.len());
            assert_eq!(fetched_objects.len(), keys_to_fetch.len());

            for ((idx, key), fetched) in none_object_idx
                .iter()
                .zip(keys_to_fetch.iter())
                .zip(fetched_objects.iter())
            {
                if let Some((object, actual_version)) = fetched {
                    self.primary
                        .write_object(key, object.clone(), *actual_version)?;
                    objects[*idx] = Some((object.clone(), *actual_version));
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
        sequence: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointData>, Error> {
        match self.primary.get_checkpoint_by_sequence_number(sequence)? {
            Some(checkpoint) => Ok(Some(checkpoint)),
            None => self.cache_checkpoint_by_sequence(sequence),
        }
    }

    fn get_latest_checkpoint(&self) -> Result<Option<FullCheckpointData>, Error> {
        match self.primary.get_latest_checkpoint()? {
            Some(checkpoint) => Ok(Some(checkpoint)),
            None => match self.secondary.get_latest_checkpoint()? {
                Some(checkpoint) => {
                    self.primary.write_checkpoint(&checkpoint)?;
                    Ok(Some(checkpoint))
                }
                None => Ok(None),
            },
        }
    }

    fn get_sequence_by_checkpoint_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        match self.primary.get_sequence_by_checkpoint_digest(digest)? {
            Some(sequence) => Ok(Some(sequence)),
            None => self.cache_checkpoint_for_checkpoint_digest(digest),
        }
    }

    fn get_sequence_by_contents_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        match self.primary.get_sequence_by_contents_digest(digest)? {
            Some(sequence) => Ok(Some(sequence)),
            None => self.cache_checkpoint_for_contents_digest(digest),
        }
    }
}

impl<P, S> CheckpointStoreWriter for ReadThroughStore<P, S>
where
    P: CheckpointStoreWriter,
    S: CheckpointStore,
{
    fn write_checkpoint(&self, checkpoint: &FullCheckpointData) -> Result<(), Error> {
        self.primary.write_checkpoint(checkpoint)
    }
}

impl<P, S> StoreSummary for ReadThroughStore<P, S>
where
    P: StoreSummary,
    S: StoreSummary,
{
    fn summary<W: std::io::Write>(&self, writer: &mut W) -> Result<()> {
        self.primary.summary(writer)?;
        self.secondary.summary(writer)
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
