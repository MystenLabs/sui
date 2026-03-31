// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
    EpochStoreWriter, ObjectKey, ObjectStore, ObjectStoreWriter, StoreSummary, TransactionInfo,
    TransactionStore, TransactionStoreWriter,
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

impl<Tx, Epoch, Obj, Ckpt> TransactionStore for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Tx: TransactionStore,
{
    fn transaction_data_and_effects(
        &self,
        _tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        todo!("forking transaction routing is not implemented in the skeleton")
    }
}

impl<Tx, Epoch, Obj, Ckpt> TransactionStoreWriter for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Tx: TransactionStoreWriter,
{
    fn write_transaction(
        &self,
        _tx_digest: &str,
        _transaction_info: TransactionInfo,
    ) -> Result<(), Error> {
        todo!("forking transaction writes are not implemented in the skeleton")
    }
}

impl<Tx, Epoch, Obj, Ckpt> EpochStore for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Epoch: EpochStore,
{
    fn epoch_info(&self, _epoch: u64) -> Result<Option<EpochData>, Error> {
        todo!("forking epoch routing is not implemented in the skeleton")
    }

    fn protocol_config(&self, _epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        todo!("forking protocol-config routing is not implemented in the skeleton")
    }
}

impl<Tx, Epoch, Obj, Ckpt> EpochStoreWriter for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Epoch: EpochStoreWriter,
{
    fn write_epoch_info(&self, _epoch: u64, _epoch_data: EpochData) -> Result<(), Error> {
        todo!("forking epoch writes are not implemented in the skeleton")
    }
}

impl<Tx, Epoch, Obj, Ckpt> ObjectStore for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Obj: ObjectStore,
{
    fn get_objects(&self, _keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        todo!("forking object routing is not implemented in the skeleton")
    }
}

impl<Tx, Epoch, Obj, Ckpt> ObjectStoreWriter for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Obj: ObjectStoreWriter,
{
    fn write_object(
        &self,
        _key: &ObjectKey,
        _object: Object,
        _actual_version: u64,
    ) -> Result<(), Error> {
        todo!("forking object writes are not implemented in the skeleton")
    }
}

impl<Tx, Epoch, Obj, Ckpt> CheckpointStore for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Ckpt: CheckpointStore,
{
    fn get_checkpoint_by_sequence_number(
        &self,
        _sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointData>, Error> {
        todo!("forking checkpoint routing is not implemented in the skeleton")
    }

    fn get_latest_checkpoint(&self) -> Result<Option<CheckpointData>, Error> {
        todo!("forking latest-checkpoint routing is not implemented in the skeleton")
    }

    fn get_sequence_by_checkpoint_digest(
        &self,
        _digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("forking checkpoint-digest routing is not implemented in the skeleton")
    }

    fn get_sequence_by_contents_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("forking contents-digest routing is not implemented in the skeleton")
    }
}

impl<Tx, Epoch, Obj, Ckpt> CheckpointStoreWriter for ForkingStore<Tx, Epoch, Obj, Ckpt>
where
    Ckpt: CheckpointStoreWriter,
{
    fn write_checkpoint(&self, _checkpoint: &CheckpointData) -> Result<(), Error> {
        todo!("forking checkpoint writes are not implemented in the skeleton")
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
