// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::Config;
use crate::latest_eth_syncer::LatestEthSyncer;
use crate::metrics::BridgeIndexerMetrics;
use crate::postgres_manager::get_latest_eth_token_transfer;
use crate::postgres_manager::{write, PgPool};
use crate::{BridgeDataSource, TokenTransfer, TokenTransferData, TokenTransferStatus};
use anyhow::Result;
use ethers::providers::Provider;
use ethers::providers::{Http, Middleware};
use ethers::types::Address as EthAddress;
use mysten_metrics::spawn_logged_monitored_task;
use std::collections::HashMap;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use sui_bridge::abi::{EthBridgeEvent, EthSuiBridgeEvents};
use sui_bridge::types::EthLog;
use sui_bridge::{eth_client::EthClient, eth_syncer::EthSyncer};
use tokio::task::JoinHandle;
use tracing::info;
use tracing::log::error;

#[derive(Clone, Debug)]
pub struct EthBridgeWorker {
    provider: Arc<Provider<Http>>,
    pg_pool: PgPool,
    metrics: BridgeIndexerMetrics,
    bridge_address: EthAddress,
    config: Config,
}

impl EthBridgeWorker {
    pub fn new(
        pg_pool: PgPool,
        metrics: BridgeIndexerMetrics,
        config: Config,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let bridge_address = EthAddress::from_str(&config.eth_sui_bridge_contract_address)?;

        let provider = Arc::new(
            Provider::<Http>::try_from(&config.eth_rpc_url)?
                .interval(std::time::Duration::from_millis(2000)),
        );

        Ok(Self {
            provider,
            pg_pool,
            metrics,
            bridge_address,
            config,
        })
    }

    pub async fn start_indexing_finalized_events(&self) -> Result<JoinHandle<()>> {
        let eth_client = Arc::new(
            EthClient::<ethers::providers::Http>::new(
                &self.config.eth_rpc_url,
                HashSet::from_iter(vec![self.bridge_address]),
            )
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
        );

        let newest_finalized_block = match get_latest_eth_token_transfer(&self.pg_pool, true)? {
            Some(transfer) => transfer.block_height as u64,
            None => self.config.start_block,
        };

        info!("Starting from finalized block: {}", newest_finalized_block);

        let finalized_contract_addresses =
            HashMap::from_iter(vec![(self.bridge_address, newest_finalized_block)]);

        let (_task_handles, eth_events_rx, _) =
            EthSyncer::new(eth_client, finalized_contract_addresses)
                .run()
                .await
                .map_err(|e| anyhow::anyhow!(format!("{:?}", e)))?;

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

    pub async fn start_indexing_unfinalized_events(&self) -> Result<JoinHandle<()>> {
        let eth_client = Arc::new(
            EthClient::<ethers::providers::Http>::new(
                &self.config.eth_rpc_url,
                HashSet::from_iter(vec![self.bridge_address]),
            )
            .await?,
        );

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
        .run()
        .await
        .map_err(|e| anyhow::anyhow!(format!("{:?}", e)))?;

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
}

async fn process_eth_events(
    provider: Arc<Provider<Http>>,
    pg_pool: PgPool,
    metrics: BridgeIndexerMetrics,
    mut eth_events_rx: mysten_metrics::metered_channel::Receiver<(EthAddress, u64, Vec<EthLog>)>,
    finalized: bool,
) {
    while let Some((_, _, logs)) = eth_events_rx.recv().await {
        for log in logs.iter() {
            let eth_bridge_event = EthBridgeEvent::try_from_eth_log(log);
            if eth_bridge_event.is_none() {
                continue;
            }
            metrics.total_eth_bridge_transactions.inc();
            let bridge_event = eth_bridge_event.unwrap();
            let block_number = log.block_number;
            let block = provider.get_block(log.block_number).await.unwrap().unwrap();
            let timestamp = block.timestamp.as_u64() * 1000;
            let transaction = provider
                .get_transaction(log.tx_hash)
                .await
                .unwrap()
                .unwrap();
            let gas = transaction.gas;
            let tx_hash = log.tx_hash;

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

            if let Err(e) = write(&pg_pool, vec![transfer]) {
                error!("Error writing token transfer to database: {:?}", e);
            }
        }
    }

    panic!("Eth event stream ended unexpectedly");
}
