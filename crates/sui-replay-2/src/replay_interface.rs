// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Logical stores needed by the replay tool.
//! Those stores are loosely modeled after the GQL schema in `crates/sui-indexer-alt-graphql/schema.graphql`.
//! A `TransactionStore` is used to retrieve transaction data and effects by digest.
//! An `EpochStore` is used to retrieve epoch information and protocol configuration.
//! An `ObjectStore` is used to retrieve objects by their keys, with different query options.
//!
//! Data is usually retrieved by getting the bcs encoded data rather than navigating the GQL schema.
//! Essetially the code uses the schema to retireve the data that is deserilized to runtime structures
//! and work with them.
//!
//! A `DataStore` with reasonable defaults is provided for convenience (`data_store.rs`).

use sui_types::{
    base_types::ObjectID, effects::TransactionEffects, object::Object,
    supported_protocol_versions::ProtocolConfig, transaction::TransactionData,
};

/// A `TransactionStore` has to be able to retrieve transaction data for a given digest.
/// To replay a transaction the data in input to
/// `sui_execution::executor::Executor::execute_transaction_to_effects` has to be provided.
/// Some of that data is not provided by the user. It is naturally available at runtime
/// on a live system and later saved in effects and in the context of a checkpoint.
pub trait TransactionStore {
    /// Given transaction digest, return `TransactionData`, `TransactionEffects`
    /// and the checkpoint that transaction was executed in.
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<(TransactionData, TransactionEffects, u64), anyhow::Error>;
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
/// given an epoch.
/// Epoch data is collected by an indexer and it is not stored anywhere
/// otherwise.
/// This is a very small amount of information and could conceivably be
/// saved locally and never hit a server.
pub trait EpochStore {
    /// Return the `EpochData` for a given epoch.
    fn epoch_info(&self, epoch: u64) -> Result<EpochData, anyhow::Error>;
    /// Return the `ProtocolConfig` for a given epoch.
    fn protocol_config(&self, epoch: u64) -> Result<ProtocolConfig, anyhow::Error>;
}

/// Query for an object.
/// Specifies an `ObjectID` and the rule to retrieve it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObjectKey {
    pub object_id: ObjectID,
    pub version_query: VersionQuery,
}

/// Query options for an object.
/// `Version` request an object at a specific version
/// `RootVersion` request an object at a given version at most (<=)
/// `AtCheckpoint` request an object at a given checkpoint. Useful for unknown `Version).
/// `ImmutableOrLatest` requests an object assumed to be unversionalbe in the system (e.g. user packages)
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VersionQuery {
    Version(u64),
    RootVersion(u64),
    AtCheckpoint(u64),
    ImmutableOrLatest,
}

/// A pasthrough query request for a store that can execute a subset of what is
/// allowed by `crates/sui-indexer-alt-graphql/schema.graphql::multiGetObjects`.
/// That query is likely to allow more than what is needed by the replay tool,
/// and it's pretty trivial in its usage.
pub trait ObjectStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<Object>>, anyhow::Error>;
}
