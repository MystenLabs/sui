// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Building blocks for the experimental `sui-fork` tool.

pub mod cli;
pub(crate) mod context;
mod gql;
pub(crate) mod ingestion;
pub(crate) mod local_store;
pub(crate) mod metadata;
mod node;
pub(crate) mod pending;
mod proto;
pub(crate) mod remote;
mod rpc;
pub(crate) mod seed;
pub(crate) mod services;
pub mod startup;
pub mod store;
#[cfg(test)]
#[path = "tests/support.rs"]
mod test_support;

pub use gql::GraphQLClient;
pub use node::Node;
pub use proto::forking::AdvanceCheckpointRequest;
pub use proto::forking::AdvanceClockRequest;
pub use proto::forking::GetStatusRequest;
pub use proto::forking::forking_service_client::ForkingServiceClient;
pub use seed::SeedInput;
pub use store::ForkStore;

use anyhow::Error;
use anyhow::Result;

use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::transaction::VerifiedTransaction;

// ============================================================================
// Read traits
// ============================================================================

/// Signed transaction envelope paired with its execution effects and the checkpoint it was
/// finalized in. The checkpoint is used by [`crate::store::ForkStore`] as a pre-fork guard,
/// because remote results whose `checkpoint > forked_at_checkpoint` must not leak into a fork that
/// has already diverged from the upstream chain.
#[derive(Clone, Debug)]
pub(crate) struct TransactionInfo {
    pub(crate) transaction: VerifiedTransaction,
    pub(crate) effects: TransactionEffects,
    pub(crate) checkpoint: CheckpointSequenceNumber,
}

/// `TransactionRead` trait is used to retrieve transaction data for a given digest.
pub(crate) trait TransactionRead {
    /// Given a transaction digest, return the signed transaction, its effects, and the
    /// checkpoint it was finalized in. Returns `None` if the transaction is not found.
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error>;
}

/// Query for an object, specifying an `ObjectID` and the rule to retrieve it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ObjectKey {
    pub(crate) object_id: ObjectID,
    pub(crate) version_query: VersionQuery,
}

/// Version rule for an object query.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum VersionQuery {
    /// Request the highest version of the object at or below the given version.
    RootVersion(u64),

    /// Request the object as of the given checkpoint. Useful when the version is unknown.
    AtCheckpoint(u64),

    /// Request an exact version, but only if it existed by the given checkpoint.
    VersionAtCheckpoint { version: u64, checkpoint: u64 },
}

/// The `ObjectRead` trait is used to retrieve objects by their keys, with different query options.
pub(crate) trait ObjectRead {
    /// Retrieve objects by their keys, with different query options.
    ///
    /// If the object is not found, the element in the vector is `None`.
    /// Otherwise each tuple contains:
    /// - `Object`: The object data
    /// - `u64`: The actual version of the object
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error>;
}

/// Checkpoint read data.
pub(crate) trait CheckpointRead {
    /// Return the verified checkpoint summary together with its decoded
    /// contents. If `sequence` is `None`, return the latest checkpoint.
    fn get_checkpoint(
        &self,
        sequence: Option<CheckpointSequenceNumber>,
    ) -> Result<Option<(VerifiedCheckpoint, CheckpointContents)>, Error>;
}
