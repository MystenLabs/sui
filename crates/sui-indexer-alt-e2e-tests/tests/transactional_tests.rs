// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    error::Error,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::Context;
use reqwest::{Client, header::HeaderName};
use serde_json::{Value, json};
use sui_indexer_alt::config::{ConcurrentLayer, IndexerConfig, Merge, PipelineLayer, PrunerLayer};
use sui_indexer_alt_e2e_tests::{OffchainCluster, OffchainClusterConfig};
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_transactional_test_runner::{
    create_adapter,
    offchain_state::{OffchainStateReader, TestResponse},
    run_tasks_with_adapter,
    test_adapter::{OffChainConfig, PRE_COMPILED, SuiTestAdapter},
};
use tokio::join;
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
        let indexer = self.cluster.wait_for_indexer(checkpoint, base_timeout);
        let consistent_store = self
            .cluster
            .wait_for_consistent_store(checkpoint, base_timeout);
        let graphql = self.cluster.wait_for_graphql(checkpoint, base_timeout);
        let _ = join!(indexer, consistent_store, graphql);
    }

    async fn wait_for_pruned_checkpoint(&self, _: u64, _: Duration) {
        unimplemented!(
            "Waiting for pruned checkpoints is not supported in these tests (add it if you need it)"
        );
    }

    async fn execute_graphql(
        &self,
        query: String,
        show_usage: bool,
    ) -> anyhow::Result<TestResponse> {
        let query = json!({ "query": query });

        let mut request = self.client.post(self.cluster.graphql_url()).json(&query);

        if show_usage {
            request = request.header(HeaderName::from_static("x-sui-rpc-show-usage"), "true");
        }

        let response = request
            .send()
            .await
            .context("Request to GraphQL server failed")?;

        let mut headers = response.headers().clone();
        headers.remove("date");

        let version = headers
            .remove("x-sui-rpc-version")
            .and_then(|v| v.to_str().ok().map(|s| s.to_owned()));

        let body: Value = response
            .json()
            .await
            .context("Failed to parse GraphQL response")?;

        Ok(TestResponse {
            response_body: serde_json::to_string_pretty(&body)?,
            http_headers: Some(headers),
            service_version: version,
        })
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
            .post(self.cluster.jsonrpc_url())
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
    let client_args = ClientArgs {
        local_ingestion_path: Some(config.data_ingestion_path.clone()),
        ..Default::default()
    };

    // The test config includes every pipeline, we configure its consistent range using the
    // off-chain config that was passed in.
    let pruner = PrunerLayer {
        retention: Some(config.snapshot_config.snapshot_min_lag as u64),
        ..Default::default()
    };

    let indexer_config = IndexerConfig::for_test()
        .merge(IndexerConfig {
            pipeline: PipelineLayer {
                coin_balance_buckets: Some(ConcurrentLayer {
                    pruner: Some(pruner.clone()),
                    ..Default::default()
                }),
                obj_info: Some(ConcurrentLayer {
                    pruner: Some(pruner),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
        .expect("Failed to create indexer config");

    Arc::new(
        OffchainCluster::new(
            client_args,
            OffchainClusterConfig {
                indexer_config,
                ..Default::default()
            },
            &prometheus::Registry::new(),
            CancellationToken::new(),
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
    run_tasks_with_adapter(path, adapter, output, None).await?;

    // clean-up the off-chain cluster
    Arc::try_unwrap(c)
        .unwrap_or_else(|_| panic!("Failed to unwrap off-chain cluster"))
        .stopped()
        .await;

    Ok(())
}
