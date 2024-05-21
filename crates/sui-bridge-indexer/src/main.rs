// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use ethers::types::Address as EthAddress;
use prometheus::Registry;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use sui_bridge::{
    eth_client::EthClient,
    eth_syncer::EthSyncer,
    indexer::{config::BridgeIndexerConfig, worker::BridgeWorker},
};
use sui_data_ingestion_core::{
    DataIngestionMetrics, FileProgressStore, IndexerExecutor, ReaderOptions, WorkerPool,
};
use tokio::sync::oneshot;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let config = BridgeIndexerConfig::parse();
    info!("Parsed config: {:#?}", config);

    // start sui side
    let (_exit_sender, exit_receiver) = oneshot::channel();
    let metrics = DataIngestionMetrics::new(&Registry::new());
    let progress_store = FileProgressStore::new(config.progress_store_file);
    let mut executor = IndexerExecutor::new(progress_store, 1 /* workflow types */, metrics);
    let worker_pool = WorkerPool::new(
        BridgeWorker::new(vec![], config.db_url),
        "bridge worker".into(),
        config.concurrency,
    );
    executor.register(worker_pool).await?;
    executor
        .run(
            config.checkpoints_path,
            config.remote_store_url,
            vec![], // optional remote store access options
            ReaderOptions::default(),
            exit_receiver,
        )
        .await?;

    // start eth side
    let eth_client = Arc::new(
        EthClient::<ethers::providers::Http>::new(
            &config.eth_rpc_url,
            HashSet::from_iter(vec![
                // Define in config?
                // bridge_proxy_address,
                // committee_address,
                // config_address,
                // limiter_address,
                // vault_address,
            ]),
        )
        .await?,
    );
    let contract_addresses: HashMap<EthAddress, u64> = HashMap::new();
    let mut all_handles = vec![];
    let (task_handles, _eth_events_rx, _) = EthSyncer::new(eth_client, contract_addresses)
        .run()
        .await
        .expect("Failed to start eth syncer");
    all_handles.extend(task_handles);

    Ok(())
}
