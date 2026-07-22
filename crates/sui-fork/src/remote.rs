// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Checkpoint-pinned remote access to the forked-from chain.
//!
//! [`RemoteSource`] owns every GraphQL round-trip the store makes and the
//! fork's remote-read policy: object queries are pinned at the fork
//! checkpoint, and checkpoint/transaction lookups refuse to return anything
//! finalized after the fork point, so upstream activity that happened after
//! the fork cannot leak into a fork that has already diverged.

use anyhow::Context as _;
use anyhow::anyhow;
use anyhow::bail;
use itertools::Itertools as _;
use tracing::debug;

use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;

use crate::CheckpointRead;
use crate::GraphQLClient;
use crate::ObjectKey;
use crate::ObjectRead;
use crate::TransactionInfo;
use crate::TransactionRead;
use crate::VersionQuery;
use crate::gql::AddressOwnedObject;
use crate::gql::InventoryObject;

/// Read access to the forked-from chain, pinned at the fork checkpoint.
#[derive(Clone)]
pub(crate) struct RemoteSource {
    gql: GraphQLClient,
    forked_at_checkpoint: CheckpointSequenceNumber,
}

impl RemoteSource {
    pub(crate) fn new(gql: GraphQLClient, forked_at_checkpoint: CheckpointSequenceNumber) -> Self {
        Self {
            gql,
            forked_at_checkpoint,
        }
    }

    /// The underlying GraphQL client, for callers with their own query needs
    /// (seed resolution runs its own checkpoint-scoped queries).
    pub(crate) fn gql(&self) -> &GraphQLClient {
        &self.gql
    }

    /// The checkpoint this source is pinned at.
    pub(crate) fn forked_at_checkpoint(&self) -> CheckpointSequenceNumber {
        self.forked_at_checkpoint
    }

    /// Latest version of an object as of the fork checkpoint.
    pub(crate) fn latest_object(&self, object_id: &ObjectID) -> anyhow::Result<Option<Object>> {
        self.object_query(
            object_id,
            VersionQuery::AtCheckpoint(self.forked_at_checkpoint),
        )
    }

    /// Exact object version, only if it existed by the fork checkpoint.
    pub(crate) fn object_at_version(
        &self,
        object_id: &ObjectID,
        version: u64,
    ) -> anyhow::Result<Option<Object>> {
        self.object_query(
            object_id,
            VersionQuery::VersionAtCheckpoint {
                version,
                checkpoint: self.forked_at_checkpoint,
            },
        )
    }

    /// Highest object version at or below `version_bound` (bounded child reads).
    pub(crate) fn object_at_or_before(
        &self,
        object_id: &ObjectID,
        version_bound: u64,
    ) -> anyhow::Result<Option<Object>> {
        self.object_query(object_id, VersionQuery::RootVersion(version_bound))
    }

    fn object_query(
        &self,
        object_id: &ObjectID,
        version_query: VersionQuery,
    ) -> anyhow::Result<Option<Object>> {
        let objects = self.gql.get_objects(&[ObjectKey {
            object_id: *object_id,
            version_query,
        }])?;
        Ok(objects
            .into_iter()
            .next()
            .flatten()
            .map(|(object, _)| object))
    }

    /// Checkpoint summary and contents by sequence number.
    ///
    /// Post-fork sequences return `None` without a remote round-trip: the fork
    /// has diverged, so upstream checkpoints after the fork point are not part
    /// of this chain.
    pub(crate) fn checkpoint(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<(VerifiedCheckpoint, CheckpointContents)>> {
        if sequence > self.forked_at_checkpoint {
            debug!(
                "Checkpoint requested for sequence {sequence} > forked_at_checkpoint {}, returning None",
                self.forked_at_checkpoint
            );
            return Ok(None);
        }
        self.gql.get_checkpoint(Some(sequence))
    }

    /// Transaction, effects, and finalizing checkpoint by digest.
    ///
    /// Transaction digests are not ordered, so post-fork requests cannot be
    /// rejected up front the way sequence-keyed checkpoint reads are. Instead
    /// the finalizing checkpoint on the remote response is checked, and
    /// anything executed strictly after the fork point is dropped.
    pub(crate) fn transaction(
        &self,
        digest: &TransactionDigest,
    ) -> anyhow::Result<Option<TransactionInfo>> {
        let Some(info) = self
            .gql
            .transaction_data_and_effects(&digest.base58_encode())?
        else {
            return Ok(None);
        };
        if info.checkpoint > self.forked_at_checkpoint {
            return Ok(None);
        }
        Ok(Some(info))
    }

    /// Events for a transaction that is known to have emitted some.
    pub(crate) fn transaction_events(
        &self,
        digest: &TransactionDigest,
    ) -> anyhow::Result<TransactionEvents> {
        self.gql
            .get_transaction_events(&digest.base58_encode())
            .with_context(|| format!("failed to fetch transaction events for {digest}"))?
            .ok_or_else(|| anyhow!("transaction {digest} events not found on remote"))
    }

    /// Fetch exact `(id, version)` objects at the fork checkpoint, validating
    /// that every response matches the requested reference.
    pub(crate) fn objects_at_fork(
        &self,
        object_refs: &[ObjectRef],
        description: &str,
    ) -> anyhow::Result<Vec<Object>> {
        let keys: Vec<_> = object_refs
            .iter()
            .map(|object_ref| ObjectKey {
                object_id: object_ref.0,
                version_query: VersionQuery::VersionAtCheckpoint {
                    version: object_ref.1.value(),
                    checkpoint: self.forked_at_checkpoint,
                },
            })
            .collect();
        let objects = self
            .gql
            .get_objects(&keys)
            .with_context(|| format!("failed to fetch {description}"))?;

        let mut fetched = Vec::with_capacity(object_refs.len());
        for (object_ref, object) in object_refs.iter().zip_eq(objects) {
            let Some((object, _)) = object else {
                bail!(
                    "{description} object {} version {} was not found at fork checkpoint {}",
                    object_ref.0,
                    object_ref.1.value(),
                    self.forked_at_checkpoint,
                );
            };
            if object.compute_object_reference() != *object_ref {
                bail!(
                    "{description} object {} metadata does not match fetched object at fork checkpoint {}",
                    object_ref.0,
                    self.forked_at_checkpoint,
                );
            }
            fetched.push(object);
        }

        Ok(fetched)
    }

    /// Full enumeration of the objects owned by `owner` at the fork checkpoint.
    pub(crate) fn scan_address_owned(
        &self,
        owner: SuiAddress,
    ) -> anyhow::Result<Vec<AddressOwnedObject>> {
        self.gql
            .get_address_owned_objects_at_checkpoint_blocking(owner, self.forked_at_checkpoint)
    }

    /// Full enumeration of the objects owned by object `parent` at the fork checkpoint.
    pub(crate) fn scan_object_owned(
        &self,
        parent: ObjectID,
    ) -> anyhow::Result<Vec<InventoryObject>> {
        self.gql
            .get_object_owned_objects_at_checkpoint_blocking(parent, self.forked_at_checkpoint)
    }

    /// Full enumeration of the objects matching `type_filter` at the fork checkpoint.
    pub(crate) fn scan_by_type(&self, type_filter: String) -> anyhow::Result<Vec<InventoryObject>> {
        self.gql
            .get_objects_by_type_at_checkpoint_blocking(type_filter, self.forked_at_checkpoint)
    }

    /// Lowest checkpoint for which the remote retains checkpoint and transaction data.
    pub(crate) fn lowest_available_checkpoint(&self) -> anyhow::Result<CheckpointSequenceNumber> {
        self.gql.get_lowest_available_checkpoint()
    }

    /// Lowest checkpoint for which the remote retains object data.
    pub(crate) fn lowest_available_checkpoint_objects(
        &self,
    ) -> anyhow::Result<CheckpointSequenceNumber> {
        self.gql.get_lowest_available_checkpoint_objects()
    }
}
