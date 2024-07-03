// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::IndexerConfig;
use crate::latest_eth_syncer::LatestEthSyncer;
use crate::metrics::BridgeIndexerMetrics;
use crate::postgres_manager::get_latest_eth_token_transfer;
use crate::postgres_manager::{write, PgPool};
use crate::{
    BridgeDataSource, ProcessedTxnData, TokenTransfer, TokenTransferData, TokenTransferStatus,
};
use anyhow::{anyhow, Result};
use ethers::providers::Provider;
use ethers::providers::{Http, Middleware};
use ethers::types::Address as EthAddress;
use mysten_metrics::spawn_logged_monitored_task;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use sui_bridge::abi::{EthBridgeEvent, EthSuiBridgeEvents};
use sui_bridge::metrics::BridgeMetrics;
use sui_bridge::types::EthEvent;
use sui_bridge::{eth_client::EthClient, eth_syncer::EthSyncer};
use tokio::task::JoinHandle;
use tracing::info;
use tracing::log::error;

#[derive(Clone)]
pub struct EthBridgeWorker {
    provider: Arc<Provider<Http>>,
    pg_pool: PgPool,
    bridge_metrics: Arc<BridgeMetrics>,
    metrics: BridgeIndexerMetrics,
    bridge_address: EthAddress,
    config: IndexerConfig,
}

impl EthBridgeWorker {
    pub fn new(
        pg_pool: PgPool,
        bridge_metrics: Arc<BridgeMetrics>,
        metrics: BridgeIndexerMetrics,
        config: IndexerConfig,
    ) -> Result<Self, anyhow::Error> {
        let bridge_address = EthAddress::from_str(&config.eth_sui_bridge_contract_address)?;

        let provider = Arc::new(
            Provider::<Http>::try_from(&config.eth_rpc_url)?
                .interval(std::time::Duration::from_millis(2000)),
        );

        Ok(Self {
            provider,
            pg_pool,
            bridge_metrics,
            metrics,
            bridge_address,
            config,
        })
    }

    pub async fn start_indexing_finalized_events(
        &self,
        eth_client: Arc<EthClient<ethers::providers::Http>>,
    ) -> Result<JoinHandle<()>> {
        let newest_finalized_block = match get_latest_eth_token_transfer(&self.pg_pool, true)? {
            Some(transfer) => transfer.block_height as u64,
            None => self.config.start_block,
        };

        info!("Starting from finalized block: {}", newest_finalized_block);

        let finalized_contract_addresses =
            HashMap::from_iter(vec![(self.bridge_address, newest_finalized_block)]);

        let (_task_handles, eth_events_rx, _) =
            EthSyncer::new(eth_client, finalized_contract_addresses)
                .run(self.bridge_metrics.clone())
                .await
                .map_err(|e| anyhow!(format!("{e:?}")))?;

        let provider_clone = self.provider.clone();
        let pg_pool_clone = self.pg_pool.clone();
        let metrics_clone = self.metrics.clone();

        Ok(spawn_logged_monitored_task!(
            process_eth_events(
                provider_clone,
                pg_pool_clone,
                metrics_clone,
                eth_events_rx,
                true
            ),
            "finalized indexer handler"
        ))
    }

    pub async fn start_indexing_unfinalized_events(
        &self,
        eth_client: Arc<EthClient<ethers::providers::Http>>,
    ) -> Result<JoinHandle<()>> {
        let newest_unfinalized_block_recorded =
            match get_latest_eth_token_transfer(&self.pg_pool, false)? {
                Some(transfer) => transfer.block_height as u64,
                None => self.config.start_block,
            };

        info!(
            "Starting from unfinalized block: {}",
            newest_unfinalized_block_recorded
        );

        let unfinalized_contract_addresses = HashMap::from_iter(vec![(
            self.bridge_address,
            newest_unfinalized_block_recorded,
        )]);

        let (_task_handles, eth_events_rx) = LatestEthSyncer::new(
            eth_client,
            self.provider.clone(),
            unfinalized_contract_addresses.clone(),
        )
        .run(self.metrics.clone())
        .await
        .map_err(|e| anyhow!(format!("{e:?}")))?;

        let provider_clone = self.provider.clone();
        let pg_pool_clone = self.pg_pool.clone();
        let metrics_clone = self.metrics.clone();

        Ok(spawn_logged_monitored_task!(
            process_eth_events(
                provider_clone,
                pg_pool_clone,
                metrics_clone,
                eth_events_rx,
                false
            ),
            "unfinalized indexer handler"
        ))
    }

    pub fn bridge_address(&self) -> EthAddress {
        self.bridge_address
    }
}

async fn process_eth_events<E: EthEvent>(
    provider: Arc<Provider<Http>>,
    pg_pool: PgPool,
    metrics: BridgeIndexerMetrics,
    mut eth_events_rx: mysten_metrics::metered_channel::Receiver<(EthAddress, u64, Vec<E>)>,
    finalized: bool,
) {
    let progress_gauge = if finalized {
        metrics.last_committed_eth_block.clone()
    } else {
        metrics.last_committed_unfinalized_eth_block.clone()
    };
    while let Some((_, _, logs)) = eth_events_rx.recv().await {
        // TODO: This for-loop can be optimzied to group tx / block info
        // and reduce the queries issued to eth full node
        for log in logs.iter() {
            let eth_bridge_event = EthBridgeEvent::try_from_log(log.log());
            if eth_bridge_event.is_none() {
                continue;
            }
            metrics.total_eth_bridge_transactions.inc();
            let bridge_event = eth_bridge_event.unwrap();
            let block_number = log.block_number();
            let block = provider.get_block(block_number).await.unwrap().unwrap();
            let timestamp = block.timestamp.as_u64() * 1000;
            let tx_hash = log.tx_hash();
            let transaction = provider.get_transaction(tx_hash).await.unwrap().unwrap();
            let gas = transaction.gas;

            let transfer: TokenTransfer = match bridge_event {
                EthBridgeEvent::EthSuiBridgeEvents(bridge_event) => match bridge_event {
                    EthSuiBridgeEvents::TokensDepositedFilter(bridge_event) => {
                        info!(
                            "Observed {} Eth Deposit at block {}",
                            if finalized {
                                "Finalized"
                            } else {
                                "Unfinalized"
                            },
                            block_number
                        );
                        if finalized {
                            metrics.total_eth_token_deposited.inc();
                        }
                        TokenTransfer {
                            chain_id: bridge_event.source_chain_id,
                            nonce: bridge_event.nonce,
                            block_height: block_number,
                            timestamp_ms: timestamp,
                            txn_hash: tx_hash.as_bytes().to_vec(),
                            txn_sender: bridge_event.sender_address.as_bytes().to_vec(),
                            status: if finalized {
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
                        }
                    }
                    EthSuiBridgeEvents::TokensClaimedFilter(bridge_event) => {
                        // Only write unfinalized claims
                        if finalized {
                            continue;
                        }
                        info!("Observed Unfinalized Eth Claim");
                        metrics.total_eth_token_transfer_claimed.inc();
                        TokenTransfer {
                            chain_id: bridge_event.source_chain_id,
                            nonce: bridge_event.nonce,
                            block_height: block_number,
                            timestamp_ms: timestamp,
                            txn_hash: tx_hash.as_bytes().to_vec(),
                            txn_sender: bridge_event.sender_address.to_vec(),
                            status: TokenTransferStatus::Claimed,
                            gas_usage: gas.as_u64() as i64,
                            data_source: BridgeDataSource::Eth,
                            data: None,
                        }
                    }
                    EthSuiBridgeEvents::PausedFilter(_)
                    | EthSuiBridgeEvents::UnpausedFilter(_)
                    | EthSuiBridgeEvents::UpgradedFilter(_)
                    | EthSuiBridgeEvents::InitializedFilter(_) => {
                        metrics.total_eth_bridge_txn_other.inc();
                        continue;
                    }
                },
                EthBridgeEvent::EthBridgeCommitteeEvents(_)
                | EthBridgeEvent::EthBridgeLimiterEvents(_)
                | EthBridgeEvent::EthBridgeConfigEvents(_)
                | EthBridgeEvent::EthCommitteeUpgradeableContractEvents(_) => {
                    metrics.total_eth_bridge_txn_other.inc();
                    continue;
                }
            };

            // TODO: we either scream here or keep retrying this until we succeed
            if let Err(e) = write(&pg_pool, vec![ProcessedTxnData::TokenTransfer(transfer)]) {
                error!("Error writing token transfer to database: {:?}", e);
            } else {
                progress_gauge.set(block_number as i64);
            }
        }
    }

    panic!("Eth event stream ended unexpectedly");
}
