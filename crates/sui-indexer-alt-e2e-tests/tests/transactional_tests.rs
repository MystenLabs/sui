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

use anyhow::{bail, Context};
use prometheus::Registry;
use reqwest::Client;
use serde_json::{json, Value};
use sui_indexer_alt::config::{IndexerConfig, Merge, PrunerLayer};
use sui_indexer_alt_e2e_tests::OffchainCluster;
use sui_indexer_alt_framework::{ingestion::ClientArgs, IndexerArgs};
use sui_indexer_alt_jsonrpc::{
    config::RpcConfig, data::system_package_task::SystemPackageTaskArgs,
};
use sui_transactional_test_runner::{
    create_adapter,
    offchain_state::{OffchainStateReader, TestResponse},
    run_tasks_with_adapter,
    test_adapter::{OffChainConfig, SuiTestAdapter, PRE_COMPILED},
};
use tokio_util::sync::CancellationToken;

struct OffchainReader {
    cluster: Arc<OffchainCluster>,
    client: Client,
    queries: AtomicUsize,
}

datatest_stable::harness!(run_test, "tests", r".*\.move$");

impl OffchainReader {
    fn new(cluster: Arc<OffchainCluster>) -> Self {
        Self {
            cluster,
            client: Client::new(),
            queries: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl OffchainStateReader for OffchainReader {
    async fn wait_for_checkpoint_catchup(&self, checkpoint: u64, base_timeout: Duration) {
        let _ = self
            .cluster
            .wait_for_checkpoint(checkpoint, base_timeout)
            .await;
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
            .post(self.cluster.rpc_url())
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

async fn cluster(config: &OffChainConfig) -> Arc<OffchainCluster> {
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

    // The test config includes every pipeline, we configure its consistent range using the
    // off-chain config that was passed in.
    let indexer_config = IndexerConfig::for_test().merge(IndexerConfig {
        consistency: PrunerLayer {
            retention: Some(config.snapshot_config.snapshot_min_lag as u64),
            ..Default::default()
        },
        ..Default::default()
    });

    let rpc_config = RpcConfig::example();

    Arc::new(
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
        .expect("Failed to create off-chain cluster"),
    )
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
    adapter.with_offchain_reader(Box::new(OffchainReader::new(c.clone())));

    // run the tasks in the test
    run_tasks_with_adapter(path, adapter, output).await?;

    // clean-up the off-chain cluster
    Arc::try_unwrap(c)
        .unwrap_or_else(|_| panic!("Failed to unwrap off-chain cluster"))
        .stopped()
        .await;

    Ok(())
}
