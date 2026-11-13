// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! An in-memory [`RpcStateReader`] for handler tests: a node without rpc indexes that holds a
//! fixed set of checkpoints, transactions and objects.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sui_types::base_types::{EpochId, ObjectID, SequenceNumber};
use sui_types::committee::Committee;
use sui_types::digests::{
    ChainIdentifier, CheckpointContentsDigest, CheckpointDigest, TransactionDigest,
};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use sui_types::error::SuiResult;
use sui_types::full_checkpoint_content::{Checkpoint, ExecutedTransaction};
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, VerifiedCheckpoint,
    VersionedFullCheckpointContents,
};
use sui_types::object::Object;
use sui_types::storage::error::{Error as StorageError, Result};
use sui_types::storage::{
    BackingPackageStore, ObjectKey, ObjectStore, PackageObject, ReadStore, RpcIndexes,
    RpcStateReader, RuntimeObjectResolver,
};
use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
use sui_types::transaction::{Transaction, VerifiedTransaction};

use crate::RpcService;

/// A node without rpc indexes. Its lowest available checkpoint, for checkpoint and object data
/// alike, is `lowest_available_checkpoint`, which the store does not enforce: it serves whatever
/// `checkpoints` and `objects` hold, as a node does while pruning catches up with its floor.
pub(crate) struct StubStore {
    /// Every stored version of every object. An object's latest version is its highest one.
    pub objects: Vec<Object>,
    /// Every stored checkpoint, with its transactions. The latest checkpoint is the highest one.
    pub checkpoints: Vec<Checkpoint>,
    pub lowest_available_checkpoint: CheckpointSequenceNumber,
    /// If set, the lowest available checkpoint becomes this once a request has read object
    /// versions by key, the last read of a transaction or checkpoint read whose mask needs object
    /// versions, as when pruning advances while the request runs.
    pub pruned_to_during_reads: Option<CheckpointSequenceNumber>,
    /// Whether object versions have been read by key.
    pub objects_read: AtomicBool,
    /// See [`RpcStateReader::retains_object_version_history`].
    pub retains_object_version_history: bool,
}

impl StubStore {
    /// A node that keeps every object version from genesis and stores nothing.
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            checkpoints: Vec::new(),
            lowest_available_checkpoint: 0,
            pruned_to_during_reads: None,
            objects_read: AtomicBool::new(false),
            retains_object_version_history: true,
        }
    }

    fn lowest_available(&self) -> CheckpointSequenceNumber {
        match self.pruned_to_during_reads {
            Some(pruned_to) if self.objects_read.load(Ordering::SeqCst) => pruned_to,
            _ => self.lowest_available_checkpoint,
        }
    }

    /// A store holding `checkpoints` and every object version in their object sets.
    pub fn with_checkpoints(checkpoints: Vec<Checkpoint>) -> Self {
        let objects = checkpoints
            .iter()
            .flat_map(|checkpoint| checkpoint.object_set.iter().cloned())
            .collect();
        Self {
            objects,
            checkpoints,
            ..Self::new()
        }
    }

    /// Checkpoints 0 and 1, one transaction each: checkpoint 0's creates owned object 1, and
    /// checkpoint 1's mutates it, reading the version checkpoint 0 wrote.
    pub fn two_checkpoints() -> Vec<Checkpoint> {
        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(1)
            .finish_transaction();
        let first = builder.build_checkpoint();
        let mut builder = builder
            .start_transaction(0)
            .mutate_owned_object(1)
            .finish_transaction();
        let second = builder.build_checkpoint();
        vec![first, second]
    }

    pub fn into_service(self) -> RpcService {
        RpcService::new(Arc::new(self))
    }

    fn checkpoint(&self, sequence_number: CheckpointSequenceNumber) -> Option<&Checkpoint> {
        self.checkpoints
            .iter()
            .find(|checkpoint| checkpoint.summary.sequence_number == sequence_number)
    }

    /// The transaction `digest`, and the checkpoint it is in.
    fn transaction(
        &self,
        digest: &TransactionDigest,
    ) -> Option<(&Checkpoint, &ExecutedTransaction)> {
        self.checkpoints.iter().find_map(|checkpoint| {
            checkpoint
                .transactions
                .iter()
                .find(|transaction| transaction.effects.transaction_digest() == digest)
                .map(|transaction| (checkpoint, transaction))
        })
    }
}

impl ObjectStore for StubStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.objects
            .iter()
            .filter(|object| object.id() == *object_id)
            .max_by_key(|object| object.version())
            .cloned()
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.objects
            .iter()
            .find(|object| object.id() == *object_id && object.version() == version)
            .cloned()
    }

    fn multi_get_objects_by_key(&self, object_keys: &[ObjectKey]) -> Vec<Option<Object>> {
        self.objects_read.store(true, Ordering::SeqCst);
        object_keys
            .iter()
            .map(|key| self.get_object_by_key(&key.0, key.1))
            .collect()
    }
}

impl BackingPackageStore for StubStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        Ok(self
            .get_object(package_id)
            .filter(|object| object.is_package())
            .map(PackageObject::new))
    }
}

impl RuntimeObjectResolver for StubStore {
    fn read_child_object(
        &self,
        _parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self
            .objects
            .iter()
            .filter(|object| object.id() == *child && object.version() <= child_version_upper_bound)
            .max_by_key(|object| object.version())
            .cloned())
    }

    fn get_object_received_at_version(
        &self,
        _owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        Ok(self.get_object_by_key(receiving_object_id, receive_object_at_version))
    }
}

impl ReadStore for StubStore {
    fn get_committee(&self, _epoch: EpochId) -> Option<Arc<Committee>> {
        None
    }

    fn get_latest_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        self.checkpoints
            .iter()
            .max_by_key(|checkpoint| checkpoint.summary.sequence_number)
            .map(|checkpoint| VerifiedCheckpoint::new_unchecked(checkpoint.summary.clone()))
            .ok_or_else(|| StorageError::missing("the stub store has no checkpoints"))
    }

    fn get_highest_verified_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        self.get_latest_checkpoint()
    }

    fn get_highest_synced_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        self.get_latest_checkpoint()
    }

    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber> {
        Ok(self.lowest_available())
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.checkpoints
            .iter()
            .find(|checkpoint| checkpoint.summary.digest() == digest)
            .map(|checkpoint| VerifiedCheckpoint::new_unchecked(checkpoint.summary.clone()))
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.checkpoint(sequence_number)
            .map(|checkpoint| VerifiedCheckpoint::new_unchecked(checkpoint.summary.clone()))
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.checkpoints
            .iter()
            .find(|checkpoint| checkpoint.contents.digest() == digest)
            .map(|checkpoint| checkpoint.contents.clone())
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        self.checkpoint(sequence_number)
            .map(|checkpoint| checkpoint.contents.clone())
    }

    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        self.transaction(tx_digest).map(|(_, transaction)| {
            Arc::new(VerifiedTransaction::new_unchecked(
                Transaction::from_generic_sig_data(
                    transaction.transaction.clone(),
                    transaction.signatures.clone(),
                ),
            ))
        })
    }

    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.transaction(tx_digest)
            .map(|(_, transaction)| transaction.effects.clone())
    }

    fn get_events(&self, event_digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.transaction(event_digest)
            .and_then(|(_, transaction)| transaction.events.clone())
    }

    fn get_unchanged_loaded_runtime_objects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<Vec<ObjectKey>> {
        self.transaction(digest)
            .map(|(_, transaction)| transaction.unchanged_loaded_runtime_objects.clone())
    }

    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        self.transaction(digest)
            .map(|(checkpoint, _)| checkpoint.summary.sequence_number)
    }

    fn get_full_checkpoint_contents(
        &self,
        _sequence_number: Option<CheckpointSequenceNumber>,
        _digest: &CheckpointContentsDigest,
    ) -> Option<VersionedFullCheckpointContents> {
        None
    }
}

impl RpcStateReader for StubStore {
    fn get_lowest_available_checkpoint_objects(&self) -> Result<CheckpointSequenceNumber> {
        Ok(self.lowest_available())
    }

    fn retains_object_version_history(&self) -> bool {
        self.retains_object_version_history
    }

    fn get_chain_identifier(&self) -> Result<ChainIdentifier> {
        Ok(CheckpointDigest::new([0; 32]).into())
    }

    fn indexes(&self) -> Option<&dyn RpcIndexes> {
        None
    }
}
