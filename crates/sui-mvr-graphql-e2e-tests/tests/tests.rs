// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(unused_imports)]
#![allow(unused_variables)]
use anyhow::bail;
use async_trait::async_trait;
use serde_json::Value;
use std::{path::Path, sync::Arc, time::Duration};
use sui_mvr_graphql_rpc::test_infra::cluster::{serve_executor, ExecutorCluster};
use sui_transactional_test_runner::{
    args::SuiInitArgs,
    create_adapter,
    offchain_state::{OffchainStateReader, TestResponse},
    run_tasks_with_adapter,
    test_adapter::{SuiTestAdapter, PRE_COMPILED},
};

pub struct OffchainReaderForAdapter {
    cluster: Arc<ExecutorCluster>,
}

#[async_trait]
impl OffchainStateReader for OffchainReaderForAdapter {
    async fn wait_for_checkpoint_catchup(&self, checkpoint: u64, base_timeout: Duration) {
        self.cluster
            .wait_for_checkpoint_catchup(checkpoint, base_timeout)
            .await
    }

    async fn wait_for_pruned_checkpoint(&self, checkpoint: u64, base_timeout: Duration) {
        self.cluster
            .wait_for_checkpoint_pruned(checkpoint, base_timeout)
            .await
    }

    async fn execute_graphql(
        &self,
        query: String,
        show_usage: bool,
    ) -> Result<TestResponse, anyhow::Error> {
        let result = self
            .cluster
            .graphql_client
            .execute_to_graphql(query, show_usage, vec![], vec![])
            .await?;

        Ok(TestResponse {
            http_headers: Some(result.http_headers_without_date()),
            response_body: result.response_body_json_pretty(),
            service_version: result.graphql_version().ok(),
        })
    }

    async fn execute_jsonrpc(&self, _: String, _: Value) -> anyhow::Result<TestResponse> {
        bail!("JSON-RPC queries are not supported in these tests")
    }
}

datatest_stable::harness!(run_test, "tests", r"stable/.*\.move$");

#[cfg_attr(not(msim), tokio::main)]
#[cfg_attr(msim, msim::main)]
async fn run_test(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    telemetry_subscribers::init_for_testing();
    if !cfg!(msim) {
        // start the adapter first to start the executor (simulacrum)
        let (output, mut adapter) =
            create_adapter::<SuiTestAdapter>(path, Some(Arc::new(PRE_COMPILED.clone()))).await?;

        let offchain_config = adapter.offchain_config.as_ref().unwrap();

        let cluster = serve_executor(
            adapter.read_replica.as_ref().unwrap().clone(),
            Some(offchain_config.snapshot_config.clone()),
            offchain_config.retention_config.clone(),
            offchain_config.data_ingestion_path.clone(),
        )
        .await;

        let cluster_arc = Arc::new(cluster);

        adapter.with_offchain_reader(Box::new(OffchainReaderForAdapter {
            cluster: cluster_arc.clone(),
        }));

        run_tasks_with_adapter(path, adapter, output).await?;

        match Arc::try_unwrap(cluster_arc) {
            Ok(cluster) => cluster.cleanup_resources().await,
            Err(_) => panic!("Still other Arc references!"),
        }
    }
    Ok(())
}
