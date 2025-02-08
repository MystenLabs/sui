// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    error::Error,
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
use sui_indexer_alt::config::IndexerConfig;
use sui_indexer_alt_e2e_tests::OffchainCluster;
use sui_indexer_alt_framework::{ingestion::ClientArgs, schema::watermarks, IndexerArgs};
use sui_indexer_alt_jsonrpc::{
    config::RpcConfig, data::system_package_task::SystemPackageTaskArgs,
};
use sui_pg_db::{Db, DbArgs};
use sui_transactional_test_runner::{
    create_adapter,
    offchain_state::{OffchainStateReader, TestResponse},
    run_tasks_with_adapter,
    test_adapter::{OffChainConfig, SuiTestAdapter, PRE_COMPILED},
};
use tokio_util::sync::CancellationToken;
use url::Url;

struct OffchainReader {
    db: Db,
    rpc_url: Url,
    client: Client,
    queries: AtomicUsize,
}

datatest_stable::harness!(run_test, "tests", r".*\.move$");

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

async fn cluster(config: &OffChainConfig) -> OffchainCluster {
    let cancel = CancellationToken::new();
    let registry = Registry::new();

    let indexer_args = IndexerArgs::default();

    let client_args = ClientArgs {
        local_ingestion_path: Some(config.data_ingestion_path.clone()),
        remote_store_url: None,
    };

    // This configuration controls how often the RPC service checks for changes to system packages.
    // The default polling interval is probably too slow for changes to get picked up, so tests
    // that rely on this behaviour will always fail, but this is better than flaky behavior.
    let system_package_task_args = SystemPackageTaskArgs::default();

    // The example config includes every pipeline, and we configure its consistent range using the
    // off-chain config that was passed in.
    let mut indexer_config = IndexerConfig::example();
    indexer_config.consistency.retention = Some(config.snapshot_config.snapshot_min_lag as u64);

    let rpc_config = RpcConfig::example();

    OffchainCluster::new(
        indexer_args,
        client_args,
        system_package_task_args,
        indexer_config,
        rpc_config,
        &registry,
        cancel,
    )
    .await
    .expect("Failed to create off-chain cluster")
}

async fn reader(cluster: &OffchainCluster) -> Box<dyn OffchainStateReader> {
    let db = Db::for_read(DbArgs {
        database_url: cluster.db_url(),
        ..Default::default()
    })
    .await
    .expect("Failed to connect to database");

    Box::new(OffchainReader {
        db,
        rpc_url: cluster.rpc_url(),
        client: Client::new(),
        queries: AtomicUsize::new(0),
    })
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
    let c = cluster(adapter.offchain_config.as_ref().unwrap()).await;
    adapter.with_offchain_reader(reader(&c).await);

    // run the tasks in the test
    run_tasks_with_adapter(path, adapter, output).await?;

    // clean-up the off-chain cluster
    c.stopped().await;
    Ok(())
}
