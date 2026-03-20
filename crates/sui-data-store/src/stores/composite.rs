// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore, EpochStoreWriter,
    FullCheckpointData, ObjectKey, ObjectStore, ObjectStoreWriter, StoreSummary, TransactionInfo,
    TransactionStore, TransactionStoreWriter,
};
use anyhow::{Error, Result};
use sui_types::{
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    object::Object,
    supported_protocol_versions::ProtocolConfig,
};

/// A store that routes each capability to a different backing chain.
pub struct CompositeStore<Tx, Epoch, Obj, Ckpt> {
    transactions: Tx,
    epochs: Epoch,
    objects: Obj,
    checkpoints: Ckpt,
}

impl<Tx, Epoch, Obj, Ckpt> CompositeStore<Tx, Epoch, Obj, Ckpt> {
    /// Create a new composite store.
    pub fn new(transactions: Tx, epochs: Epoch, objects: Obj, checkpoints: Ckpt) -> Self {
        Self {
            transactions,
            epochs,
            objects,
            checkpoints,
        }
    }
}

impl<Tx, Epoch, Obj, Ckpt> TransactionStore for CompositeStore<Tx, Epoch, Obj, Ckpt>
where
    Tx: TransactionStore,
{
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        self.transactions.transaction_data_and_effects(tx_digest)
    }
}

impl<Tx, Epoch, Obj, Ckpt> TransactionStoreWriter for CompositeStore<Tx, Epoch, Obj, Ckpt>
where
    Tx: TransactionStoreWriter,
{
    fn write_transaction(
        &self,
        tx_digest: &str,
        transaction_info: TransactionInfo,
    ) -> Result<(), Error> {
        self.transactions
            .write_transaction(tx_digest, transaction_info)
    }
}

impl<Tx, Epoch, Obj, Ckpt> EpochStore for CompositeStore<Tx, Epoch, Obj, Ckpt>
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

impl<Tx, Epoch, Obj, Ckpt> EpochStoreWriter for CompositeStore<Tx, Epoch, Obj, Ckpt>
where
    Epoch: EpochStoreWriter,
{
    fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), Error> {
        self.epochs.write_epoch_info(epoch, epoch_data)
    }
}

impl<Tx, Epoch, Obj, Ckpt> ObjectStore for CompositeStore<Tx, Epoch, Obj, Ckpt>
where
    Obj: ObjectStore,
{
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        self.objects.get_objects(keys)
    }
}

impl<Tx, Epoch, Obj, Ckpt> ObjectStoreWriter for CompositeStore<Tx, Epoch, Obj, Ckpt>
where
    Obj: ObjectStoreWriter,
{
    fn write_object(
        &self,
        key: &ObjectKey,
        object: Object,
        actual_version: u64,
    ) -> Result<(), Error> {
        self.objects.write_object(key, object, actual_version)
    }
}

impl<Tx, Epoch, Obj, Ckpt> CheckpointStore for CompositeStore<Tx, Epoch, Obj, Ckpt>
where
    Ckpt: CheckpointStore,
{
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointData>, Error> {
        self.checkpoints.get_checkpoint_by_sequence_number(sequence)
    }

    fn get_latest_checkpoint(&self) -> Result<Option<FullCheckpointData>, Error> {
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

impl<Tx, Epoch, Obj, Ckpt> CheckpointStoreWriter for CompositeStore<Tx, Epoch, Obj, Ckpt>
where
    Ckpt: CheckpointStoreWriter,
{
    fn write_checkpoint(&self, checkpoint: &FullCheckpointData) -> Result<(), Error> {
        self.checkpoints.write_checkpoint(checkpoint)
    }
}

impl<Tx, Epoch, Obj, Ckpt> StoreSummary for CompositeStore<Tx, Epoch, Obj, Ckpt>
where
    Tx: StoreSummary,
    Epoch: StoreSummary,
    Obj: StoreSummary,
    Ckpt: StoreSummary,
{
    fn summary<W: std::io::Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(writer, "CompositeStore summary")?;
        writeln!(writer, "Transactions:")?;
        self.transactions.summary(writer)?;
        writeln!(writer, "Epochs:")?;
        self.epochs.summary(writer)?;
        writeln!(writer, "Objects:")?;
        self.objects.summary(writer)?;
        writeln!(writer, "Checkpoints:")?;
        self.checkpoints.summary(writer)
    }
}
