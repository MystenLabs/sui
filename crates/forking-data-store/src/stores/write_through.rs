// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;

use anyhow::{Error, Result};
use sui_types::{
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    supported_protocol_versions::ProtocolConfig,
};

use crate::{
    CheckpointData, CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore,
    EpochStoreWriter, SetupStore, StoreSummary,
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

impl<P, S> EpochStore for WriteThroughStore<P, S>
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

impl<P, S> EpochStoreWriter for WriteThroughStore<P, S>
where
    P: EpochStoreWriter,
    S: EpochStoreWriter,
{
    fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), Error> {
        self.secondary.write_epoch_info(epoch, epoch_data.clone())?;
        self.primary.write_epoch_info(epoch, epoch_data)
    }
}

impl<P, S> CheckpointStore for WriteThroughStore<P, S>
where
    P: CheckpointStoreWriter,
    S: CheckpointStore,
{
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointData>, Error> {
        match self.primary.get_checkpoint_by_sequence_number(sequence)? {
            Some(checkpoint) => Ok(Some(checkpoint)),
            None => match self.secondary.get_checkpoint_by_sequence_number(sequence)? {
                Some(checkpoint) => {
                    self.primary.write_checkpoint(&checkpoint)?;
                    Ok(Some(checkpoint))
                }
                None => Ok(None),
            },
        }
    }

    fn get_latest_checkpoint(&self) -> Result<Option<CheckpointData>, Error> {
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
            None => self.secondary.get_sequence_by_checkpoint_digest(digest),
        }
    }

    fn get_sequence_by_contents_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        match self.primary.get_sequence_by_contents_digest(digest)? {
            Some(sequence) => Ok(Some(sequence)),
            None => self.secondary.get_sequence_by_contents_digest(digest),
        }
    }
}

impl<P, S> CheckpointStoreWriter for WriteThroughStore<P, S>
where
    P: CheckpointStoreWriter,
    S: CheckpointStoreWriter,
{
    fn write_checkpoint(&self, checkpoint: &CheckpointData) -> Result<(), Error> {
        self.secondary.write_checkpoint(checkpoint)?;
        self.primary.write_checkpoint(checkpoint)
    }
}

impl<P, S> SetupStore for WriteThroughStore<P, S>
where
    P: SetupStore,
    S: SetupStore,
{
    fn setup(&self, chain_id: Option<String>) -> Result<Option<String>, Error> {
        let resolved_chain_id = self.secondary.setup(chain_id)?;
        self.primary.setup(resolved_chain_id.clone())?;
        Ok(resolved_chain_id)
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
