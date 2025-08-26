// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Logical stores needed by the replay tool.
//! Those stores are loosely modeled after the GQL schema in
//! `crates/sui-indexer-alt-graphql/schema.graphql`.
//! A `TransactionStore` is used to retrieve transaction data and effects by digest.
//! An `EpochStore` is used to retrieve epoch information and protocol configuration.
//! An `ObjectStore` is used to retrieve objects by their keys, with different query options.
//!
//! Data is usually retrieved by getting BCS-encoded data rather than navigating the
//! GQL schema.
//! Essentially the code uses the schema to retrieve the data, deserializes it into runtime
//! structures, and then operates on those.
//!
//! A `DataStore` with reasonable defaults is provided for convenience (`data_store.rs`).
//! Other styles of data stores are also provided in `data_stores` for different use cases.

use std::io::Write;
use sui_types::{
    base_types::ObjectID, effects::TransactionEffects, object::Object,
    supported_protocol_versions::ProtocolConfig, transaction::TransactionData,
};

// ============================================================================
// Data store read traits
// ============================================================================

/// Transaction data with effects and checkpoint required to replay a transaction.
#[derive(Clone, Debug)]
pub struct TransactionInfo {
    pub data: TransactionData,
    pub effects: TransactionEffects,
    pub checkpoint: u64,
}

/// A `TransactionStore` has to be able to retrieve transaction data for a given digest.
/// To replay a transaction the data provided to
/// `sui_execution::executor::Executor::execute_transaction_to_effects` must be available.
/// Some of that data is not provided by the user. It is naturally available at runtime on a
/// live system and later saved in effects and in the context of a checkpoint.
pub trait TransactionStore {
    /// Given a transaction digest, return transaction info including data, effects,
    /// and the checkpoint that transaction was executed in.
    /// Returns `None` if the transaction is not found.
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, anyhow::Error>;
}

/// Epoch data required to reaplay a transaction.
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
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, anyhow::Error>;
    /// Return the `ProtocolConfig` for a given epoch.
    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, anyhow::Error>;
}

/// Query for an object.
/// Specifies an `ObjectID` and the "rule" to retrieve it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObjectKey {
    pub object_id: ObjectID,
    pub version_query: VersionQuery,
}

/// Query options for an object.
/// `Version` request an object at a specific version
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
/// That query likely allows more than what the replay tool needs, which is fairly limited in
/// its usage.
pub trait ObjectStore {
    /// Retrieve objects by their keys, with different query options.
    ///
    /// If the object is not found, the element in the vector is `None`.
    /// Otherwise each tuple contains:
    /// - `Object`: The object data
    /// - `u64`: The actual version of the object
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, anyhow::Error>;
}

// ============================================================================
// Set up trait
// ============================================================================

// This is a bit of a hack to allow the FileSystemStore to map network to chain id.
// It is not exposed in a very simple and consistent way quite yet and something
// we want to revisit in the future.

/// A trait to set up the data store.
/// This is used to setup internal state of the data store before starting the replay.
/// At the moment is exclusively used by the FileSystemStore to map network to chain id.
pub trait SetupStore {
    /// Set up the data store.
    /// Returns the chain identifier if available, or None if not available.
    /// When `chain_id` is `None` this is a no-op, unless the given data store
    /// has a way to fetch the chain id from the network.
    /// When `chain_id` is `Some(chain_id)` the given data store should override
    /// the map from network to chain id if it has one.
    /// That is a meaningful operation only for the FileSystemStore.
    fn setup(&self, chain_id: Option<String>) -> Result<Option<String>, anyhow::Error>;
}

// ============================================================================
// Write-back traits
// ============================================================================

/// Write-back trait for transaction data.
/// Allows storing transaction data, effects, and checkpoint information.
pub trait TransactionStoreWriter: TransactionStore {
    /// Store transaction data, effects, and the checkpoint it was executed in.
    fn write_transaction(
        &self,
        tx_digest: &str,
        transaction_info: TransactionInfo,
    ) -> Result<(), anyhow::Error>;
}

/// Write-back trait for epoch data.
/// Allows storing epoch information.
pub trait EpochStoreWriter: EpochStore {
    /// Store epoch data for a given epoch.
    fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), anyhow::Error>;
}

/// Write-back trait for object data.
/// Allows storing objects and their version mappings based on the query type.
pub trait ObjectStoreWriter: ObjectStore {
    /// Store object data based on the ObjectKey and actual version.
    ///
    /// Behavior depends on the VersionQuery in the key:
    /// - `Version(v)`: Stores the object at version `v` (actual_version should equal `v`)
    /// - `RootVersion(max_v)`: Stores a mapping from `max_v` to `actual_version` and
    ///   the object at `actual_version`
    /// - `AtCheckpoint(checkpoint)`: Stores a mapping from `checkpoint` to `actual_version` and
    ///   the object at `actual_version`
    fn write_object(
        &self,
        key: &ObjectKey,
        object: Object,
        actual_version: u64,
    ) -> Result<(), anyhow::Error>;
}

// ============================================================================
// Store summary reporting
// ============================================================================

/// A reporting trait for data stores to print a usage/summary report.
///
/// Implementors are free to print any relevant statistics or configuration details.
/// The writer allows callers to decide where summaries go (stdout, file, buffers, etc.).
pub trait StoreSummary {
    fn summary<W: Write>(&self, writer: &mut W) -> anyhow::Result<()>;
}

// ============================================================================
// Traits combining read and write capabilities
// ============================================================================

/// Trait combining all read capabilities for a data store
pub trait ReadDataStore: TransactionStore + EpochStore + ObjectStore {}

/// Trait combining all read and write capabilities for a data store
pub trait ReadWriteDataStore:
    ReadDataStore + TransactionStoreWriter + EpochStoreWriter + ObjectStoreWriter
{
}

// Blanket implementations for the read and write traits
impl<T> ReadDataStore for T where T: TransactionStore + EpochStore + ObjectStore {}

impl<T> ReadWriteDataStore for T where
    T: ReadDataStore + TransactionStoreWriter + EpochStoreWriter + ObjectStoreWriter
{
}
