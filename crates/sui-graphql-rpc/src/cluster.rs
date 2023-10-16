// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::ConnectionConfig;
use crate::config::ServiceConfig;
use crate::server::simple_server::start_example_server;
use std::env;
use std::net::SocketAddr;
use sui_indexer::errors::IndexerError;
use sui_indexer::store::IndexerStore;
use sui_indexer::store::PgIndexerStore;
use sui_indexer::IndexerConfig;
use sui_swarm_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tokio::task::JoinHandle;

const _WAIT_UNTIL_TIME_LIMIT: u64 = 60;
const _VALIDATOR_COUNT: usize = 7;
const _EPOCH_DURATION_MS: u64 = 15000;

const _ACCOUNT_NUM: usize = 20;
const _GAS_OBJECT_COUNT: usize = 3;

async fn _start(db_url: String) {
    // Starts validator+fullnode+indexer cluster
    let (val_fn, _pg_store, _pg_handle, db_url) = _start_test_cluster(Some(db_url), true).await;

    // Starts graphql server
    let conn = ConnectionConfig::new(
        None,
        None,
        Some(val_fn.rpc_url().to_string()),
        Some(db_url),
        None,
        None,
    );
    start_example_server(conn, ServiceConfig::default()).await;
}

async fn _start_indexer(
    db_url: String,
    executor_address: SocketAddr,
    wait_for_sync: bool,
) -> (PgIndexerStore, JoinHandle<Result<(), IndexerError>>) {
    let indexer_cfg = sui_indexer::IndexerConfig {
        db_url: Some(db_url),
        rpc_client_url: executor_address.to_string(),
        reset_db: true,
        fullnode_sync_worker: true,
        ..Default::default()
    };

    tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;
    let (store, handle) = sui_indexer::test_utils::start_test_indexer(indexer_cfg)
        .await
        .unwrap();

    // Allow indexer to sync
    if wait_for_sync {
        _wait_until_next_checkpoint(&store).await;
    }
    (store, handle)
}

async fn _wait_until_next_checkpoint(store: &sui_indexer::store::PgIndexerStore) {
    let since = std::time::Instant::now();
    let mut cp_res = store.get_latest_tx_checkpoint_sequence_number().await;
    while cp_res.is_err() {
        cp_res = store.get_latest_tx_checkpoint_sequence_number().await;
    }
    let mut cp = cp_res.unwrap();
    let target = cp + 1;
    while cp < target {
        let now = std::time::Instant::now();
        if now.duration_since(since).as_secs() > _WAIT_UNTIL_TIME_LIMIT {
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

async fn _start_test_cluster(
    db_url: Option<String>,
    wait_for_sync: bool,
) -> (
    TestCluster,
    PgIndexerStore,
    JoinHandle<Result<(), IndexerError>>,
    String,
) {
    let db_url = db_url.unwrap_or_else(|| {
        let pg_host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".into());
        let pg_port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "32770".into());
        let pw = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgrespw".into());
        format!("postgres://postgres:{pw}@{pg_host}:{pg_port}")
    });

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(_VALIDATOR_COUNT)
        .with_epoch_duration_ms(_EPOCH_DURATION_MS)
        .with_accounts(vec![
            AccountConfig {
                address: None,
                gas_amounts: vec![DEFAULT_GAS_AMOUNT; _GAS_OBJECT_COUNT],
            };
            _ACCOUNT_NUM
        ])
        .build()
        .await;

    let config = IndexerConfig {
        db_url: Some(db_url.clone()),
        rpc_client_url: test_cluster.rpc_url().to_string(),
        migrated_methods: IndexerConfig::all_implemented_methods(),
        reset_db: true,
        fullnode_sync_worker: true,
        ..Default::default()
    };

    let (store, handle) = sui_indexer::test_utils::start_test_indexer(config)
        .await
        .unwrap();
    if wait_for_sync {
        _wait_until_next_checkpoint(&store).await;
    }

    (test_cluster, store, handle, db_url)
}
