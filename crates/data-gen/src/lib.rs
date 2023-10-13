// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Spin up a local Sui simulator and run a transactional test against it.
// Spin up an indexer to read data from the simulator
// Spin up a GraphQL server to serve data from the indexer DB
// Spin up a GraphQL client to query data from the GraphQL server

use std::{net::SocketAddr, path::Path};
mod simulator_runner;
use move_transactional_test_runner::framework::handle_actual_output;
use simulator_runner::test_adapter::{DataGenAdapter, PRE_COMPILED};
use sui_graphql_rpc::client::simple_client::SimpleClient;
use sui_graphql_rpc::config::ConnectionConfig;
use sui_graphql_rpc::server::simple_server::start_example_server;

use sui_indexer::errors::IndexerError;
use sui_indexer::store::IndexerStore;
use sui_indexer::store::PgIndexerStore;
use tokio::task::JoinHandle;

pub const TEST_DIR: &str = "tests";
const WAIT_UNTIL_TIME_LIMIT: u64 = 60;

#[test]
fn testx() {
    datatest_stable::harness!(run_test, TEST_DIR, r".*\.(mvir|move)$");
}
#[cfg_attr(not(msim), tokio::main)]
#[cfg_attr(msim, msim::main)]
pub async fn run_test(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let (output, adapter) = handle_actual_output::<DataGenAdapter>(path, Some(&*PRE_COMPILED)).await?;
    Ok(())
}

async fn start_indexer(
    db_url: String,
    executor_address: SocketAddr,
    rpc_server_address: SocketAddr,
    wait_for_sync: bool,
) -> (PgIndexerStore, JoinHandle<Result<(), IndexerError>>) {
    let indexer_cfg = sui_indexer::IndexerConfig {
        db_url: Some(db_url),
        rpc_client_url: executor_address.to_string(),
        rpc_server_url: rpc_server_address.ip().to_string(),
        rpc_server_port: rpc_server_address.port(),
        reset_db: true,
        ..Default::default()
    };

    tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;
    let (store, handle) = sui_indexer::test_utils::start_test_indexer(indexer_cfg)
        .await
        .unwrap();

    // Allow indexer to sync
    if wait_for_sync {
        wait_until_next_checkpoint(&store).await;
    }
    (store, handle)
}

async fn wait_until_next_checkpoint(store: &sui_indexer::store::PgIndexerStore) {
    let since = std::time::Instant::now();
    let mut cp_res = store.get_latest_tx_checkpoint_sequence_number().await;
    while cp_res.is_err() {
        cp_res = store.get_latest_tx_checkpoint_sequence_number().await;
    }
    let mut cp = cp_res.unwrap();
    let target = cp + 1;
    while cp < target {
        let now = std::time::Instant::now();
        if now.duration_since(since).as_secs() > WAIT_UNTIL_TIME_LIMIT {
            panic!("wait_until_next_checkpoint timed out!");
        }
        tokio::task::yield_now().await;
        let mut cp_res = store.get_latest_tx_checkpoint_sequence_number().await;
        while cp_res.is_err() {
            cp_res = store.get_latest_tx_checkpoint_sequence_number().await;
        }
        cp = cp_res.unwrap();
    }
}

async fn start_graphql_server(db_url: String, server_bind_address: SocketAddr) {
    start_example_server(
        ConnectionConfig::new(
            Some(server_bind_address.port()),
            Some(server_bind_address.ip().to_string()),
            Some(db_url),
            None,
        ),
        None,
    )
    .await;
}

async fn get_graphql_client(graphql_server_address: SocketAddr) -> SimpleClient {
    SimpleClient::new(graphql_server_address.to_string())
}
