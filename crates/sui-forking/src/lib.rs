// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Building blocks for the experimental `sui-forking` tool.

#![allow(unused)]

pub(crate) mod filesystem;
mod gql;
pub(crate) mod node;
pub(crate) mod store;

pub(crate) use gql::GraphQLClient;
pub(crate) use node::Node;

use anyhow::{Error, Result};

use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::messages_checkpoint::{CheckpointSequenceNumber, VerifiedCheckpoint};
use sui_types::object::Object;
use sui_types::supported_protocol_versions::ProtocolConfig;
use sui_types::transaction::TransactionData;

// ============================================================================
// Read traits
// ============================================================================

/// Transaction data with effects and checkpoint.
#[derive(Clone, Debug)]
pub(crate) struct TransactionInfo {
    pub(crate) data: TransactionData,
    pub(crate) effects: TransactionEffects,
    pub(crate) checkpoint: u64,
}

/// A `TransactionStore` has to be able to retrieve transaction data for a given digest.
/// The data provided to `sui_execution::executor::Executor::execute_transaction_to_effects`
/// must be available. Some of that data is not provided by the user. It is naturally available
/// at runtime on a live system and later saved in effects and in the context of a checkpoint.
pub(crate) trait TransactionRead {
    /// Given a transaction digest, return transaction info including data, effects,
    /// and the checkpoint that transaction was executed in.
    /// Returns `None` if the transaction is not found.
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error>;
}

/// Epoch data.
#[derive(Clone, Debug)]
pub(crate) struct EpochData {
    pub(crate) epoch_id: u64,
    pub(crate) protocol_version: u64,
    pub(crate) rgp: u64,
    pub(crate) start_timestamp: u64,
}

/// An `EpochStore` retrieves the epoch data and protocol configuration
/// for a given epoch.
/// Epoch data is collected by an indexer and it is not stored anywhere otherwise.
/// This is a very small amount of information and could conceivably be saved locally
/// and never hit a server.
pub(crate) trait EpochRead {
    /// Return the `EpochData` for a given epoch.
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, Error>;
    /// Return the `ProtocolConfig` for a given epoch.
    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, Error>;
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

/// The `ObjectStore` trait is used to retrieve objects by their keys,
/// with different query options.
///
/// This trait can execute a subset of what is allowed by
/// `crates/sui-indexer-alt-graphql/schema.graphql::multiGetObjects`.
/// That query likely allows more than what most clients need, which is fairly limited in
/// its usage.
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
    /// Return the verified checkpoint data. If `sequence` is `None`, return the latest checkpoint.
    fn get_verified_checkpoint(
        &self,
        sequence: Option<CheckpointSequenceNumber>,
    ) -> Result<Option<VerifiedCheckpoint>, Error>;
}
