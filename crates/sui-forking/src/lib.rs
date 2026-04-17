// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Building blocks for the experimental `sui-forking` tool.

#![allow(unused)]

pub(crate) mod context;
pub(crate) mod filesystem;
mod gql;
mod node;
mod rpc;
pub mod startup;
pub mod store;

pub use gql::GraphQLClient;
pub use node::Node;
pub use store::DataStore;

use anyhow::{Error, Result};

use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::supported_protocol_versions::ProtocolConfig;
use sui_types::transaction::VerifiedTransaction;

// ============================================================================
// Read traits
// ============================================================================

/// Signed transaction envelope paired with its execution effects and the checkpoint
/// it was finalized in. The checkpoint is used by [`crate::store::DataStore`] as a
/// pre-fork guard: remote results whose `checkpoint > forked_at_checkpoint` must not
/// leak into a fork that has already diverged from the upstream chain.
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

/// Query for an object.
/// Specifies an `ObjectID` and the "rule" to retrieve it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ObjectKey {
    pub(crate) object_id: ObjectID,
    pub(crate) version_query: VersionQuery,
}

/// Query options for an object.
/// `Version` request an object at a specific version, or latest if no version is provided
/// `RootVersion` request an object at a given version at most (<=)
/// `AtCheckpoint` request an object at a given checkpoint. Useful for unknown `Version`.
/// `VersionAtCheckpoint` requests an exact version, but only if it existed by the given checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum VersionQuery {
    Version(u64),
    RootVersion(u64),
    AtCheckpoint(u64),
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
