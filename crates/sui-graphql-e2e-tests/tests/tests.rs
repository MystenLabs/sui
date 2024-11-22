// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(unused_imports)]
#![allow(unused_variables)]
use std::{path::Path, sync::Arc};
use sui_graphql_rpc::test_infra::cluster::serve_executor;
use sui_transactional_test_runner::{
    args::SuiInitArgs,
    create_adapter, run_tasks_with_adapter,
    test_adapter::{SuiTestAdapter, PRE_COMPILED},
};

datatest_stable::harness!(
    run_test,
    "tests",
    if cfg!(feature = "staging") {
        r"\.move$"
    } else {
        r"stable/.*\.move$"
    }
);

#[cfg_attr(not(msim), tokio::main)]
#[cfg_attr(msim, msim::main)]
async fn run_test(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    telemetry_subscribers::init_for_testing();
    if !cfg!(msim) {
        // extract init args

        // if test wants to provide data-ingestion-path and rest-api-url to testadapter...this seems harder
        // easier for testadapter to tell indexer where it's writing to

        // then initialize the adapter

        // snapshotconfig and retentionconfig ofc
        // serve_executor(adapter.read_replica, init_opt..., init_opt..., adapter.data_ingestion_path).await?;

        // start the adapter first to start the executor
        let (output, mut adapter) =
            create_adapter::<SuiTestAdapter>(path, Some(Arc::new(PRE_COMPILED.clone()))).await?;

        let cluster = serve_executor(
            adapter.read_replica.as_ref().unwrap().clone(),
            None,
            None,
            adapter
                .offchain_config
                .as_ref()
                .unwrap()
                .data_ingestion_path
                .clone(),
        )
        .await;

        adapter.with_graphql_rpc(format!(
            "http://{}:{}",
            cluster.graphql_connection_config.host, cluster.graphql_connection_config.port
        ));

        // serve_executor, which is kind of a misnomer since it takes the read replica
        run_tasks_with_adapter(path, adapter, output).await?;

        // and then cleanup
    }
    Ok(())
}
