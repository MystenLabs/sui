// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;

use anyhow::{Context, Error, Result};
use cynic::{GraphQlResponse, Operation};

use sui_types::{
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    object::Object,
    supported_protocol_versions::{Chain, ProtocolConfig},
};

use crate::{
    CheckpointData, CheckpointStore, EpochData, EpochStore, ObjectKey, ObjectStore, SetupStore,
    StoreSummary, TransactionInfo, TransactionStore, node::Node,
};

/// Remote GraphQL-backed store.
#[derive(Debug, Clone)]
pub struct GraphQLStore {
    client: reqwest::Client,
    rpc: reqwest::Url,
    node: Node,
    version: String,
}

impl GraphQLStore {
    /// Create a new GraphQL-backed store.
    pub fn new(node: Node, version: &str) -> Result<Self, Error> {
        let rpc = reqwest::Url::parse(node.gql_url())
            .with_context(|| format!("invalid GraphQL URL '{}'", node.gql_url()))?;
        Ok(Self {
            client: reqwest::Client::new(),
            rpc,
            node,
            version: version.to_string(),
        })
    }

    /// Return the configured node.
    pub fn node(&self) -> &Node {
        &self.node
    }

    /// Return the configured GraphQL endpoint.
    pub fn rpc(&self) -> &reqwest::Url {
        &self.rpc
    }

    /// Return the binary version used for identification.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Return the chain associated with the configured node.
    pub fn chain(&self) -> Chain {
        self.node.chain()
    }

    /// Return the HTTP client used by the store.
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    pub(crate) async fn run_query<T, V>(
        &self,
        _operation: &Operation<T, V>,
    ) -> Result<GraphQlResponse<T>, Error>
    where
        T: serde::de::DeserializeOwned,
        V: serde::Serialize,
    {
        todo!("GraphQL query execution is not implemented in the skeleton")
    }
}

impl TransactionStore for GraphQLStore {
    fn transaction_data_and_effects(
        &self,
        _tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        todo!("GraphQL transaction reads are not implemented in the skeleton")
    }
}

impl EpochStore for GraphQLStore {
    fn epoch_info(&self, _epoch: u64) -> Result<Option<EpochData>, Error> {
        todo!("GraphQL epoch reads are not implemented in the skeleton")
    }

    fn protocol_config(&self, _epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        todo!("GraphQL protocol-config reads are not implemented in the skeleton")
    }
}

impl ObjectStore for GraphQLStore {
    fn get_objects(&self, _keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        todo!("GraphQL object reads are not implemented in the skeleton")
    }
}

impl CheckpointStore for GraphQLStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        _sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointData>, Error> {
        todo!("GraphQL checkpoint reads are not implemented in the skeleton")
    }

    fn get_latest_checkpoint(&self) -> Result<Option<CheckpointData>, Error> {
        todo!("GraphQL latest-checkpoint lookup is not implemented in the skeleton")
    }

    fn get_sequence_by_checkpoint_digest(
        &self,
        _digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("GraphQL checkpoint-digest lookups are not implemented in the skeleton")
    }

    fn get_sequence_by_contents_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("GraphQL contents-digest lookups are not implemented in the skeleton")
    }
}

impl SetupStore for GraphQLStore {
    fn setup(&self, _chain_id: Option<String>) -> Result<Option<String>, Error> {
        todo!("GraphQL setup is not implemented in the skeleton")
    }
}

impl StoreSummary for GraphQLStore {
    fn summary<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(
            writer,
            "DataStore(node={}, rpc={}, version={})",
            self.node.network_name(),
            self.rpc,
            self.version
        )?;
        Ok(())
    }
}
