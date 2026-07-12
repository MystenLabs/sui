// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::IndexerConfig;
use crate::eth_bridge_indexer::{
    EthDataMapper, EthFinalizedSyncDatasource, EthSubscriptionDatasource,
};
use crate::metrics::BridgeIndexerMetrics;
use crate::postgres_manager::PgPool;
use crate::storage::PgBridgePersistent;
use alloy::primitives::Address as EthAddress;
use std::str::FromStr;
use std::sync::Arc;
use sui_bridge::eth_client::EthClient;
use sui_bridge::metrics::BridgeMetrics;
use sui_bridge::utils::{get_eth_contract_addresses, get_eth_provider};
use sui_bridge_schema::models::{
    BridgeDataSource, GovernanceAction as DBGovernanceAction, TokenTransferStatus,
};
use sui_bridge_schema::models::{GovernanceActionType, TokenTransferData as DBTokenTransferData};
use sui_bridge_schema::models::{SuiErrorTransactions, TokenTransfer as DBTokenTransfer};
use sui_types::base_types::{SuiAddress, TransactionDigest};

pub mod config;
pub mod metrics;
pub mod postgres_manager;
mod storage;

mod eth_bridge_indexer;

use indexer_builder::{BackfillStrategy, Datasource, Indexer, IndexerBuilder};
use metrics::IndexerMetricProvider;
use progress::{ProgressSavingPolicy, SaveAfterDurationPolicy};

#[derive(Clone)]
pub enum ProcessedTxnData {
    TokenTransfer(TokenTransfer),
    GovernanceAction(GovernanceAction),
    Error(SuiTxnError),
}

#[derive(Clone)]
pub struct SuiTxnError {
    tx_digest: TransactionDigest,
    sender: SuiAddress,
    timestamp_ms: u64,
    failure_status: String,
    cmd_idx: Option<u64>,
}

#[derive(Clone)]
pub struct TokenTransfer {
    chain_id: u8,
    nonce: u64,
    block_height: u64,
    timestamp_ms: u64,
    txn_hash: Vec<u8>,
    txn_sender: Vec<u8>,
    status: TokenTransferStatus,
    gas_usage: i64,
    data_source: BridgeDataSource,
    data: Option<TokenTransferData>,
    is_finalized: bool,
}

#[derive(Clone)]
pub struct GovernanceAction {
    nonce: Option<u64>,
    data_source: BridgeDataSource,
    tx_digest: Vec<u8>,
    sender: Vec<u8>,
    timestamp_ms: u64,
    action: GovernanceActionType,
    data: serde_json::Value,
}

#[derive(Clone)]
pub struct TokenTransferData {
    sender_address: Vec<u8>,
    destination_chain: u8,
    recipient_address: Vec<u8>,
    token_id: u8,
    amount: u64,
    is_finalized: bool,
    /// For V2 transfers, the timestamp (in ms) from the bridge message payload.
    /// `None` for V1 transfers.
    message_timestamp_ms: Option<u64>,
}

impl TokenTransfer {
    fn to_db(&self) -> DBTokenTransfer {
        DBTokenTransfer {
            chain_id: self.chain_id as i32,
            nonce: self.nonce as i64,
            block_height: self.block_height as i64,
            timestamp_ms: self.timestamp_ms as i64,
            txn_hash: self.txn_hash.clone(),
            txn_sender: self.txn_sender.clone(),
            status: self.status,
            gas_usage: self.gas_usage,
            data_source: self.data_source,
            is_finalized: self.is_finalized,
        }
    }

    fn to_data_maybe(&self) -> Option<DBTokenTransferData> {
        self.data.as_ref().map(|data| DBTokenTransferData {
            chain_id: self.chain_id as i32,
            nonce: self.nonce as i64,
            block_height: self.block_height as i64,
            timestamp_ms: self.timestamp_ms as i64,
            txn_hash: self.txn_hash.clone(),
            sender_address: data.sender_address.clone(),
            destination_chain: data.destination_chain as i32,
            recipient_address: data.recipient_address.clone(),
            token_id: data.token_id as i32,
            amount: data.amount as i64,
            is_finalized: data.is_finalized,
            message_timestamp_ms: data.message_timestamp_ms.map(|ts| ts as i64),
        })
    }
}

impl SuiTxnError {
    fn to_db(&self) -> SuiErrorTransactions {
        SuiErrorTransactions {
            txn_digest: self.tx_digest.inner().to_vec(),
            sender_address: self.sender.to_vec(),
            timestamp_ms: self.timestamp_ms as i64,
            failure_status: self.failure_status.clone(),
            cmd_idx: self.cmd_idx.map(|idx| idx as i64),
        }
    }
}

impl GovernanceAction {
    fn to_db(&self) -> DBGovernanceAction {
        DBGovernanceAction {
            nonce: self.nonce.map(|nonce| nonce as i64),
            data_source: self.data_source,
            txn_digest: self.tx_digest.clone(),
            sender_address: self.sender.to_vec(),
            timestamp_ms: self.timestamp_ms as i64,
            action: self.action,
            data: self.data.clone(),
        }
    }
}

pub async fn create_eth_sync_indexer(
    pool: PgPool,
    metrics: BridgeIndexerMetrics,
    bridge_metrics: Arc<BridgeMetrics>,
    config: &IndexerConfig,
    eth_client: Arc<EthClient>,
) -> Result<Indexer<PgBridgePersistent, EthFinalizedSyncDatasource, EthDataMapper>, anyhow::Error> {
    let bridge_addresses = get_eth_bridge_contract_addresses(config).await?;
    // Start the eth sync data source
    let eth_sync_datasource = EthFinalizedSyncDatasource::new(
        bridge_addresses,
        eth_client.clone(),
        config.eth_rpc_url.clone(),
        metrics.clone().boxed(),
        bridge_metrics.clone(),
        config.eth_bridge_genesis_block,
    )
    .await?;
    Ok(create_eth_indexer_builder(
        pool,
        metrics,
        eth_sync_datasource,
        "EthBridgeFinalizedSyncIndexer",
    )
    .await?
    .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 1000 })
    .build())
}

pub async fn create_eth_subscription_indexer(
    pool: PgPool,
    metrics: BridgeIndexerMetrics,
    config: &IndexerConfig,
    eth_client: Arc<EthClient>,
) -> Result<Indexer<PgBridgePersistent, EthSubscriptionDatasource, EthDataMapper>, anyhow::Error> {
    // Start the eth subscription indexer
    let bridge_addresses = get_eth_bridge_contract_addresses(config).await?;
    // Start the eth subscription indexer
    let eth_subscription_datasource = EthSubscriptionDatasource::new(
        bridge_addresses.clone(),
        eth_client.clone(),
        config.eth_ws_url.clone(),
        metrics.clone().boxed(),
        config.eth_bridge_genesis_block,
    )
    .await?;

    Ok(create_eth_indexer_builder(
        pool,
        metrics,
        eth_subscription_datasource,
        "EthBridgeSubscriptionIndexer",
    )
    .await?
    .with_backfill_strategy(BackfillStrategy::Disabled)
    .build())
}

async fn create_eth_indexer_builder<T: Send, D: Datasource<T>>(
    pool: PgPool,
    metrics: BridgeIndexerMetrics,
    datasource: D,
    indexer_name: &str,
) -> Result<IndexerBuilder<D, EthDataMapper, PgBridgePersistent>, anyhow::Error> {
    let datastore = PgBridgePersistent::new(
        pool,
        ProgressSavingPolicy::SaveAfterDuration(SaveAfterDurationPolicy::new(
            tokio::time::Duration::from_secs(30),
        )),
    );

    // Start the eth subscription indexer
    Ok(IndexerBuilder::new(
        indexer_name,
        datasource,
        EthDataMapper { metrics },
        datastore.clone(),
    ))
}

async fn get_eth_bridge_contract_addresses(
    config: &IndexerConfig,
) -> Result<Vec<EthAddress>, anyhow::Error> {
    let bridge_address = EthAddress::from_str(&config.eth_sui_bridge_contract_address)?;
    let eth_provider = get_eth_provider(&config.eth_rpc_url)?;
    let bridge_addresses = get_eth_contract_addresses(bridge_address, eth_provider).await?;
    Ok(vec![
        bridge_address,
        bridge_addresses.0,
        bridge_addresses.1,
        bridge_addresses.2,
        bridge_addresses.3,
    ])
}

// inline old sui-indexer-builder

mod indexer_builder;
mod progress;
const LIVE_TASK_TARGET_CHECKPOINT: i64 = i64::MAX;

#[derive(Clone, Debug)]
pub struct Task {
    pub task_name: String,
    pub start_checkpoint: u64,
    pub target_checkpoint: u64,
    pub timestamp: u64,
    pub is_live_task: bool,
}

impl Task {
    // TODO: this is really fragile and we should fix the task naming thing and storage schema asasp
    pub fn name_prefix(&self) -> &str {
        self.task_name.split(' ').next().unwrap_or("Unknown")
    }

    pub fn type_str(&self) -> &str {
        if self.is_live_task {
            "live"
        } else {
            "backfill"
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tasks {
    live_task: Option<Task>,
    backfill_tasks: Vec<Task>,
}

impl Tasks {
    pub fn new(tasks: Vec<Task>) -> anyhow::Result<Self> {
        let mut live_tasks = vec![];
        let mut backfill_tasks = vec![];
        for task in tasks {
            if task.is_live_task {
                live_tasks.push(task);
            } else {
                backfill_tasks.push(task);
            }
        }
        if live_tasks.len() > 1 {
            anyhow::bail!("More than one live task found: {:?}", live_tasks);
        }
        Ok(Self {
            live_task: live_tasks.pop(),
            backfill_tasks,
        })
    }

    pub fn live_task(&self) -> Option<Task> {
        self.live_task.clone()
    }

    pub fn backfill_tasks_ordered_desc(&self) -> Vec<Task> {
        let mut tasks = self.backfill_tasks.clone();
        tasks.sort_by(|t1, t2| t2.start_checkpoint.cmp(&t1.start_checkpoint));
        tasks
    }
}
