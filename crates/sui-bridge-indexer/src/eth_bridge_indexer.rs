// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::BridgeIndexerMetrics;
use crate::{
    BridgeDataSource, GovernanceAction, ProcessedTxnData, TokenTransfer, TokenTransferData,
    TokenTransferStatus,
};
use alloy::providers::Provider;
use alloy::rpc::types::Log;
use alloy::{
    primitives::{Address as EthAddress, B256},
    rpc::types::{Block, Filter, Transaction},
};
use anyhow::Error;
use async_trait::async_trait;
use futures::stream::StreamExt;
use mysten_metrics::spawn_monitored_task;
use prometheus::IntGauge;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use sui_bridge::abi::{
    EthBridgeCommitteeEvents, EthBridgeConfigEvents, EthBridgeEvent, EthBridgeLimiterEvents,
    EthSuiBridgeEvents,
};
use sui_bridge::error::BridgeError;
use sui_bridge::eth_client::EthClient;
use sui_bridge::eth_syncer::EthSyncer;
use sui_bridge::metrics::BridgeMetrics;
use sui_bridge::retry_with_max_elapsed_time;
use sui_bridge::types::{EthEvent, RawEthLog};
use sui_bridge::utils::{EthProvider, EthWsProvider, get_eth_provider, get_eth_ws_provider};
use sui_bridge_schema::models::GovernanceActionType;
use sui_indexer_builder::Task;
use sui_indexer_builder::indexer_builder::{DataMapper, DataSender, Datasource};
use sui_indexer_builder::metrics::IndexerMetricProvider;
use tap::tap::TapFallible;
use tokio::select;
use tokio::task::JoinHandle;
use tracing::{info, warn};

#[derive(Debug)]
pub struct RawEthData {
    log: RawEthLog,
    block: Block<B256>,
    transaction: Transaction,
    is_finalized: bool,
}

// Create max log query range
const MAX_LOG_QUERY_RANGE: u64 = 1000;
pub struct EthSubscriptionDatasource {
    eth_client: Arc<EthClient>,
    addresses: Vec<EthAddress>,
    eth_ws_url: String,
    metrics: Box<dyn IndexerMetricProvider>,
    genesis_block: u64,
}

impl EthSubscriptionDatasource {
    pub async fn new(
        eth_sui_bridge_contract_addresses: Vec<EthAddress>,
        eth_client: Arc<EthClient>,
        eth_ws_url: String,
        metrics: Box<dyn IndexerMetricProvider>,
        genesis_block: u64,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            addresses: eth_sui_bridge_contract_addresses,
            eth_client,
            eth_ws_url,
            metrics,
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
        assert!(
            task.is_live_task,
            "EthSubscriptionDatasource only supports live tasks"
        );
        let filter = Filter::new()
            .address(self.addresses.clone())
            .from_block(task.start_checkpoint)
            .to_block(task.target_checkpoint);

        let eth_ws_url = self.eth_ws_url.clone();
        let task_name = task.task_name.clone();
        let task_name_clone = task_name.clone();
        let progress_metric = self
            .metrics
            .get_tasks_latest_retrieved_checkpoints()
            .with_label_values(&[task.name_prefix(), task.type_str()]);
        let handle = spawn_monitored_task!(async move {
            let eth_ws_provider = get_eth_ws_provider(&eth_ws_url).await?;

            let subscription = eth_ws_provider.subscribe_logs(&filter).await.tap_err(|e| {
                tracing::error!("Failed to subscribe logs: {:?}", e);
            })?;
            let mut log_stream = subscription.into_stream();
            // Check latest block height every 5 sec
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                select! {
                    log = log_stream.next() => {
                        if let Some(log) = log {
                            Self::handle_log(&task_name_clone, log, eth_ws_provider.clone(), &data_sender).await;
                        } else {
                            panic!("EthSubscriptionDatasource log stream ended unexpectedly");
                        }
                    }
                    _ = interval.tick() => {
                        let Ok(Ok(block_num)) = retry_with_max_elapsed_time!(
                            eth_ws_provider.get_block_number(),
                            Duration::from_secs(30000)
                        ) else {
                            tracing::error!("Failed to get block number");
                            continue;
                        };
                        progress_metric.set(block_num as i64);
                    }
                }
            }
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

    fn metric_provider(&self) -> &dyn IndexerMetricProvider {
        self.metrics.as_ref()
    }
}

impl EthSubscriptionDatasource {
    async fn handle_log(
        task_name: &str,
        log: Log,
        eth_ws_provider: EthWsProvider,
        data_sender: &DataSender<RawEthData>,
    ) {
        tracing::info!(
            task_name,
            "EthSubscriptionDatasource retrieved log: {:?}",
            log
        );
        // TODO: enable a shared cache for blocks that can be used by both the subscription and finalized sync
        let mut cached_blocks: HashMap<u64, Block<B256>> = HashMap::new();
        let raw_log = RawEthLog {
            block_number: log
                .block_number
                .ok_or(BridgeError::ProviderError(
                    "Provider returns log without block_number".into(),
                ))
                .unwrap(),
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
                eth_ws_provider.get_block_by_number(block_number.into()),
                Duration::from_secs(30000)
            ) else {
                panic!("Unable to get block from provider");
            };

            let block = block.map_transactions(|tx| tx.block_hash.unwrap_or_default());
            cached_blocks.insert(block_number, block.clone());
            block
        };

        let Ok(Ok(Some(transaction))) = retry_with_max_elapsed_time!(
            eth_ws_provider.get_transaction_by_hash(raw_log.tx_hash),
            Duration::from_secs(30000)
        ) else {
            panic!("Unable to get transaction from provider");
        };
        tracing::info!(
            task_name,
            "Sending data from EthSubscriptionDatasource: {:?}",
            (raw_log.tx_hash, block_number)
        );
        let raw_eth_data = vec![RawEthData {
            log: raw_log,
            block,
            transaction,
            is_finalized: false,
        }];
        data_sender
            .send((block_number, raw_eth_data))
            .await
            .unwrap_or_else(|e| {
                tracing::error!(
                    task_name,
                    "Failed to send data from EthSubscriptionDatasource: {:?}",
                    e
                );
            });
    }
}

pub struct EthFinalizedSyncDatasource {
    bridge_addresses: Vec<EthAddress>,
    eth_http_url: String,
    eth_client: Arc<EthClient>,
    metrics: Box<dyn IndexerMetricProvider>,
    bridge_metrics: Arc<BridgeMetrics>,
    genesis_block: u64,
}

impl EthFinalizedSyncDatasource {
    pub async fn new(
        eth_sui_bridge_contract_addresses: Vec<EthAddress>,
        eth_client: Arc<EthClient>,
        eth_http_url: String,
        metrics: Box<dyn IndexerMetricProvider>,
        bridge_metrics: Arc<BridgeMetrics>,
        genesis_block: u64,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            bridge_addresses: eth_sui_bridge_contract_addresses,
            eth_http_url,
            eth_client,
            metrics,
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
        let eth_provider = get_eth_provider(&self.eth_http_url)?;
        let progress_metric = self
            .metrics
            .get_tasks_latest_retrieved_checkpoints()
            .with_label_values(&[task.name_prefix(), task.type_str()]);
        let bridge_addresses = self.bridge_addresses.clone();
        let client = self.eth_client.clone();
        let provider = eth_provider.clone();
        let bridge_metrics = self.bridge_metrics.clone();
        let handle = spawn_monitored_task!(async move {
            if task.is_live_task {
                loop_retrieve_and_process_live_finalized_logs(
                    task,
                    client,
                    provider,
                    bridge_addresses,
                    data_sender,
                    bridge_metrics,
                    progress_metric,
                )
                .await?;
            } else {
                loop_retrieve_and_process_log_range(
                    task,
                    client,
                    provider,
                    bridge_addresses,
                    data_sender,
                    progress_metric,
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

    fn metric_provider(&self) -> &dyn IndexerMetricProvider {
        self.metrics.as_ref()
    }
}

async fn loop_retrieve_and_process_live_finalized_logs(
    task: Task,
    client: Arc<EthClient>,
    provider: EthProvider,
    addresses: Vec<EthAddress>,
    data_sender: DataSender<RawEthData>,
    bridge_metrics: Arc<BridgeMetrics>,
    progress_metric: IntGauge,
) -> Result<(), Error> {
    let task_name = task.task_name.clone();
    let starting_checkpoint = task.start_checkpoint;
    let eth_contracts_to_watch = HashMap::from_iter(
        addresses
            .iter()
            .map(|address| (*address, starting_checkpoint)),
    );
    let (_, mut eth_events_rx, _) = EthSyncer::new(client.clone(), eth_contracts_to_watch)
        .run(bridge_metrics.clone())
        .await
        .expect("Failed to start eth syncer");

    // EthSyncer sends items even when there is no matching events.
    // We leverge this to update the progress metric.
    while let Some((_, block, logs)) = eth_events_rx.recv().await {
        let raw_logs: Vec<RawEthLog> = logs
            .into_iter()
            .map(|log| RawEthLog {
                block_number: block,
                tx_hash: log.tx_hash,
                log: log.log,
            })
            .collect();

        process_logs(
            &task_name,
            raw_logs,
            provider.clone(),
            data_sender.clone(),
            block,
            true,
        )
        .await
        .expect("Failed to process logs");
        progress_metric.set(block as i64);
    }

    panic!("Eth finalized syncer live task stopped unexpectedly");
}

async fn loop_retrieve_and_process_log_range(
    task: Task,
    client: Arc<EthClient>,
    provider: EthProvider,
    addresses: Vec<EthAddress>,
    data_sender: DataSender<RawEthData>,
    progress_metric: IntGauge,
) -> Result<(), Error> {
    let task_name = task.task_name.clone();
    let starting_checkpoint = task.start_checkpoint;
    let target_checkpoint = task.target_checkpoint;
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
        &task_name,
        all_logs,
        provider.clone(),
        data_sender.clone(),
        target_checkpoint,
        true,
    )
    .await
    .tap_ok(|_| {
        tracing::info!(task_name, "Finished processing range");
    })
    .tap_err(|e| {
        tracing::error!(task_name, "Failed to process logs: {:?}", e);
    })
    .expect("Process logs should not fail");
    progress_metric.set(target_checkpoint as i64);
    Ok::<_, Error>(())
}

async fn process_logs(
    task_name: &str,
    logs: Vec<RawEthLog>,
    provider: EthProvider,
    data_sender: DataSender<RawEthData>,
    block_height: u64,
    is_finalized: bool,
) -> Result<(), Error> {
    let mut data = Vec::new();
    let mut cached_blocks: HashMap<u64, Block<B256>> = HashMap::new();

    for log in logs {
        let block = if let Some(cached_block) = cached_blocks.get(&log.block_number) {
            cached_block.clone()
        } else {
            // TODO: add block query parallelism
            let Ok(Ok(Some(block))) = retry_with_max_elapsed_time!(
                provider.get_block_by_number(log.block_number.into()),
                Duration::from_secs(30000)
            ) else {
                panic!("Unable to get block from provider");
            };

            let block = block.map_transactions(|tx| tx.block_hash.unwrap_or_default());
            cached_blocks.insert(log.block_number, block.clone());
            block
        };

        let Ok(Ok(Some(transaction))) = retry_with_max_elapsed_time!(
            provider.get_transaction_by_hash(log.tx_hash),
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
    let tx_hashes = data
        .iter()
        .map(|data| (data.log.tx_hash, data.block.number()))
        .collect::<Vec<(B256, u64)>>();
    tracing::info!(
        task_name,
        "Sending data from EthFinalizedSyncDatasource: {:?}",
        tx_hashes
    );
    data_sender
        .send((block_height, data))
        .await
        .expect("Failed to send data");
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
        let timestamp_ms = block.header.timestamp * 1000;
        let gas = alloy::consensus::Transaction::gas_limit(&transaction);
        let mut processed_txn_data = Vec::new();
        let txn_sender = transaction.inner.signer().to_vec();
        let txn_hash = transaction.inner.hash().to_vec();

        match bridge_event {
            EthBridgeEvent::EthSuiBridgeEvents(bridge_event) => match &bridge_event {
                EthSuiBridgeEvents::TokensDeposited(bridge_event) => {
                    info!(
                        "Observed Eth Deposit at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );
                    self.metrics.total_eth_token_deposited.inc();
                    processed_txn_data.push(ProcessedTxnData::TokenTransfer(TokenTransfer {
                        chain_id: bridge_event.sourceChainID,
                        nonce: bridge_event.nonce,
                        block_height: log.block_number(),
                        timestamp_ms,
                        txn_hash: txn_hash.clone(),
                        txn_sender: txn_sender.clone(),
                        status: TokenTransferStatus::Deposited,
                        gas_usage: gas as i64,
                        data_source: BridgeDataSource::ETH,
                        is_finalized,
                        data: Some(TokenTransferData {
                            sender_address: txn_sender.clone(),
                            destination_chain: bridge_event.destinationChainID,
                            recipient_address: bridge_event.recipientAddress.to_vec(),
                            token_id: bridge_event.tokenID,
                            amount: bridge_event.suiAdjustedAmount,
                            is_finalized,
                        }),
                    }));
                }
                EthSuiBridgeEvents::TokensClaimed(bridge_event) => {
                    info!(
                        "Observed Eth Claim at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );
                    self.metrics.total_eth_token_transfer_claimed.inc();
                    processed_txn_data.push(ProcessedTxnData::TokenTransfer(TokenTransfer {
                        chain_id: bridge_event.sourceChainID,
                        nonce: bridge_event.nonce,
                        block_height: log.block_number(),
                        timestamp_ms,
                        txn_hash: txn_hash.clone(),
                        txn_sender: txn_sender.clone(),
                        status: TokenTransferStatus::Claimed,
                        gas_usage: gas as i64,
                        data_source: BridgeDataSource::ETH,
                        data: None,
                        is_finalized,
                    }));
                }
                EthSuiBridgeEvents::EmergencyOperation(f) => {
                    info!(
                        "Observed Eth Emergency Operation at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );
                    processed_txn_data.push(ProcessedTxnData::GovernanceAction(GovernanceAction {
                        nonce: Some(f.nonce),
                        data_source: BridgeDataSource::ETH,
                        tx_digest: txn_hash.clone(),
                        sender: txn_sender.clone(),
                        timestamp_ms,
                        action: GovernanceActionType::EmergencyOperation,
                        data: serde_json::to_value(bridge_event)?,
                    }));
                }
                EthSuiBridgeEvents::ContractUpgraded(f) => {
                    info!(
                        "Observed Eth SuiBridge Upgrade at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );

                    processed_txn_data.push(ProcessedTxnData::GovernanceAction(GovernanceAction {
                        nonce: Some(f.nonce.to::<u64>()),
                        data_source: BridgeDataSource::ETH,
                        tx_digest: txn_hash.clone(),
                        sender: txn_sender.clone(),
                        timestamp_ms,
                        action: GovernanceActionType::UpgradeEVMContract,
                        data: serde_json::to_value(bridge_event)?,
                    }));
                }

                EthSuiBridgeEvents::Initialized(_)
                | EthSuiBridgeEvents::Paused(_)
                | EthSuiBridgeEvents::Unpaused(_)
                | EthSuiBridgeEvents::Upgraded(_) => {
                    warn!("Unexpected event {bridge_event:?}.")
                }
            },
            EthBridgeEvent::EthBridgeCommitteeEvents(bridge_event) => match &bridge_event {
                EthBridgeCommitteeEvents::BlocklistUpdated(_) => {
                    info!(
                        "Observed Eth Blocklist Update at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );

                    processed_txn_data.push(ProcessedTxnData::GovernanceAction(GovernanceAction {
                        nonce: None,
                        data_source: BridgeDataSource::ETH,
                        tx_digest: txn_hash.clone(),
                        sender: txn_sender.clone(),
                        timestamp_ms,
                        action: GovernanceActionType::UpdateCommitteeBlocklist,
                        data: serde_json::to_value(bridge_event)?,
                    }));
                }
                EthBridgeCommitteeEvents::BlocklistUpdatedV2(f) => {
                    info!(
                        "Observed Eth Blocklist Update at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );

                    processed_txn_data.push(ProcessedTxnData::GovernanceAction(GovernanceAction {
                        nonce: Some(f.nonce),
                        data_source: BridgeDataSource::ETH,
                        tx_digest: txn_hash.clone(),
                        sender: txn_sender.clone(),
                        timestamp_ms,
                        action: GovernanceActionType::UpdateCommitteeBlocklist,
                        data: serde_json::to_value(bridge_event)?,
                    }));
                }
                EthBridgeCommitteeEvents::ContractUpgraded(f) => {
                    info!(
                        "Observed Eth BridgeCommittee Upgrade at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );

                    processed_txn_data.push(ProcessedTxnData::GovernanceAction(GovernanceAction {
                        nonce: Some(f.nonce.to::<u64>()),
                        data_source: BridgeDataSource::ETH,
                        tx_digest: txn_hash.clone(),
                        sender: txn_sender.clone(),
                        timestamp_ms,
                        action: GovernanceActionType::UpgradeEVMContract,
                        data: serde_json::to_value(bridge_event)?,
                    }));
                }
                EthBridgeCommitteeEvents::Initialized(_)
                | EthBridgeCommitteeEvents::Upgraded(_) => {
                    warn!("Unexpected event {bridge_event:?}.")
                }
            },
            EthBridgeEvent::EthBridgeLimiterEvents(bridge_event) => match &bridge_event {
                EthBridgeLimiterEvents::LimitUpdated(_) => {
                    info!(
                        "Observed Eth BridgeLimiter Update at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );

                    processed_txn_data.push(ProcessedTxnData::GovernanceAction(GovernanceAction {
                        nonce: None,
                        data_source: BridgeDataSource::ETH,
                        tx_digest: txn_hash.clone(),
                        sender: txn_sender.clone(),
                        timestamp_ms,
                        action: GovernanceActionType::UpdateBridgeLimit,
                        data: serde_json::to_value(bridge_event)?,
                    }));
                }
                EthBridgeLimiterEvents::LimitUpdatedV2(f) => {
                    info!(
                        "Observed Eth BridgeLimiter Update at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );

                    processed_txn_data.push(ProcessedTxnData::GovernanceAction(GovernanceAction {
                        nonce: Some(f.nonce),
                        data_source: BridgeDataSource::ETH,
                        tx_digest: txn_hash.clone(),
                        sender: txn_sender.clone(),
                        timestamp_ms,
                        action: GovernanceActionType::UpdateBridgeLimit,
                        data: serde_json::to_value(bridge_event)?,
                    }));
                }
                EthBridgeLimiterEvents::ContractUpgraded(f) => {
                    info!(
                        "Observed Eth BridgeLimiter Upgrade at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );

                    processed_txn_data.push(ProcessedTxnData::GovernanceAction(GovernanceAction {
                        nonce: Some(f.nonce.to::<u64>()),
                        data_source: BridgeDataSource::ETH,
                        tx_digest: txn_hash.clone(),
                        sender: txn_sender.clone(),
                        timestamp_ms,
                        action: GovernanceActionType::UpgradeEVMContract,
                        data: serde_json::to_value(bridge_event)?,
                    }));
                }

                EthBridgeLimiterEvents::HourlyTransferAmountUpdated(_)
                | EthBridgeLimiterEvents::Initialized(_)
                | EthBridgeLimiterEvents::OwnershipTransferred(_)
                | EthBridgeLimiterEvents::Upgraded(_) => {
                    warn!("Unexpected event {bridge_event:?}.")
                }
            },
            EthBridgeEvent::EthBridgeConfigEvents(bridge_event) => match &bridge_event {
                EthBridgeConfigEvents::TokenPriceUpdated(_) => {
                    info!(
                        "Observed Eth TokenPrices Update at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );

                    processed_txn_data.push(ProcessedTxnData::GovernanceAction(GovernanceAction {
                        nonce: None,
                        data_source: BridgeDataSource::ETH,
                        tx_digest: txn_hash.clone(),
                        sender: txn_sender.clone(),
                        timestamp_ms,
                        action: GovernanceActionType::UpdateTokenPrices,
                        data: serde_json::to_value(bridge_event)?,
                    }));
                }
                EthBridgeConfigEvents::TokenPriceUpdatedV2(f) => {
                    info!(
                        "Observed Eth TokenPrices Update at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );

                    processed_txn_data.push(ProcessedTxnData::GovernanceAction(GovernanceAction {
                        nonce: Some(f.nonce),
                        data_source: BridgeDataSource::ETH,
                        tx_digest: txn_hash.clone(),
                        sender: txn_sender.clone(),
                        timestamp_ms,
                        action: GovernanceActionType::UpdateTokenPrices,
                        data: serde_json::to_value(bridge_event)?,
                    }));
                }
                EthBridgeConfigEvents::TokenAdded(_) => {
                    info!(
                        "Observed Eth AddSuiTokens at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );

                    processed_txn_data.push(ProcessedTxnData::GovernanceAction(GovernanceAction {
                        nonce: None,
                        data_source: BridgeDataSource::ETH,
                        tx_digest: txn_hash.clone(),
                        sender: txn_sender.clone(),
                        timestamp_ms,
                        action: GovernanceActionType::AddEVMTokens,
                        data: serde_json::to_value(bridge_event)?,
                    }));
                }
                EthBridgeConfigEvents::TokensAddedV2(f) => {
                    info!(
                        "Observed Eth AddSuiTokens at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );

                    processed_txn_data.push(ProcessedTxnData::GovernanceAction(GovernanceAction {
                        nonce: Some(f.nonce),
                        data_source: BridgeDataSource::ETH,
                        tx_digest: txn_hash.clone(),
                        sender: txn_sender.clone(),
                        timestamp_ms,
                        action: GovernanceActionType::AddEVMTokens,
                        data: serde_json::to_value(bridge_event)?,
                    }));
                }
                EthBridgeConfigEvents::ContractUpgraded(f) => {
                    info!(
                        "Observed Eth BridgeConfig Upgrade at block: {}, tx_hash: {}",
                        log.block_number(),
                        log.tx_hash
                    );

                    processed_txn_data.push(ProcessedTxnData::GovernanceAction(GovernanceAction {
                        nonce: Some(f.nonce.to::<u64>()),
                        data_source: BridgeDataSource::ETH,
                        tx_digest: txn_hash.clone(),
                        sender: txn_sender.clone(),
                        timestamp_ms,
                        action: GovernanceActionType::UpgradeEVMContract,
                        data: serde_json::to_value(bridge_event)?,
                    }));
                }

                EthBridgeConfigEvents::Initialized(_) | EthBridgeConfigEvents::Upgraded(_) => {
                    warn!("Unexpected event {bridge_event:?}.")
                }
            },
            EthBridgeEvent::EthCommitteeUpgradeableContractEvents(_) => {
                warn!("Unexpected event {bridge_event:?}.")
            }
        };
        Ok(processed_txn_data)
    }
}
