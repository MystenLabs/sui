// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! An implementation of the replay interfaces: `TransactionStore`, `EpochStore`, and `ObjectStore`.
//! The `DataStore` is backed by the rpc gql endpoint and the schema defined in
//! `crates/sui-indexer-alt-graphql/schema.graphql`.
//! The `DataStore` carries an epoch cache and soon a system package cache that can help
//! when replaying multiple transactions.

use crate::{
    gql_queries,
    replay_interface::{EpochData, EpochStore, ObjectKey, ObjectStore, TransactionStore},
    Node,
};
use anyhow::Context;
use cynic::GraphQlResponse;
use cynic::Operation;
use reqwest::header::USER_AGENT;
use std::{collections::BTreeMap, sync::RwLock};
use sui_types::{
    base_types::ObjectID,
    committee::ProtocolVersion,
    effects::TransactionEffects,
    object::Object,
    supported_protocol_versions::{Chain, ProtocolConfig},
    transaction::TransactionData,
};
use tracing::debug;

//
// DataStore and traits implementations
//

type EpochId = u64;

// Simple implementation of the replay_interface traits
pub struct DataStore {
    client: reqwest::Client,
    rpc: reqwest::Url,
    node: Node,
    // Keep the epoch data considering its small size and footprint
    epoch_map: RwLock<BTreeMap<EpochId, EpochData>>,
    // TODO: define a system package map?
    /// The binary's version passed to the User-Agent header in GQL query requests
    version: String,
}

macro_rules! block_on {
    ($expr:expr) => {{
        // TODO: Use this (and remove the `futures` `block_on` below) once simtest support
        // `block_in_place`.
        // tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on($expr))
        #[allow(clippy::disallowed_methods, clippy::result_large_err)]
        {
            futures::executor::block_on($expr)
        }
    }};
}

impl TransactionStore for DataStore {
    fn transaction_data_and_effects(
        &self,
        digest: &str,
    ) -> Result<(TransactionData, TransactionEffects, u64), anyhow::Error> {
        block_on!(self.transaction(digest))
    }
}

impl EpochStore for DataStore {
    fn epoch_info(&self, epoch: u64) -> Result<EpochData, anyhow::Error> {
        if let Some(epoch_data) = self.epoch_map.read().unwrap().get(&epoch) {
            return Ok(epoch_data.clone());
        }
        let epoch_data = block_on!(self.epoch(epoch))?;
        self.epoch_map
            .write()
            .unwrap()
            .insert(epoch, epoch_data.clone());
        Ok(epoch_data)
    }

    // Get the protocol config for an epoch directly from the binary
    fn protocol_config(&self, epoch: u64) -> Result<ProtocolConfig, anyhow::Error> {
        let epoch = self.epoch_info(epoch)?;
        Ok(ProtocolConfig::get_for_version(
            ProtocolVersion::new(epoch.protocol_version),
            self.chain(),
        ))
    }
}

impl ObjectStore for DataStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<Object>>, anyhow::Error> {
        block_on!(self.objects(keys))
    }
}

impl DataStore {
    pub fn new(node: Node, version: &str) -> Result<Self, anyhow::Error> {
        debug!("Start stores creation");
        let client = reqwest::Client::new();
        let url = match node {
            Node::Mainnet => "https://public-rpc.sui-mainnet.mystenlabs.com/graphql",
            Node::Testnet => "https://public-rpc.sui-testnet.mystenlabs.com/graphql",
            // Node::Devnet => "",
            Node::Custom(ref url) => url,
        };
        let rpc =
            reqwest::Url::parse(url).context(format!("Failed to parse GQL RPC URL {}", url))?;
        let epoch_map = RwLock::new(BTreeMap::new());
        debug!("End stores creation");

        Ok(Self {
            client,
            node,
            epoch_map,
            rpc,
            version: version.to_string(),
        })
    }

    pub fn node(&self) -> &Node {
        &self.node
    }

    pub fn chain(&self) -> Chain {
        self.node.chain()
    }

    // This is exclusively called from GQL queries
    pub(crate) async fn run_query<T, V>(
        &self,
        operation: &Operation<T, V>,
    ) -> Result<GraphQlResponse<T>, anyhow::Error>
    where
        T: serde::de::DeserializeOwned,
        V: serde::Serialize,
    {
        self.client
            .post(self.rpc.clone())
            .header(USER_AGENT, format!("sui-replay-v{}", &self.version))
            .json(&operation)
            .send()
            .await
            .context("Failed to send GQL query")?
            .json::<GraphQlResponse<T>>()
            .await
            .context("Failed to read response in GQL query")
    }

    //
    // Wrappers around the GQL queries
    //

    async fn transaction(
        &self,
        digest: &str,
    ) -> Result<(TransactionData, TransactionEffects, u64), anyhow::Error> {
        debug!("Start transaction data query");
        let data = gql_queries::txn_query::query(digest.to_string(), self).await;
        debug!("End transaction data query");
        data
    }

    async fn epoch(&self, epoch_id: u64) -> Result<EpochData, anyhow::Error> {
        debug!("Start epoch query");
        let data = gql_queries::epoch_query::query(epoch_id, self).await;
        debug!("End epoch query");
        data
    }

    async fn objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<Object>>, anyhow::Error> {
        debug!("Start multi objects query");
        let data = gql_queries::object_query::query(keys, self).await;
        debug!("End multi objects query");
        data
    }
}

#[allow(dead_code)]
// True if the package is a system package
fn is_framework_package(pkg_id: &ObjectID) -> bool {
    sui_types::SYSTEM_PACKAGE_ADDRESSES.contains(pkg_id)
}
