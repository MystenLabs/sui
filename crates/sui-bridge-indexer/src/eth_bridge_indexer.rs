// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use ethers::prelude::Transaction;
use ethers::providers::{Http, Middleware, Provider};
use ethers::types::{Address as EthAddress, Block, H256};
use tokio::task::JoinHandle;
use tracing::info;

use mysten_metrics::metered_channel::Receiver;
use mysten_metrics::{metered_channel, spawn_monitored_task};
use sui_bridge::abi::{EthBridgeEvent, EthSuiBridgeEvents};
use sui_bridge::eth_client::EthClient;
use sui_bridge::eth_syncer::EthSyncer;
use sui_bridge::metered_eth_provider::{new_metered_eth_provider, MeteredEthHttpProvier};
use sui_bridge::metrics::BridgeMetrics;
use sui_bridge::retry_with_max_elapsed_time;
use sui_bridge::types::{EthEvent, EthLog, RawEthLog};

use crate::indexer_builder::{DataMapper, Datasource};
use crate::latest_eth_syncer::LatestEthSyncer;
use crate::metrics::BridgeIndexerMetrics;
use crate::sui_bridge_indexer::PgBridgePersistent;
use crate::{
    BridgeDataSource, ProcessedTxnData, TokenTransfer, TokenTransferData, TokenTransferStatus,
};

type EthData = (EthLog, Block<H256>, Transaction);
type RawEthData = (RawEthLog, Block<H256>, Transaction);

pub struct EthFinalizedDatasource {
    bridge_address: EthAddress,
    eth_rpc_url: String,
    bridge_metrics: Arc<BridgeMetrics>,
    indexer_metrics: BridgeIndexerMetrics,
}

impl EthFinalizedDatasource {
    pub fn new(
        eth_sui_bridge_contract_address: String,
        eth_rpc_url: String,
        bridge_metrics: Arc<BridgeMetrics>,
        indexer_metrics: BridgeIndexerMetrics,
    ) -> Result<Self, anyhow::Error> {
        let bridge_address = EthAddress::from_str(&eth_sui_bridge_contract_address)?;
        Ok(Self {
            bridge_address,
            eth_rpc_url,
            bridge_metrics,
            indexer_metrics,
        })
    }
}

#[async_trait]
impl Datasource<EthData, PgBridgePersistent, ProcessedTxnData> for EthFinalizedDatasource {
    async fn start_data_retrieval(
        &self,
        task_name: String,
        starting_checkpoint: u64,
        target_checkpoint: u64,
    ) -> Result<(JoinHandle<Result<(), Error>>, Receiver<(u64, Vec<EthData>)>), Error> {
        let eth_client = Arc::new(
            EthClient::<MeteredEthHttpProvier>::new(
                &self.eth_rpc_url,
                HashSet::from_iter(vec![self.bridge_address]),
                self.bridge_metrics.clone(),
            )
            .await?,
        );

        let provider = Arc::new(
            Provider::<Http>::try_from(&self.eth_rpc_url)?
                .interval(std::time::Duration::from_millis(2000)),
        );

        info!("Starting from finalized block: {}", starting_checkpoint);

        let finalized_contract_addresses =
            HashMap::from_iter(vec![(self.bridge_address, starting_checkpoint)]);

        let (task_handles, mut eth_events_rx, _) =
            EthSyncer::new(eth_client, finalized_contract_addresses)
                .run(self.bridge_metrics.clone())
                .await
                .map_err(|e| anyhow!(format!("{e:?}")))?;

        let (data_sender, data_receiver) = metered_channel::channel(
            1000,
            &mysten_metrics::get_metrics()
                .unwrap()
                .channel_inflight
                .with_label_values(&[&task_name]),
        );
        let indexer_metrics = self.indexer_metrics.clone();
        let handle = spawn_monitored_task!(async {
            'outer: while let Some((_, _, logs)) = eth_events_rx.recv().await {
                // group logs by block, BTreeMap is used here to keep blocks in ascending order.
                let blocks = logs.into_iter().fold(
                    BTreeMap::new(),
                    |mut result: BTreeMap<_, Vec<_>>, log| {
                        let block_number = log.block_number;
                        result.entry(block_number).or_default().push(log);
                        result
                    },
                );
                for (block_number, logs) in blocks {
                    if block_number > target_checkpoint {
                        break 'outer;
                    }
                    let mut data = vec![];
                    let Ok(Ok(Some(block))) = retry_with_max_elapsed_time!(
                        provider.get_block(block_number),
                        Duration::from_secs(300)
                    ) else {
                        panic!("Failed to query block {block_number} from Ethereum after retry");
                    };

                    for log in logs {
                        let tx_hash = log.tx_hash();
                        let Ok(Ok(Some(transaction))) = retry_with_max_elapsed_time!(
                            provider.get_transaction(tx_hash),
                            Duration::from_secs(300)
                        ) else {
                            panic!(
                                "Failed to query transaction {tx_hash} from Ethereum after retry"
                            );
                        };
                        info!(
                            "Retrieved eth log {} for block {}",
                            log.tx_hash, log.block_number
                        );
                        data.push((log, block.clone(), transaction));
                    }
                    indexer_metrics
                        .last_committed_eth_block
                        .set(block_number as i64);

                    if data_sender.send((block_number, data)).await.is_err() {
                        // exit data retrieval loop when receiver dropped
                        break 'outer;
                    }
                }
            }
            task_handles.iter().for_each(|h| h.abort());
            Ok::<_, Error>(())
        });
        Ok((handle, data_receiver))
    }
}

pub struct EthUnfinalizedDatasource {
    bridge_address: EthAddress,
    eth_rpc_url: String,
    bridge_metrics: Arc<BridgeMetrics>,
    indexer_metrics: BridgeIndexerMetrics,
}

impl EthUnfinalizedDatasource {
    pub fn new(
        eth_sui_bridge_contract_address: String,
        eth_rpc_url: String,
        bridge_metrics: Arc<BridgeMetrics>,
        indexer_metrics: BridgeIndexerMetrics,
    ) -> Result<Self, anyhow::Error> {
        let bridge_address = EthAddress::from_str(&eth_sui_bridge_contract_address)?;
        Ok(Self {
            bridge_address,
            eth_rpc_url,
            bridge_metrics,
            indexer_metrics,
        })
    }
}

#[async_trait]
impl Datasource<RawEthData, PgBridgePersistent, ProcessedTxnData> for EthUnfinalizedDatasource {
    async fn start_data_retrieval(
        &self,
        task_name: String,
        starting_checkpoint: u64,
        target_checkpoint: u64,
    ) -> Result<
        (
            JoinHandle<Result<(), Error>>,
            Receiver<(u64, Vec<RawEthData>)>,
        ),
        Error,
    > {
        let eth_client = Arc::new(
            EthClient::<MeteredEthHttpProvier>::new(
                &self.eth_rpc_url,
                HashSet::from_iter(vec![self.bridge_address]),
                self.bridge_metrics.clone(),
            )
            .await?,
        );

        let provider = Arc::new(
            new_metered_eth_provider(&self.eth_rpc_url, self.bridge_metrics.clone())?
                .interval(Duration::from_millis(2000)),
        );

        info!("Starting from unfinalized block: {}", starting_checkpoint);

        let unfinalized_contract_addresses =
            HashMap::from_iter(vec![(self.bridge_address, starting_checkpoint)]);

        let (task_handles, mut eth_events_rx) = LatestEthSyncer::new(
            eth_client,
            provider.clone(),
            unfinalized_contract_addresses.clone(),
        )
        .run(self.indexer_metrics.clone())
        .await
        .map_err(|e| anyhow!(format!("{e:?}")))?;

        let (data_sender, data_receiver) = metered_channel::channel(
            1000,
            &mysten_metrics::get_metrics()
                .unwrap()
                .channel_inflight
                .with_label_values(&[&task_name]),
        );
        let indexer_metrics = self.indexer_metrics.clone();
        let handle = spawn_monitored_task!(async {
            'outer: while let Some((_, _, logs)) = eth_events_rx.recv().await {
                // group logs by block
                let blocks = logs.into_iter().fold(
                    BTreeMap::new(),
                    |mut result: BTreeMap<_, Vec<_>>, log| {
                        let block_number = log.block_number;
                        result.entry(block_number).or_default().push(log);
                        result
                    },
                );
                for (block_number, logs) in blocks {
                    if block_number > target_checkpoint {
                        break 'outer;
                    }
                    let mut data = vec![];
                    let block = provider.get_block(block_number).await?.unwrap();

                    for log in logs {
                        let tx_hash = log.tx_hash();
                        let transaction = provider.get_transaction(tx_hash).await?.unwrap();
                        data.push((log.clone(), block.clone(), transaction.clone()));
                        info!(
                            "Processing eth log {} for block {}",
                            log.tx_hash, log.block_number
                        )
                    }
                    indexer_metrics
                        .last_committed_unfinalized_eth_block
                        .set(block_number as i64);
                    if data_sender.send((block_number, data)).await.is_err() {
                        // exit data retrieval loop when receiver dropped
                        break 'outer;
                    }
                }
            }
            task_handles.iter().for_each(|h| h.abort());
            Ok::<_, Error>(())
        });
        Ok((handle, data_receiver))
    }
}

#[derive(Clone)]
pub struct EthDataMapper {
    pub finalized: bool,
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
                    info!(
                        "Observed {} Eth Deposit at block {}",
                        if self.finalized {
                            "Finalized"
                        } else {
                            "Unfinalized"
                        },
                        log.block_number()
                    );
                    if self.finalized {
                        self.metrics.total_eth_token_deposited.inc();
                    }
                    ProcessedTxnData::TokenTransfer(TokenTransfer {
                        chain_id: bridge_event.source_chain_id,
                        nonce: bridge_event.nonce,
                        block_height: log.block_number(),
                        timestamp_ms,
                        txn_hash: transaction.hash.as_bytes().to_vec(),
                        txn_sender: bridge_event.sender_address.as_bytes().to_vec(),
                        status: if self.finalized {
                            TokenTransferStatus::Deposited
                        } else {
                            TokenTransferStatus::DepositedUnfinalized
                        },
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
                    // Only write unfinalized claims
                    if self.finalized {
                        return Ok(vec![]);
                    }
                    info!("Observed Unfinalized Eth Claim");
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
