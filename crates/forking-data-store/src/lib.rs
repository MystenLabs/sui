// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Multi-tier caching data store for Sui blockchain data.
//!
//! This crate provides a flexible data store abstraction for retrieving and caching
//! Sui blockchain data (transactions, checkpooints, epochs, objects). The stores are loosely
//! modeled after the GQL schema in `crates/sui-indexer-alt-graphql/schema.graphql`.
//!
//! ## Core Traits
//!
//! - [`TransactionStore`] - Retrieve transaction data and effects by digest
//! - [`CheckpointStore`] - Retrieve checkpoint data by sequence number or digest
//! - [`EpochStore`] - Retrieve epoch information and protocol configuration
//! - [`ObjectStore`] - Retrieve objects by their keys with flexible version queries
//!
//! ## Store Implementations
//!
//! - [`stores::GraphQLStore`] - Remote GraphQL-backed store (mainnet/testnet/devnet/custom GraphQL
//!   endpoint)
//! - [`stores::FileSystemStore`] - Persistent local disk cache
//! - [`stores::InMemoryStore`] - Unbounded in-memory cache
//! - [`stores::LruMemoryStore`] - Bounded LRU cache
//!
//! ## Composition
//!
//! Use the composition primitives from [`stores`] to assemble capability-specific
//! cache chains:
//! - [`stores::ReadThroughStore`] - cache over read-only source
//! - [`stores::WriteThroughStore`] - hot cache over writable backing store
//! - [`stores::ForkingStore`] - route each capability to a different store

mod gql_queries;
pub mod node;
pub mod stores;

pub use node::Node;

use std::{io::Write, ops::Deref, sync::Arc};

use anyhow::{Error, Result};

use sui_types::{
    base_types::ObjectID,
    digests::{CheckpointContentsDigest, CheckpointDigest},
    effects::TransactionEffects,
    full_checkpoint_content::Checkpoint,
    messages_checkpoint::CheckpointSequenceNumber,
    object::Object,
    supported_protocol_versions::ProtocolConfig,
    transaction::TransactionData,
};

type CheckpointData = Checkpoint;

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

/// Retrieve checkpoint data and indexes.
pub trait CheckpointStore {
    /// Return a full checkpoint payload by sequence number.
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Result<Option<Checkpoint>, Error>;

    /// Return the latest checkpoint known to this store.
    fn get_latest_checkpoint(&self) -> Result<Option<Checkpoint>, Error>;

    /// Resolve a checkpoint digest to a sequence number.
    fn get_sequence_by_checkpoint_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error>;

    /// Resolve a checkpoint contents digest to a sequence number.
    fn get_sequence_by_contents_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error>;
}

// ============================================================================
// Set up trait
// ============================================================================

// This is a bit of a hack to allow the FileSystemStore to map network to chain id.
// It is not exposed in a very simple and consistent way quite yet and something
// we want to revisit in the future.

/// A trait to set up the data store.
/// This is used to setup internal state of the data store before use.
/// At the moment is exclusively used by the FileSystemStore to map network to chain id.
pub trait SetupStore {
    /// Set up the data store.
    /// Returns the chain identifier if available, or None if not available.
    /// When `chain_id` is `None` this is a no-op, unless the given data store
    /// has a way to fetch the chain id from the network.
    /// When `chain_id` is `Some(chain_id)` the given data store should override
    /// the map from network to chain id if it has one.
    /// That is a meaningful operation only for the FileSystemStore.
    fn setup(&self, chain_id: Option<String>) -> Result<Option<String>, Error>;
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
    ) -> Result<(), Error>;
}

/// Write-back trait for epoch data.
/// Allows storing epoch information.
pub trait EpochStoreWriter: EpochStore {
    /// Store epoch data for a given epoch.
    fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), Error>;
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
    ) -> Result<(), Error>;
}

/// Write-back trait for checkpoint data.
pub trait CheckpointStoreWriter: CheckpointStore {
    /// Persist a full checkpoint payload.
    fn write_checkpoint(&self, checkpoint: &Checkpoint) -> Result<(), Error>;
}

// ============================================================================
// Store summary reporting
// ============================================================================

/// A reporting trait for data stores to print a usage/summary report.
///
/// Implementors are free to print any relevant statistics or configuration details.
/// The writer allows callers to decide where summaries go (stdout, file, buffers, etc.).
pub trait StoreSummary {
    fn summary<W: Write>(&self, writer: &mut W) -> Result<()>;
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

macro_rules! impl_store_for_deref {
    ($wrapper:ty) => {
        impl<T: TransactionStore + ?Sized> TransactionStore for $wrapper {
            fn transaction_data_and_effects(
                &self,
                tx_digest: &str,
            ) -> Result<Option<TransactionInfo>, Error> {
                Deref::deref(self).transaction_data_and_effects(tx_digest)
            }
        }

        impl<T: TransactionStoreWriter + ?Sized> TransactionStoreWriter for $wrapper {
            fn write_transaction(
                &self,
                tx_digest: &str,
                transaction_info: TransactionInfo,
            ) -> Result<(), Error> {
                Deref::deref(self).write_transaction(tx_digest, transaction_info)
            }
        }

        impl<T: EpochStore + ?Sized> EpochStore for $wrapper {
            fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, Error> {
                Deref::deref(self).epoch_info(epoch)
            }

            fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
                Deref::deref(self).protocol_config(epoch)
            }
        }

        impl<T: EpochStoreWriter + ?Sized> EpochStoreWriter for $wrapper {
            fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), Error> {
                Deref::deref(self).write_epoch_info(epoch, epoch_data)
            }
        }

        impl<T: ObjectStore + ?Sized> ObjectStore for $wrapper {
            fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
                Deref::deref(self).get_objects(keys)
            }
        }

        impl<T: ObjectStoreWriter + ?Sized> ObjectStoreWriter for $wrapper {
            fn write_object(
                &self,
                key: &ObjectKey,
                object: Object,
                actual_version: u64,
            ) -> Result<(), Error> {
                Deref::deref(self).write_object(key, object, actual_version)
            }
        }

        impl<T: CheckpointStore + ?Sized> CheckpointStore for $wrapper {
            fn get_checkpoint_by_sequence_number(
                &self,
                sequence: CheckpointSequenceNumber,
            ) -> Result<Option<Checkpoint>, Error> {
                Deref::deref(self).get_checkpoint_by_sequence_number(sequence)
            }

            fn get_latest_checkpoint(&self) -> Result<Option<Checkpoint>, Error> {
                Deref::deref(self).get_latest_checkpoint()
            }

            fn get_sequence_by_checkpoint_digest(
                &self,
                digest: &CheckpointDigest,
            ) -> Result<Option<CheckpointSequenceNumber>, Error> {
                Deref::deref(self).get_sequence_by_checkpoint_digest(digest)
            }

            fn get_sequence_by_contents_digest(
                &self,
                digest: &CheckpointContentsDigest,
            ) -> Result<Option<CheckpointSequenceNumber>, Error> {
                Deref::deref(self).get_sequence_by_contents_digest(digest)
            }
        }

        impl<T: CheckpointStoreWriter + ?Sized> CheckpointStoreWriter for $wrapper {
            fn write_checkpoint(&self, checkpoint: &Checkpoint) -> Result<(), Error> {
                Deref::deref(self).write_checkpoint(checkpoint)
            }
        }

        impl<T: SetupStore + ?Sized> SetupStore for $wrapper {
            fn setup(&self, chain_id: Option<String>) -> Result<Option<String>, Error> {
                Deref::deref(self).setup(chain_id)
            }
        }

        impl<T: StoreSummary + ?Sized> StoreSummary for $wrapper {
            fn summary<W: Write>(&self, writer: &mut W) -> Result<()> {
                Deref::deref(self).summary(writer)
            }
        }
    };
}

impl_store_for_deref!(&T);
impl_store_for_deref!(Box<T>);
impl_store_for_deref!(Arc<T>);
