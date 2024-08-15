// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Error;
use async_trait::async_trait;
use ethers::prelude::Transaction;
use ethers::providers::{Http, Middleware, Provider, StreamExt, Ws};
use ethers::types::{Address as EthAddress, Block, Filter, H256};
use sui_bridge::error::BridgeError;
use sui_bridge::eth_client::EthClient;
use sui_bridge::metered_eth_provider::MeteredEthHttpProvier;
use sui_bridge::retry_with_max_elapsed_time;
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

type RawEthData = (RawEthLog, Block<H256>, Transaction);

pub struct EthSubscriptionDatasource {
    bridge_address: EthAddress,
    eth_ws_url: String,
    indexer_metrics: BridgeIndexerMetrics,
}

impl EthSubscriptionDatasource {
    pub fn new(
        eth_sui_bridge_contract_address: String,
        eth_ws_url: String,
        indexer_metrics: BridgeIndexerMetrics,
    ) -> Result<Self, anyhow::Error> {
        let bridge_address = EthAddress::from_str(&eth_sui_bridge_contract_address)?;
        Ok(Self {
            bridge_address,
            eth_ws_url,
            indexer_metrics,
        })
    }
}
#[async_trait]
impl Datasource<RawEthData> for EthSubscriptionDatasource {
    async fn start_data_retrieval(
        &self,
        starting_checkpoint: u64,
        target_checkpoint: u64,
        data_sender: DataSender<RawEthData>,
    ) -> Result<JoinHandle<Result<(), Error>>, Error> {
        let filter = Filter::new()
            .address(self.bridge_address)
            .from_block(starting_checkpoint)
            .to_block(target_checkpoint);

        let eth_ws_url = self.eth_ws_url.clone();
        let indexer_metrics: BridgeIndexerMetrics = self.indexer_metrics.clone();

        let handle = spawn_monitored_task!(async move {
            let eth_ws_client = Provider::<Ws>::connect(&eth_ws_url).await?;

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
                    .send((block_number, vec![(raw_log, block, transaction)]))
                    .await?;

                indexer_metrics
                    .latest_committed_eth_block
                    .set(block_number as i64);
            }

            Ok::<_, Error>(())
        });
        Ok(handle)
    }
}

pub struct EthSyncDatasource {
    bridge_address: EthAddress,
    eth_http_url: String,
    indexer_metrics: BridgeIndexerMetrics,
    bridge_metrics: Arc<BridgeMetrics>,
}

impl EthSyncDatasource {
    pub fn new(
        eth_sui_bridge_contract_address: String,
        eth_http_url: String,
        indexer_metrics: BridgeIndexerMetrics,
        bridge_metrics: Arc<BridgeMetrics>,
    ) -> Result<Self, anyhow::Error> {
        let bridge_address = EthAddress::from_str(&eth_sui_bridge_contract_address)?;
        Ok(Self {
            bridge_address,
            eth_http_url,
            indexer_metrics,
            bridge_metrics,
        })
    }
}
#[async_trait]
impl Datasource<RawEthData> for EthSyncDatasource {
    async fn start_data_retrieval(
        &self,
        starting_checkpoint: u64,
        target_checkpoint: u64,
        data_sender: DataSender<RawEthData>,
    ) -> Result<JoinHandle<Result<(), Error>>, Error> {
        let client: Arc<EthClient<MeteredEthHttpProvier>> = Arc::new(
            EthClient::<MeteredEthHttpProvier>::new(
                &self.eth_http_url,
                HashSet::from_iter(vec![self.bridge_address]),
                self.bridge_metrics.clone(),
            )
            .await?,
        );

        let provider = Arc::new(
            Provider::<Http>::try_from(&self.eth_http_url)?
                .interval(std::time::Duration::from_millis(2000)),
        );

        let bridge_address = self.bridge_address;
        let indexer_metrics: BridgeIndexerMetrics = self.indexer_metrics.clone();
        let client = Arc::clone(&client);
        let provider = Arc::clone(&provider);

        let handle = spawn_monitored_task!(async move {
            let mut cached_blocks: HashMap<u64, Block<H256>> = HashMap::new();

            let Ok(Ok(logs)) = retry_with_max_elapsed_time!(
                client.get_raw_events_in_range(
                    bridge_address,
                    starting_checkpoint,
                    target_checkpoint
                ),
                Duration::from_secs(30000)
            ) else {
                panic!("Unable to get logs from provider");
            };

            let mut data = Vec::new();
            let mut first_block = 0;

            for log in logs {
                let block = if let Some(cached_block) = cached_blocks.get(&log.block_number) {
                    cached_block.clone()
                } else {
                    let Ok(Ok(Some(block))) = retry_with_max_elapsed_time!(
                        provider.get_block(log.block_number),
                        Duration::from_secs(30000)
                    ) else {
                        panic!("Unable to get block from provider");
                    };

                    cached_blocks.insert(log.block_number, block.clone());
                    block
                };

                if first_block == 0 {
                    first_block = log.block_number;
                }

                let Ok(Ok(Some(transaction))) = retry_with_max_elapsed_time!(
                    provider.get_transaction(log.tx_hash),
                    Duration::from_secs(30000)
                ) else {
                    panic!("Unable to get transaction from provider");
                };

                data.push((log, block, transaction));
            }

            data_sender.send((target_checkpoint, data)).await?;

            indexer_metrics
                .last_synced_eth_block
                .set(first_block as i64);

            Ok::<_, Error>(())
        });

        Ok(handle)
    }
}

#[derive(Clone)]
pub struct EthDataMapper {
    pub metrics: BridgeIndexerMetrics,
}

impl<E: EthEvent> DataMapper<(E, Block<H256>, Transaction), ProcessedTxnData> for EthDataMapper {
    fn map(
        &self,
        (log, block, transaction): (E, Block<H256>, Transaction),
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
                        data: Some(TokenTransferData {
                            sender_address: bridge_event.sender_address.as_bytes().to_vec(),
                            destination_chain: bridge_event.destination_chain_id,
                            recipient_address: bridge_event.recipient_address.to_vec(),
                            token_id: bridge_event.token_id,
                            amount: bridge_event.sui_adjusted_amount,
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
