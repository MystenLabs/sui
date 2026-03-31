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

/// A router that delegates each capability to a dedicated store.
#[derive(Debug)]
pub struct ForkingStore<Tx, Epoch, Obj, Ckpt> {
    transactions: Tx,
    epochs: Epoch,
    objects: Obj,
    checkpoints: Ckpt,
}

impl<Tx, Epoch, Obj, Ckpt> ForkingStore<Tx, Epoch, Obj, Ckpt> {
    /// Create a new forking store.
    pub fn new(transactions: Tx, epochs: Epoch, objects: Obj, checkpoints: Ckpt) -> Self {
        Self {
            transactions,
            epochs,
            objects,
            checkpoints,
        }
    }

    /// Return the transaction store.
    pub fn transactions(&self) -> &Tx {
        &self.transactions
    }

    /// Return the epoch store.
    pub fn epochs(&self) -> &Epoch {
        &self.epochs
    }

    /// Return the object store.
    pub fn objects(&self) -> &Obj {
        &self.objects
    }

    /// Return the checkpoint store.
    pub fn checkpoints(&self) -> &Ckpt {
        &self.checkpoints
    }
}

impl<Tx, Epoch, Obj, Ckpt> EpochStore for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Epoch: EpochStore,
{
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, Error> {
        self.epochs.epoch_info(epoch)
    }

    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        self.epochs.protocol_config(epoch)
    }
}

impl<Tx, Epoch, Obj, Ckpt> EpochStoreWriter for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Epoch: EpochStoreWriter,
{
    fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), Error> {
        self.epochs.write_epoch_info(epoch, epoch_data)
    }
}

impl<Tx, Epoch, Obj, Ckpt> CheckpointStore for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Ckpt: CheckpointStore,
{
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointData>, Error> {
        self.checkpoints.get_checkpoint_by_sequence_number(sequence)
    }

    fn get_latest_checkpoint(&self) -> Result<Option<CheckpointData>, Error> {
        self.checkpoints.get_latest_checkpoint()
    }

    fn get_sequence_by_checkpoint_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        self.checkpoints.get_sequence_by_checkpoint_digest(digest)
    }

    fn get_sequence_by_contents_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        self.checkpoints.get_sequence_by_contents_digest(digest)
    }
}

impl<Tx, Epoch, Obj, Ckpt> CheckpointStoreWriter for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Ckpt: CheckpointStoreWriter,
{
    fn write_checkpoint(&self, checkpoint: &CheckpointData) -> Result<(), Error> {
        self.checkpoints.write_checkpoint(checkpoint)
    }
}

impl<Tx, Epoch, Obj, Ckpt> SetupStore for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Tx: SetupStore,
    Epoch: SetupStore,
    Obj: SetupStore,
    Ckpt: SetupStore,
{
    fn setup(&self, chain_id: Option<String>) -> Result<Option<String>, Error> {
        let transaction_chain_id = self.transactions.setup(chain_id.clone())?;
        let epoch_chain_id = self.epochs.setup(chain_id.clone())?;
        let object_chain_id = self.objects.setup(chain_id.clone())?;
        let checkpoint_chain_id = self.checkpoints.setup(chain_id)?;

        Ok(checkpoint_chain_id
            .or(object_chain_id)
            .or(epoch_chain_id)
            .or(transaction_chain_id))
    }
}

impl<Tx, Epoch, Obj, Ckpt> StoreSummary for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Tx: StoreSummary,
    Epoch: StoreSummary,
    Obj: StoreSummary,
    Ckpt: StoreSummary,
{
    fn summary<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(writer, "ForkingStore")?;
        self.transactions.summary(writer)?;
        self.epochs.summary(writer)?;
        self.objects.summary(writer)?;
        self.checkpoints.summary(writer)
    }
}
