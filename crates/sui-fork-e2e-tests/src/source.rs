// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A local Sui network with the GraphQL stack a fork reads from.
//!
//! `sui-fork` resolves everything it does not hold locally through GraphQL pinned at the fork
//! checkpoint, so a source network needs a fullnode, an indexer writing to PostgreSQL, a
//! consistent store for the owned-object and balance enumerations that seeding relies on, and a
//! GraphQL server over both. [`SourceNetwork`] starts all of them in-process on ephemeral ports,
//! wired the same way `sui start --with-graphql` wires them, so tests can run in parallel without
//! colliding on ports.

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::ensure;
use prometheus::Registry;
use reqwest::Client;
use serde_json::json;
use sui_futures::service::Service;
use sui_indexer_alt::config::IndexerConfig;
use sui_indexer_alt::setup_indexer;
use sui_indexer_alt_consistent_store::args::RpcArgs as ConsistentArgs;
use sui_indexer_alt_consistent_store::config::ServiceConfig as ConsistentConfig;
use sui_indexer_alt_consistent_store::start_service as start_consistent_store;
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
use sui_types::base_types::SuiAddress;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use url::Url;

/// Epoch length of the source network, long enough that no test straddles an epoch change.
const EPOCH_DURATION_MS: u64 = 60 * 60 * 1_000;

/// Longest wait for GraphQL to index a checkpoint the fullnode has already produced.
const GRAPHQL_SYNC_TIMEOUT: Duration = Duration::from_secs(60);

/// Binaries the temporary PostgreSQL needs on `PATH`.
const POSTGRES_BINARIES: [&str; 3] = ["initdb", "postgres", "pg_ctl"];

/// A running localnet together with the indexer, consistent store, and GraphQL server over it.
///
/// [`Self::stop`] shuts the services down before the temporary database and directories are
/// removed, so nothing is left writing to them.
pub struct SourceNetwork {
    pub cluster: TestCluster,
    graphql_url: Url,
    services: Service,
    _database: TempDb,
    _ingestion_dir: tempfile::TempDir,
    _consistent_store_dir: tempfile::TempDir,
}

impl SourceNetwork {
    /// Start a one-validator localnet, index it, and serve GraphQL over the indexes.
    ///
    /// Returns once GraphQL has caught up with the fullnode's latest checkpoint, so a fork taken
    /// "at latest" straight afterwards does not race the indexer. Fails before starting anything
    /// when a PostgreSQL binary is missing, because `TempDb` reports that only as a "command not
    /// found" deep in an error chain.
    pub async fn start() -> Result<Self> {
        check_postgres_binaries()?;

        let ingestion_dir = tempfile::tempdir().context("failed to create ingestion directory")?;
        let cluster = TestClusterBuilder::new()
            .with_num_validators(1)
            .with_epoch_duration_ms(EPOCH_DURATION_MS)
            .disable_fullnode_pruning()
            .with_data_ingestion_dir(ingestion_dir.path().to_path_buf())
            .build()
            .await;

        let database = TempDb::new().context("failed to start temporary PostgreSQL")?;
        let database_url = database.database().url().clone();
        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(ingestion_dir.path().to_path_buf()),
                ..Default::default()
            },
            ..Default::default()
        };
        let registry = Registry::new();
        let indexer = setup_indexer(
            database_url.clone(),
            DbArgs::default(),
            IndexerArgs::default(),
            client_args.clone(),
            IndexerConfig::for_test(),
            None,
            &registry,
        )
        .await
        .context("failed to set up source indexer")?;
        let pipelines = indexer.pipelines().map(str::to_owned).collect();
        let indexer = indexer
            .run()
            .await
            .context("failed to start source indexer")?;

        let consistent_store_dir =
            tempfile::tempdir().context("failed to create consistent store directory")?;
        let consistent_store_address =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), get_available_port());
        let consistent_store = start_consistent_store(
            consistent_store_dir.path(),
            IndexerArgs::default(),
            client_args,
            ConsistentArgs {
                rpc_listen_address: consistent_store_address,
                ..Default::default()
            },
            "test",
            ConsistentConfig::for_test(),
            &registry,
        )
        .await
        .context("failed to start source consistent store")?;

        let graphql_address =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), get_available_port());
        let fullnode_url = Url::parse(cluster.rpc_url()).context("invalid fullnode RPC URL")?;
        let graphql = start_graphql(
            Some(database_url),
            FullnodeArgs::new(fullnode_url.clone()),
            DbArgs::default(),
            KvArgs {
                ledger_grpc_url: Some(fullnode_url.as_str().parse()?),
                ..Default::default()
            },
            ConsistentReaderArgs {
                consistent_store_url: Some(Url::parse(&format!(
                    "http://{consistent_store_address}"
                ))?),
                ..Default::default()
            },
            GraphQlArgs {
                rpc_listen_address: graphql_address,
                no_ide: true,
            },
            SystemPackageTaskArgs::default(),
            SubscriptionArgs::default(),
            "test",
            GraphQlConfig::default(),
            pipelines,
            &registry,
        )
        .await
        .context("failed to start source GraphQL")?;
        let graphql_url = Url::parse(&format!("http://{graphql_address}/graphql"))?;

        let network = Self {
            cluster,
            graphql_url,
            services: indexer.merge(consistent_store).merge(graphql),
            _database: database,
            _ingestion_dir: ingestion_dir,
            _consistent_store_dir: consistent_store_dir,
        };
        network.wait_for_graphql_tip().await?;
        Ok(network)
    }

    /// Return the GraphQL endpoint, which is what a fork passes as `--network`.
    pub fn graphql_url(&self) -> &Url {
        &self.graphql_url
    }

    /// Return the fullnode RPC URL, which the Sui CLI uses as the `rpc` of its `localnet` env.
    pub fn rpc_url(&self) -> &str {
        self.cluster.rpc_url()
    }

    /// Return the directory holding the localnet's `client.yaml` and keystore.
    pub fn config_dir(&self) -> &Path {
        self.cluster.swarm.dir()
    }

    /// Return the first funded address in the localnet keystore.
    pub fn active_address(&self) -> SuiAddress {
        self.cluster.get_address_0()
    }

    /// Wait until GraphQL has indexed `checkpoint`.
    pub async fn wait_for_graphql(&self, checkpoint: u64) -> Result<()> {
        tokio::time::timeout(GRAPHQL_SYNC_TIMEOUT, async {
            loop {
                if self
                    .latest_graphql_checkpoint()
                    .await
                    .is_ok_and(|latest| latest >= checkpoint)
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        })
        .await
        .with_context(|| format!("GraphQL did not reach checkpoint {checkpoint}"))?;
        Ok(())
    }

    /// Wait until GraphQL has indexed the fullnode's current latest checkpoint.
    pub async fn wait_for_graphql_tip(&self) -> Result<()> {
        let tip = self
            .cluster
            .grpc_client()
            .get_latest_checkpoint()
            .await
            .context("failed to read the fullnode's latest checkpoint")?
            .sequence_number;
        self.wait_for_graphql(tip).await
    }

    /// Stop the indexer, the consistent store, and the GraphQL server.
    pub async fn stop(self) -> Result<()> {
        self.services.shutdown().await?;
        Ok(())
    }

    async fn latest_graphql_checkpoint(&self) -> Result<u64> {
        let body: serde_json::Value = Client::new()
            .post(self.graphql_url.clone())
            .json(&json!({ "query": "query { checkpoint { sequenceNumber } }" }))
            .send()
            .await?
            .json()
            .await?;

        body.pointer("/data/checkpoint/sequenceNumber")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| anyhow!("GraphQL checkpoint response is missing data: {body}"))
    }
}

/// Fail with an actionable message when a PostgreSQL binary is missing from `PATH`.
///
/// The check fails rather than skipping the test, because CI provisions PostgreSQL and a silent
/// skip would hide a broken image.
fn check_postgres_binaries() -> Result<()> {
    for binary in POSTGRES_BINARIES {
        ensure!(
            on_path(binary),
            "`{binary}` is not on PATH. The sui-fork e2e tests start a temporary PostgreSQL for \
             the source indexer, so install PostgreSQL and add its bin directory to PATH."
        );
    }
    Ok(())
}

fn on_path(binary: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(binary).is_file()))
}
