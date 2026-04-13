// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Multi-tier caching data store for Sui blockchain data.
//!
//! This crate provides a flexible data store abstraction for retrieving
//! Sui blockchain data (transactions, checkpooints, epochs, objects). The stores are loosely
//! modeled after the GQL schema in `crates/sui-indexer-alt-graphql/schema.graphql`.
//!
//! ## Core Traits
//!
//! - [`TransactionStore`] - Retrieve transaction data and effects by digest
//! - [`CheckpointStore`] - Retrieve verified checkpoint by sequence number
//! - [`EpochStore`] - Retrieve epoch information and protocol configuration
//! - [`ObjectStore`] - Retrieve objects by their keys with flexible version queries
//!
//! ## Store Implementations
//!
//! - [`stores::GraphQLStore`] - Remote GraphQL-backed store (mainnet/testnet/devnet/custom GraphQL
//!   endpoint)
//!

mod gql_queries;
pub mod node;
pub mod stores;

pub use node::Node;

use anyhow::{Error, Result};

use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::messages_checkpoint::{CheckpointSequenceNumber, VerifiedCheckpoint};
use sui_types::object::Object;
use sui_types::supported_protocol_versions::ProtocolConfig;
use sui_types::transaction::TransactionData;

// ============================================================================
// Data store read traits
// ============================================================================

/// Transaction data with effects and checkpoint.
#[derive(Clone, Debug)]
pub struct TransactionInfo {
    pub data: TransactionData,
    pub effects: TransactionEffects,
    pub checkpoint: u64,
}

/// A `TransactionStore` has to be able to retrieve transaction data for a given digest.
/// The data provided to `sui_execution::executor::Executor::execute_transaction_to_effects`
/// must be available. Some of that data is not provided by the user. It is naturally available
/// at runtime on a live system and later saved in effects and in the context of a checkpoint.
pub trait TransactionStore {
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
pub struct EpochData {
    pub epoch_id: u64,
    pub protocol_version: u64,
    pub rgp: u64,
    pub start_timestamp: u64,
}

/// An `EpochStore` retrieves the epoch data and protocol configuration
/// for a given epoch.
/// Epoch data is collected by an indexer and it is not stored anywhere otherwise.
/// This is a very small amount of information and could conceivably be saved locally
/// and never hit a server.
pub trait EpochStore {
    /// Return the `EpochData` for a given epoch.
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, Error>;
    /// Return the `ProtocolConfig` for a given epoch.
    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, Error>;
}

/// Query for an object.
/// Specifies an `ObjectID` and the "rule" to retrieve it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObjectKey {
    pub object_id: ObjectID,
    pub version_query: VersionQuery,
}

/// Query options for an object.
/// `Version` request an object at a specific version, or latest if no version is provided
/// `RootVersion` request an object at a given version at most (<=)
/// `AtCheckpoint` request an object at a given checkpoint. Useful for unknown `Version`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VersionQuery {
    Version(u64),
    RootVersion(u64),
    AtCheckpoint(u64),
}

/// The `ObjectStore` trait is used to retrieve objects by their keys,
/// with different query options.
///
/// This trait can execute a subset of what is allowed by
/// `crates/sui-indexer-alt-graphql/schema.graphql::multiGetObjects`.
/// That query likely allows more than what most clients need, which is fairly limited in
/// its usage.
pub trait ObjectStore {
    /// Retrieve objects by their keys, with different query options.
    ///
    /// If the object is not found, the element in the vector is `None`.
    /// Otherwise each tuple contains:
    /// - `Object`: The object data
    /// - `u64`: The actual version of the object
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error>;
}

/// Checkpoint read data.
pub trait CheckpointStore {
    /// Return the verified checkpoint data. If `sequence` is `None`, return the latest checkpoint.
    fn get_verified_checkpoint(
        &self,
        sequence: Option<CheckpointSequenceNumber>,
    ) -> Result<Option<VerifiedCheckpoint>, Error>;
}
