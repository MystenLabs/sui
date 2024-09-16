// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Error;
use async_trait::async_trait;
use ethers::prelude::Transaction;
use ethers::providers::{Http, Middleware, Provider, StreamExt, Ws};
use ethers::types::{Address as EthAddress, Block, Filter, H256};
use prometheus::{IntCounterVec, IntGaugeVec};
use sui_bridge::error::BridgeError;
use sui_bridge::eth_client::EthClient;
use sui_bridge::eth_syncer::EthSyncer;
use sui_bridge::metered_eth_provider::MeteredEthHttpProvier;
use sui_bridge::retry_with_max_elapsed_time;
use sui_indexer_builder::Task;
use tokio::task::JoinHandle;
use tracing::info;

use mysten_metrics::spawn_monitored_task;
use sui_bridge::abi::{EthBridgeEvent, EthSuiBridgeEvents};

use crate::metrics::BridgeIndexerMetrics;
use sui_bridge::metrics::BridgeMetrics;
use sui_bridge::types::{EthEvent, RawEthLog};
use sui_indexer_builder::indexer_builder::{DataMapper, DataSender, Datasource};

use crate::{
    BridgeDataSource, ProcessedTxnData, TokenTransfer, TokenTransferData, TokenTransferStatus,
};

pub struct RawEthData {
    log: RawEthLog,
    block: Block<H256>,
    transaction: Transaction,
    is_finalized: bool,
}

// Create max log query range
const MAX_LOG_QUERY_RANGE: u64 = 1000;
pub struct EthSubscriptionDatasource {
    eth_client: Arc<EthClient<MeteredEthHttpProvier>>,
    addresses: Vec<EthAddress>,
    eth_ws_url: String,
    indexer_metrics: BridgeIndexerMetrics,
    genesis_block: u64,
}

impl EthSubscriptionDatasource {
    pub async fn new(
        eth_sui_bridge_contract_addresses: Vec<EthAddress>,
        eth_client: Arc<EthClient<MeteredEthHttpProvier>>,
        eth_ws_url: String,
        indexer_metrics: BridgeIndexerMetrics,
        genesis_block: u64,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            addresses: eth_sui_bridge_contract_addresses,
            eth_client,
            eth_ws_url,
            indexer_metrics,
            genesis_block,
        })
    }
}
#[async_trait]
impl Datasource<RawEthData> for EthSubscriptionDatasource {
    async fn start_data_retrieval(
        &self,
        task: Task,
        data_sender: DataSender<RawEthData>,
    ) -> Result<JoinHandle<Result<(), Error>>, Error> {
        let filter = Filter::new()
            .address(self.addresses.clone())
            .from_block(task.start_checkpoint)
            .to_block(task.target_checkpoint);

        let eth_ws_url = self.eth_ws_url.clone();

        let handle = spawn_monitored_task!(async move {
            let eth_ws_client = Provider::<Ws>::connect(&eth_ws_url).await?;

            // TODO: enable a shared cache for blocks that can be used by both the subscription and finalized sync
            let mut cached_blocks: HashMap<u64, Block<H256>> = HashMap::new();

            let mut stream = eth_ws_client.subscribe_logs(&filter).await?;
            while let Some(log) = stream.next().await {
                let raw_log = RawEthLog {
                    block_number: log
                        .block_number
                        .ok_or(BridgeError::ProviderError(
                            "Provider returns log without block_number".into(),
                        ))
                        .unwrap()
                        .as_u64(),
                    tx_hash: log
                        .transaction_hash
                        .ok_or(BridgeError::ProviderError(
                            "Provider returns log without transaction_hash".into(),
                        ))
                        .unwrap(),
                    log,
                };

                let block_number = raw_log.block_number();

                let block = if let Some(cached_block) = cached_blocks.get(&block_number) {
                    cached_block.clone()
                } else {
                    let Ok(Ok(Some(block))) = retry_with_max_elapsed_time!(
                        eth_ws_client.get_block(block_number),
                        Duration::from_secs(30000)
                    ) else {
                        panic!("Unable to get block from provider");
                    };

                    cached_blocks.insert(block_number, block.clone());
                    block
                };

                let Ok(Ok(Some(transaction))) = retry_with_max_elapsed_time!(
                    eth_ws_client.get_transaction(raw_log.tx_hash),
                    Duration::from_secs(30000)
                ) else {
                    panic!("Unable to get transaction from provider");
                };

                data_sender
                    .send((
                        block_number,
                        vec![RawEthData {
                            log: raw_log,
                            block,
                            transaction,
                            is_finalized: false,
                        }],
                    ))
                    .await?;
            }

            Ok::<_, Error>(())
        });
        Ok(handle)
    }

    async fn get_live_task_starting_checkpoint(&self) -> Result<u64, Error> {
        self.eth_client
            .get_last_finalized_block_id()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get last finalized block id: {:?}", e))
    }

    fn get_genesis_height(&self) -> u64 {
        self.genesis_block
    }

    fn get_tasks_remaining_checkpoints_metric(&self) -> &IntGaugeVec {
        &self.indexer_metrics.backfill_tasks_remaining_checkpoints
    }

    fn get_tasks_processed_checkpoints_metric(&self) -> &IntCounterVec {
        &self.indexer_metrics.tasks_processed_checkpoints
    }
}

pub struct EthFinalizedSyncDatasource {
    bridge_addresses: Vec<EthAddress>,
    eth_http_url: String,
    eth_client: Arc<EthClient<MeteredEthHttpProvier>>,
    indexer_metrics: BridgeIndexerMetrics,
    bridge_metrics: Arc<BridgeMetrics>,
    genesis_block: u64,
}

impl EthFinalizedSyncDatasource {
    pub async fn new(
        eth_sui_bridge_contract_addresses: Vec<EthAddress>,
        eth_client: Arc<EthClient<MeteredEthHttpProvier>>,
        eth_http_url: String,
        indexer_metrics: BridgeIndexerMetrics,
        bridge_metrics: Arc<BridgeMetrics>,
        genesis_block: u64,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            bridge_addresses: eth_sui_bridge_contract_addresses,
            eth_http_url,
            eth_client,
            indexer_metrics,
            bridge_metrics,
            genesis_block,
        })
    }
}
#[async_trait]
impl Datasource<RawEthData> for EthFinalizedSyncDatasource {
    async fn start_data_retrieval(
        &self,
        task: Task,
        data_sender: DataSender<RawEthData>,
    ) -> Result<JoinHandle<Result<(), Error>>, Error> {
        let provider = Arc::new(
            Provider::<Http>::try_from(&self.eth_http_url)?
                .interval(std::time::Duration::from_millis(2000)),
        );

        let bridge_addresses = self.bridge_addresses.clone();
        let client = self.eth_client.clone();
        let provider = provider.clone();
        let bridge_metrics = self.bridge_metrics.clone();

        let handle = spawn_monitored_task!(async move {
            if task.is_live_task {
                retrieve_and_process_live_finalized_logs(
                    client,
                    provider,
                    bridge_addresses,
                    task.start_checkpoint,
                    data_sender,
                    bridge_metrics,
                )
                .await?;
            } else {
                retrieve_and_process_log_range(
                    client,
                    provider,
                    bridge_addresses,
                    task.start_checkpoint,
                    task.target_checkpoint,
                    data_sender,
                )
                .await?;
            }
            Ok::<_, Error>(())
        });

        Ok(handle)
    }

    async fn get_live_task_starting_checkpoint(&self) -> Result<u64, Error> {
        self.eth_client
            .get_last_finalized_block_id()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get last finalized block id: {:?}", e))
    }

    fn get_genesis_height(&self) -> u64 {
        self.genesis_block
    }

    fn get_tasks_remaining_checkpoints_metric(&self) -> &IntGaugeVec {
        &self.indexer_metrics.backfill_tasks_remaining_checkpoints
    }

    fn get_tasks_processed_checkpoints_metric(&self) -> &IntCounterVec {
        &self.indexer_metrics.tasks_processed_checkpoints
    }
}

async fn retrieve_and_process_live_finalized_logs(
    client: Arc<EthClient<MeteredEthHttpProvier>>,
    provider: Arc<Provider<Http>>,
    addresses: Vec<EthAddress>,
    starting_checkpoint: u64,
    data_sender: DataSender<RawEthData>,
    bridge_metrics: Arc<BridgeMetrics>,
) -> Result<(), Error> {
    let eth_contracts_to_watch = HashMap::from_iter(
        addresses
            .iter()
            .map(|address| (*address, starting_checkpoint)),
    );

    let (_, mut eth_events_rx, _) = EthSyncer::new(client.clone(), eth_contracts_to_watch)
        .run(bridge_metrics.clone())
        .await
        .expect("Failed to start eth syncer");

    // forward received events to the data sender
    while let Some((_, block, logs)) = eth_events_rx.recv().await {
        let raw_logs: Vec<RawEthLog> = logs
            .into_iter()
            .map(|log| RawEthLog {
                block_number: block,
                tx_hash: log.tx_hash,
                log: log.log,
            })
            .collect();

        process_logs(raw_logs, provider.clone(), data_sender.clone(), block, true)
            .await
            .expect("Failed to process logs");
    }

    panic!("Eth finalized syncer live task stopped unexpectedly");
}

async fn retrieve_and_process_log_range(
    client: Arc<EthClient<MeteredEthHttpProvier>>,
    provider: Arc<Provider<Http>>,
    addresses: Vec<EthAddress>,
    starting_checkpoint: u64,
    target_checkpoint: u64,
    data_sender: DataSender<RawEthData>,
) -> Result<(), Error> {
    let mut all_logs = Vec::new();
    let mut current_start = starting_checkpoint;

    while current_start <= target_checkpoint {
        // Calculate the end of the current chunk
        let current_end = (current_start + MAX_LOG_QUERY_RANGE - 1).min(target_checkpoint);

        // Retry the request for the current chunk
        let Ok(Ok(logs)) = retry_with_max_elapsed_time!(
            client.get_raw_events_in_range(addresses.clone(), current_start, current_end),
            Duration::from_secs(30000)
        ) else {
            panic!(
                "Unable to get logs from provider for range {} to {}",
                current_start, current_end
            );
        };

        // Add the logs from this chunk to the total
        all_logs.extend(logs);

        // Update the start for the next chunk
        current_start = current_end + 1;
    }

    process_logs(
        all_logs,
        provider.clone(),
        data_sender.clone(),
        target_checkpoint,
        true,
    )
    .await?;

    Ok::<_, Error>(())
}

async fn process_logs(
    logs: Vec<RawEthLog>,
    provider: Arc<Provider<Http>>,
    data_sender: DataSender<RawEthData>,
    target_checkpoint: u64,
    is_finalized: bool,
) -> Result<(), Error> {
    let mut data = Vec::new();
    let mut cached_blocks: HashMap<u64, Block<H256>> = HashMap::new();

    for log in logs {
        let block = if let Some(cached_block) = cached_blocks.get(&log.block_number) {
            cached_block.clone()
        } else {
            // TODO: add block query parallelism
            let Ok(Ok(Some(block))) = retry_with_max_elapsed_time!(
                provider.get_block(log.block_number),
                Duration::from_secs(30000)
            ) else {
                panic!("Unable to get block from provider");
            };

            cached_blocks.insert(log.block_number, block.clone());
            block
        };

        let Ok(Ok(Some(transaction))) = retry_with_max_elapsed_time!(
            provider.get_transaction(log.tx_hash),
            Duration::from_secs(30000)
        ) else {
            panic!("Unable to get transaction from provider");
        };

        data.push(RawEthData {
            log,
            block,
            transaction,
            is_finalized,
        });
    }

    data_sender.send((target_checkpoint, data)).await?;

    Ok::<_, Error>(())
}

#[derive(Clone)]
pub struct EthDataMapper {
    pub metrics: BridgeIndexerMetrics,
}

impl DataMapper<RawEthData, ProcessedTxnData> for EthDataMapper {
    fn map(
        &self,
        RawEthData {
            log,
            block,
            transaction,
            is_finalized,
        }: RawEthData,
    ) -> Result<Vec<ProcessedTxnData>, Error> {
        let eth_bridge_event = EthBridgeEvent::try_from_log(log.log());
        if eth_bridge_event.is_none() {
            return Ok(vec![]);
        }
        self.metrics.total_eth_bridge_transactions.inc();
        let bridge_event = eth_bridge_event.unwrap();
        let timestamp_ms = block.timestamp.as_u64() * 1000;
        let gas = transaction.gas;

        let transfer = match bridge_event {
            EthBridgeEvent::EthSuiBridgeEvents(bridge_event) => match bridge_event {
                EthSuiBridgeEvents::TokensDepositedFilter(bridge_event) => {
                    info!("Observed Eth Deposit at block: {}", log.block_number());
                    self.metrics.total_eth_token_deposited.inc();
                    ProcessedTxnData::TokenTransfer(TokenTransfer {
                        chain_id: bridge_event.source_chain_id,
                        nonce: bridge_event.nonce,
                        block_height: log.block_number(),
                        timestamp_ms,
                        txn_hash: transaction.hash.as_bytes().to_vec(),
                        txn_sender: bridge_event.sender_address.as_bytes().to_vec(),
                        status: TokenTransferStatus::Deposited,
                        gas_usage: gas.as_u64() as i64,
                        data_source: BridgeDataSource::Eth,
                        is_finalized,
                        data: Some(TokenTransferData {
                            sender_address: bridge_event.sender_address.as_bytes().to_vec(),
                            destination_chain: bridge_event.destination_chain_id,
                            recipient_address: bridge_event.recipient_address.to_vec(),
                            token_id: bridge_event.token_id,
                            amount: bridge_event.sui_adjusted_amount,
                            is_finalized,
                        }),
                    })
                }
                EthSuiBridgeEvents::TokensClaimedFilter(bridge_event) => {
                    info!("Observed Eth Claim at block: {}", log.block_number());
                    self.metrics.total_eth_token_transfer_claimed.inc();
                    ProcessedTxnData::TokenTransfer(TokenTransfer {
                        chain_id: bridge_event.source_chain_id,
                        nonce: bridge_event.nonce,
                        block_height: log.block_number(),
                        timestamp_ms,
                        txn_hash: transaction.hash.as_bytes().to_vec(),
                        txn_sender: bridge_event.sender_address.to_vec(),
                        status: TokenTransferStatus::Claimed,
                        gas_usage: gas.as_u64() as i64,
                        data_source: BridgeDataSource::Eth,
                        data: None,
                        is_finalized,
                    })
                }
                EthSuiBridgeEvents::PausedFilter(_)
                | EthSuiBridgeEvents::UnpausedFilter(_)
                | EthSuiBridgeEvents::UpgradedFilter(_)
                | EthSuiBridgeEvents::InitializedFilter(_) => {
                    // TODO: handle these events
                    self.metrics.total_eth_bridge_txn_other.inc();
                    return Ok(vec![]);
                }
            },
            EthBridgeEvent::EthBridgeCommitteeEvents(_)
            | EthBridgeEvent::EthBridgeLimiterEvents(_)
            | EthBridgeEvent::EthBridgeConfigEvents(_)
            | EthBridgeEvent::EthCommitteeUpgradeableContractEvents(_) => {
                // TODO: handle these events
                self.metrics.total_eth_bridge_txn_other.inc();
                return Ok(vec![]);
            }
        };
        Ok(vec![transfer])
    }
}
