// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{anyhow, bail, Context};
use diesel::{dsl, ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use prometheus::Registry;
use reqwest::Client;
use serde_json::{json, Value};
use sui_indexer_alt::{config::IndexerConfig, start_indexer};
use sui_indexer_alt_framework::{ingestion::ClientArgs, schema::watermarks, IndexerArgs};
use sui_indexer_alt_jsonrpc::{
    config::RpcConfig, data::system_package_task::SystemPackageTaskArgs, start_rpc, RpcArgs,
};
use sui_pg_db::{
    temp::{get_available_port, TempDb},
    Db, DbArgs,
};
use sui_transactional_test_runner::{
    create_adapter,
    offchain_state::{OffchainStateReader, TestResponse},
    run_tasks_with_adapter,
    test_adapter::{OffChainConfig, SuiTestAdapter, PRE_COMPILED},
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use url::Url;

struct OffchainCluster {
    rpc_listen_address: SocketAddr,
    indexer: JoinHandle<()>,
    jsonrpc: JoinHandle<()>,
    database: TempDb,
    cancel: CancellationToken,
}

struct OffchainReader {
    db: Db,
    rpc_url: Url,
    client: Client,
    queries: AtomicUsize,
}

datatest_stable::harness!(run_test, "tests", r".*\.move$");

impl OffchainCluster {
    /// Create a new off-chain cluster consisting of a temporary database, Indexer, and JSONRPC
    /// service, to serve requests from E2E tests.
    ///
    /// NOTE: this cluster does not honour the following fields in `OffChainConfig`, because they
    /// do not map to to how its components are implemented:
    ///
    /// - `snapshot_config.sleep_duration` -- there are multiple consistent pipelines, and each
    ///   controls the interval at which it runs.
    /// - `retention_config`, as retention is not measured in epochs for this pipeline (it is
    ///    measured in checkpoints).
    async fn new(config: &OffChainConfig) -> Self {
        let cancel = CancellationToken::new();

        let rpc_port = get_available_port();
        let rpc_listen_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), rpc_port);

        // We don't expose metrics in these tests, but we create a registry to collect them anyway.
        let registry = Registry::new();

        let database = TempDb::new().expect("Failed to create temporary database");

        let db_args = DbArgs {
            database_url: database.database().url().clone(),
            ..Default::default()
        };

        let client_args = ClientArgs {
            local_ingestion_path: Some(config.data_ingestion_path.clone()),
            remote_store_url: None,
        };

        // The example config includes every pipeline, and we configure its consistent range using
        // the off-chain config that was passed in.
        let mut indexer_config = IndexerConfig::example();
        indexer_config.consistency.retention = Some(config.snapshot_config.snapshot_min_lag as u64);

        let rpc_args = RpcArgs {
            rpc_listen_address,
            ..Default::default()
        };

        let rpc_config = RpcConfig::example();

        // This configuration controls how often the RPC service checks for changes to system
        // packages. The default polling interval is probably too slow for changes to get picked
        // up, so tests that rely on this behaviour will always fail, but this is better than flaky
        // behavior.
        let system_package_task_args = SystemPackageTaskArgs::default();

        let with_genesis = true;
        let indexer = start_indexer(
            db_args.clone(),
            IndexerArgs::default(),
            client_args,
            indexer_config,
            with_genesis,
            &registry,
            cancel.child_token(),
        )
        .await
        .expect("Failed to start indexer");

        let jsonrpc = start_rpc(
            db_args,
            rpc_args,
            system_package_task_args,
            rpc_config,
            &registry,
            cancel.child_token(),
        )
        .await
        .expect("Failed to start JSON-RPC server");

        Self {
            rpc_listen_address,
            indexer,
            jsonrpc,
            database,
            cancel,
        }
    }

    /// An implementation of the API that the test cluster uses to send reads to the off-chain
    /// set-up.
    async fn reader(&self) -> Box<dyn OffchainStateReader> {
        let db = Db::for_read(DbArgs {
            database_url: self.database.database().url().clone(),
            ..Default::default()
        })
        .await
        .expect("Failed to connect to database");

        let rpc_url = Url::parse(&format!("http://{}/", self.rpc_listen_address))
            .expect("Failed to parse RPC URL");

        Box::new(OffchainReader {
            db,
            rpc_url,
            client: Client::new(),
            queries: AtomicUsize::new(0),
        })
    }

    /// Triggers cancellation of all downstream services, waits for them to stop and cleans up the
    /// temporary database.
    async fn stopped(self) {
        self.cancel.cancel();
        let _ = self.indexer.await;
        let _ = self.jsonrpc.await;
    }
}

impl OffchainReader {
    /// Wait indefinitely until all pipelines have caught up with `checkpoint`.
    async fn wait_for_checkpoint(&self, checkpoint: u64) {
        loop {
            if matches!(self.latest_checkpoint().await, Ok(latest) if latest >= checkpoint) {
                break;
            } else {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }

    /// Return the lowest checkpoint that we have committed data for across all pipelines,
    /// according to the watermarks table.
    ///
    /// NOTE: We exclude pipelines that we know correspond to pruners, because they usually lag
    /// behind their committer counterparts.
    async fn latest_checkpoint(&self) -> anyhow::Result<u64> {
        let mut conn = self
            .db
            .connect()
            .await
            .context("Failed to connect to database")?;

        // FIXME: It's not ideal that we have to enumerate pruners here -- if we forget to add one,
        // tests will hang indefinitely. Hopefully, by moving these over to the framework's pruning
        // support, we can avoid this complication.
        const PRUNERS: &[&str] = &["coin_balance_buckets_pruner", "obj_info_pruner"];

        let latest: Option<i64> = watermarks::table
            .select(dsl::min(watermarks::checkpoint_hi_inclusive))
            .filter(watermarks::pipeline.ne_all(PRUNERS))
            .first(&mut conn)
            .await?;

        latest
            .map(|latest| latest as u64)
            .ok_or_else(|| anyhow!("No checkpoints recorded yet"))
    }
}

#[async_trait::async_trait]
impl OffchainStateReader for OffchainReader {
    async fn wait_for_checkpoint_catchup(&self, checkpoint: u64, base_timeout: Duration) {
        let _ = tokio::time::timeout(base_timeout, self.wait_for_checkpoint(checkpoint)).await;
    }

    async fn wait_for_pruned_checkpoint(&self, _: u64, _: Duration) {
        unimplemented!("Waiting for pruned checkpoints is not supported in these tests (add it if you need it)");
    }

    async fn execute_graphql(&self, _: String, _: bool) -> anyhow::Result<TestResponse> {
        bail!("GraphQL queries are not supported in these tests")
    }

    async fn execute_jsonrpc(&self, method: String, params: Value) -> anyhow::Result<TestResponse> {
        let query = json!({
            "jsonrpc": "2.0",
            "id": self.queries.fetch_add(1, Ordering::SeqCst),
            "method": method,
            "params": params,
        });

        let response = self
            .client
            .post(self.rpc_url.clone())
            .json(&query)
            .send()
            .await
            .context("Request to JSON-RPC server failed")?;

        // Extract headers but remove the ones that will change from run to run.
        let mut headers = response.headers().clone();
        headers.remove("date");

        let body: Value = response
            .json()
            .await
            .context("Failed to parse JSON-RPC response")?;

        Ok(TestResponse {
            response_body: serde_json::to_string_pretty(&body)?,
            http_headers: Some(headers),
            service_version: None,
        })
    }
}

#[cfg_attr(not(msim), tokio::main)]
#[cfg_attr(msim, msim::main)]
async fn run_test(path: &Path) -> Result<(), Box<dyn Error>> {
    if cfg!(msim) {
        return Ok(());
    }

    telemetry_subscribers::init_for_testing();

    // start the adapter first to start the executor (simulacrum)
    let (output, mut adapter) =
        create_adapter::<SuiTestAdapter>(path, Some(Arc::new(PRE_COMPILED.clone()))).await?;

    // configure access to the off-chain reader
    let cluster = OffchainCluster::new(adapter.offchain_config.as_ref().unwrap()).await;
    adapter.with_offchain_reader(cluster.reader().await);

    // run the tasks in the test
    run_tasks_with_adapter(path, adapter, output).await?;

    // clean-up the off-chain cluster
    cluster.stopped().await;
    Ok(())
}
