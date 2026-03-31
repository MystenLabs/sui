// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;

use anyhow::{Context, Error, Result};
use cynic::{GraphQlResponse, Operation};
use reqwest::header::USER_AGENT;

use sui_types::{
    committee::ProtocolVersion,
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    supported_protocol_versions::{Chain, ProtocolConfig},
};

use crate::{
    CheckpointData, CheckpointStore, EpochData, EpochStore, SetupStore, StoreSummary,
    gql_queries::{chain_id_query, checkpoint_query, epoch_query},
    node::Node,
    normalize_chain_identifier,
};

macro_rules! block_on {
    ($expr:expr) => {{
        #[allow(clippy::disallowed_methods, clippy::result_large_err)]
        {
            if tokio::runtime::Handle::try_current().is_ok() {
                std::thread::scope(|scope| {
                    scope
                        .spawn(|| {
                            let rt = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .expect("failed to build Tokio runtime");
                            rt.block_on($expr)
                        })
                        .join()
                        .expect("failed to join scoped thread running nested runtime")
                })
            } else {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build Tokio runtime");
                rt.block_on($expr)
            }
        }
    }};
}

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

    pub(crate) async fn run_query<T, V>(
        &self,
        operation: &Operation<T, V>,
    ) -> Result<GraphQlResponse<T>, Error>
    where
        T: serde::de::DeserializeOwned,
        V: serde::Serialize,
    {
        self.client
            .post(self.rpc.clone())
            .header(USER_AGENT, format!("forking-data-store-v{}", self.version))
            .json(operation)
            .send()
            .await
            .context("failed to send GQL query")?
            .json::<GraphQlResponse<T>>()
            .await
            .context("failed to read response in GQL query")
    }
}

impl EpochStore for GraphQLStore {
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, Error> {
        block_on!(epoch_query::query(epoch, self))
    }

    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        Ok(self.epoch_info(epoch)?.map(|epoch_data| {
            ProtocolConfig::get_for_version(
                ProtocolVersion::new(epoch_data.protocol_version),
                self.chain(),
            )
        }))
    }
}

impl CheckpointStore for GraphQLStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointData>, Error> {
        block_on!(checkpoint_query::query(Some(sequence), self))
    }

    fn get_latest_checkpoint(&self) -> Result<Option<CheckpointData>, Error> {
        block_on!(checkpoint_query::query(None, self))
    }

    fn get_sequence_by_checkpoint_digest(
        &self,
        _digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        Ok(None)
    }

    fn get_sequence_by_contents_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        Ok(None)
    }
}

impl SetupStore for GraphQLStore {
    fn setup(&self, chain_id: Option<String>) -> Result<Option<String>, Error> {
        let chain_id = match chain_id {
            Some(chain_id) => chain_id,
            None => block_on!(chain_id_query::query(self))?,
        };
        Ok(Some(normalize_chain_identifier(&chain_id)?))
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
