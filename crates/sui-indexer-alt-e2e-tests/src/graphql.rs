// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;

use anyhow::Context;
use prometheus::Registry;
use reqwest::Client;
use serde_json::Value;
use serde_json::json;
use sui_futures::service::Service;
use sui_indexer_alt::config::IndexerConfig;
use sui_indexer_alt::setup_indexer;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_indexer_alt_graphql::RpcArgs as GraphQlArgs;
use sui_indexer_alt_graphql::args::SubscriptionArgs;
use sui_indexer_alt_graphql::config::RpcConfig as GraphQlConfig;
use sui_indexer_alt_graphql::start_rpc as start_graphql;
use sui_indexer_alt_reader::consistent_reader::ConsistentReaderArgs;
use sui_indexer_alt_reader::fullnode_client::FullnodeArgs;
use sui_indexer_alt_reader::kv_loader::KvArgs;
use sui_indexer_alt_reader::system_package_task::SystemPackageTaskArgs;
use sui_pg_db::DbArgs;
use sui_pg_db::temp::TempDb;
use sui_pg_db::temp::get_available_port;
use test_cluster::TestCluster;
use url::Url;

/// A lightweight indexed GraphQL test harness backed by a validator cluster and Postgres.
///
/// Unlike `FullCluster`, this only starts the indexer and GraphQL services needed by tests that
/// execute against a validator cluster and then query indexed GraphQL state.
pub struct IndexedGraphQlCluster {
    url: Url,
    client: Client,
    /// Hold on to the service so it doesn't get dropped (and therefore aborted) until the cluster
    /// goes out of scope.
    #[allow(unused)]
    service: Service,
    /// Hold on to the database so it doesn't get dropped until the cluster is stopped.
    #[allow(unused)]
    database: TempDb,
}

impl IndexedGraphQlCluster {
    pub async fn new(validator_cluster: &TestCluster) -> Self {
        let graphql_port = get_available_port();
        let graphql_listen_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), graphql_port);

        let database = TempDb::new().expect("Failed to create temp database");
        let database_url = database.database().url().clone();

        let fullnode_args = FullnodeArgs::new(validator_cluster.rpc_url().parse().unwrap());

        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                rpc_api_url: Some(
                    Url::parse(validator_cluster.rpc_url()).expect("Invalid RPC URL"),
                ),
                ..Default::default()
            },
            ..Default::default()
        };

        let indexer = setup_indexer(
            database_url.clone(),
            DbArgs::default(),
            IndexerArgs::default(),
            client_args,
            IndexerConfig::for_test(),
            None,
            &Registry::new(),
        )
        .await
        .expect("Failed to setup indexer");

        let pipelines: Vec<String> = indexer.pipelines().map(|s| s.to_string()).collect();
        let s_indexer = indexer.run().await.expect("Failed to start indexer");

        let s_graphql = start_graphql(
            Some(database_url),
            fullnode_args,
            DbArgs::default(),
            KvArgs::default(),
            ConsistentReaderArgs::default(),
            GraphQlArgs {
                rpc_listen_address: graphql_listen_address,
                no_ide: true,
            },
            SystemPackageTaskArgs::default(),
            SubscriptionArgs::default(),
            "0.0.0",
            GraphQlConfig::default(),
            pipelines,
            &Registry::new(),
        )
        .await
        .expect("Failed to start GraphQL server");

        let url = Url::parse(&format!("http://{}/graphql", graphql_listen_address))
            .expect("Failed to parse GraphQL URL");

        Self {
            url,
            client: Client::new(),
            service: s_graphql.merge(s_indexer),
            database,
        }
    }

    pub fn url(&self) -> Url {
        self.url.clone()
    }

    /// Execute a GraphQL mutation or query.
    pub async fn execute_graphql(&self, query: &str, variables: Value) -> anyhow::Result<Value> {
        let request_body = json!({
            "query": query,
            "variables": variables
        });

        let response = self
            .client
            .post(self.url.clone())
            .json(&request_body)
            .send()
            .await
            .context("GraphQL request failed")?;

        let body: Value = response
            .json()
            .await
            .context("Failed to parse GraphQL response")?;

        Ok(body)
    }
}
