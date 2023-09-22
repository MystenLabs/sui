// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, RwLock};

use simulacrum::Simulacrum;
use sui_types::storage::{CheckpointStore, EventStore, ObjectStore2, Store, TransactionStore};
use tap::Pipe;

#[derive(Clone)]
pub struct SharedSimulacrum {
    inner: Arc<RwLock<Simulacrum>>,
}

impl SharedSimulacrum {
    pub fn inner(&self) -> std::sync::RwLockReadGuard<'_, Simulacrum> {
        self.inner.read().unwrap()
    }

    pub fn inner_mut(&self) -> std::sync::RwLockWriteGuard<'_, Simulacrum> {
        self.inner.write().unwrap()
    }
}

impl From<Simulacrum> for SharedSimulacrum {
    fn from(value: Simulacrum) -> Self {
        Self {
            inner: Arc::new(RwLock::new(value)),
        }
    }
}

impl Store for SharedSimulacrum {
    type Error = std::convert::Infallible;
}

impl CheckpointStore for SharedSimulacrum {
    fn get_latest_checkpoint(
        &self,
    ) -> Result<sui_types::messages_checkpoint::VerifiedCheckpoint, Self::Error> {
        self.inner()
            .store()
            .get_highest_checkpint()
            .expect("should always have 1 checkpoint")
            .to_owned()
            .pipe(Ok)
    }

    fn get_checkpoint_by_digest(
        &self,
        digest: &sui_types::messages_checkpoint::CheckpointDigest,
    ) -> Result<Option<sui_types::messages_checkpoint::VerifiedCheckpoint>, Self::Error> {
        self.inner()
            .store()
            .get_checkpoint_by_digest(digest)
            .cloned()
            .pipe(Ok)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: sui_types::messages_checkpoint::CheckpointSequenceNumber,
    ) -> Result<Option<sui_types::messages_checkpoint::VerifiedCheckpoint>, Self::Error> {
        self.inner()
            .store()
            .get_checkpoint_by_sequence_number(sequence_number)
            .cloned()
            .pipe(Ok)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &sui_types::messages_checkpoint::CheckpointContentsDigest,
    ) -> Result<Option<sui_types::messages_checkpoint::CheckpointContents>, Self::Error> {
        self.inner()
            .store()
            .get_checkpoint_contents(digest)
            .cloned()
            .pipe(Ok)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: sui_types::messages_checkpoint::CheckpointSequenceNumber,
    ) -> Result<Option<sui_types::messages_checkpoint::CheckpointContents>, Self::Error> {
        let inner = self.inner();
        let store = inner.store();
        store
            .get_checkpoint_by_sequence_number(sequence_number)
            .and_then(|checkpoint| store.get_checkpoint_contents(&checkpoint.content_digest))
            .cloned()
            .pipe(Ok)
    }
}

impl TransactionStore for SharedSimulacrum {
    fn get_transaction(
        &self,
        tx_digest: &sui_types::digests::TransactionDigest,
    ) -> Result<Option<sui_types::transaction::VerifiedTransaction>, Self::Error> {
        self.inner()
            .store()
            .get_transaction(tx_digest)
            .cloned()
            .pipe(Ok)
    }

    fn get_transaction_effects(
        &self,
        tx_digest: &sui_types::digests::TransactionDigest,
    ) -> Result<Option<sui_types::effects::TransactionEffects>, Self::Error> {
        self.inner()
            .store()
            .get_transaction_effects(tx_digest)
            .cloned()
            .pipe(Ok)
    }
}

impl EventStore for SharedSimulacrum {
    fn get_events(
        &self,
        event_digest: &sui_types::digests::TransactionEventsDigest,
    ) -> Result<Option<sui_types::effects::TransactionEvents>, Self::Error> {
        self.inner()
            .store()
            .get_transaction_events(event_digest)
            .cloned()
            .pipe(Ok)
    }
}

impl ObjectStore2 for SharedSimulacrum {
    fn get_object(
        &self,
        object_id: &sui_types::base_types::ObjectID,
    ) -> Result<Option<sui_types::object::Object>, Self::Error> {
        self.inner().store().get_object(object_id).cloned().pipe(Ok)
    }

    fn get_object_by_key(
        &self,
        object_id: &sui_types::base_types::ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Result<Option<sui_types::object::Object>, Self::Error> {
        self.inner()
            .store()
            .get_object_at_version(object_id, version)
            .cloned()
            .pipe(Ok)
    }
}
