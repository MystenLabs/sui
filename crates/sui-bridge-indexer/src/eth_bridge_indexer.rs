// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use ethers::prelude::Transaction;
use ethers::providers::{Http, Middleware, Provider};
use ethers::types::{Address as EthAddress, Block, H256};
use tracing::info;

use sui_bridge::abi::{EthBridgeEvent, EthSuiBridgeEvents};
use sui_bridge::eth_client::EthClient;
use sui_bridge::eth_syncer::EthSyncer;
use sui_bridge::metrics::BridgeMetrics;
use sui_bridge::types::{EthEvent, EthLog};

use crate::indexer_builder::{
    DataFilter, DataMapper, Datasource, IndexerProgressStore, Persistent,
};
use crate::sui_bridge_indexer::PgBridgePersistent;
use crate::{
    BridgeDataSource, ProcessedTxnData, TokenTransfer, TokenTransferData, TokenTransferStatus,
};

type EthData = (EthLog, Block<H256>, Transaction);

pub struct EthFinalizedDatasource {
    bridge_address: EthAddress,
    eth_rpc_url: String,
    bridge_metrics: Arc<BridgeMetrics>,
}

impl EthFinalizedDatasource {
    pub fn new(
        eth_sui_bridge_contract_address: String,
        eth_rpc_url: String,
        bridge_metrics: Arc<BridgeMetrics>,
    ) -> Result<Self, anyhow::Error> {
        let bridge_address = EthAddress::from_str(&eth_sui_bridge_contract_address)?;
        Ok(Self {
            bridge_address,
            eth_rpc_url,
            bridge_metrics,
        })
    }
}

#[async_trait]
impl Datasource<EthData, PgBridgePersistent, ProcessedTxnData> for EthFinalizedDatasource {
    async fn start_ingestion_task<F, M>(
        &self,
        task_name: String,
        target_checkpoint: u64,
        mut storage: PgBridgePersistent,
        _filter: F,
        data_mapper: M,
    ) -> Result<(), Error>
    where
        F: DataFilter<EthData> + 'static,
        M: DataMapper<EthData, ProcessedTxnData> + 'static,
    {
        let eth_client = Arc::new(
            EthClient::<Http>::new(
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

        let newest_finalized_block = storage.load(task_name.clone()).await?;
        info!("Starting from finalized block: {}", newest_finalized_block);

        let finalized_contract_addresses =
            HashMap::from_iter(vec![(self.bridge_address, newest_finalized_block)]);

        let (_task_handles, mut eth_events_rx, _) =
            EthSyncer::new(eth_client, finalized_contract_addresses)
                .run(self.bridge_metrics.clone())
                .await
                .map_err(|e| anyhow!(format!("{e:?}")))?;

        'outer: while let Some((_, _, logs)) = eth_events_rx.recv().await {
            // TODO: This for-loop can be optimzied to group tx / block info
            // and reduce the queries issued to eth full node
            for log in logs.iter() {
                let block_number = log.block_number();
                let block = provider.get_block(block_number).await?.unwrap();
                let tx_hash = log.tx_hash();
                let transaction = provider.get_transaction(tx_hash).await?.unwrap();
                storage.write(data_mapper.map((log.clone(), block, transaction))?)?;
                storage.save(task_name.clone(), block_number).await?;
                if log.block_number >= target_checkpoint {
                    break 'outer;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct EthDataMapper {
    pub finalized: bool,
}

impl DataMapper<EthData, ProcessedTxnData> for EthDataMapper {
    fn map(&self, (log, block, transaction): EthData) -> Result<Vec<ProcessedTxnData>, Error> {
        let eth_bridge_event = EthBridgeEvent::try_from_log(log.log());
        if eth_bridge_event.is_none() {
            return Ok(vec![]);
        }
        // todo: metrics.total_eth_bridge_transactions.inc();
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
                        log.block_number
                    );
                    if self.finalized {
                        // todo: metrics.total_eth_token_deposited.inc();
                    }
                    ProcessedTxnData::TokenTransfer(TokenTransfer {
                        chain_id: bridge_event.source_chain_id,
                        nonce: bridge_event.nonce,
                        block_height: log.block_number,
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
                    // todo: metrics.total_eth_token_transfer_claimed.inc();
                    ProcessedTxnData::TokenTransfer(TokenTransfer {
                        chain_id: bridge_event.source_chain_id,
                        nonce: bridge_event.nonce,
                        block_height: log.block_number,
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
                    // todo: metrics.total_eth_bridge_txn_other.inc();
                    return Ok(vec![]);
                }
            },
            EthBridgeEvent::EthBridgeCommitteeEvents(_)
            | EthBridgeEvent::EthBridgeLimiterEvents(_)
            | EthBridgeEvent::EthBridgeConfigEvents(_)
            | EthBridgeEvent::EthCommitteeUpgradeableContractEvents(_) => {
                // todo: metrics.total_eth_bridge_txn_other.inc();
                return Ok(vec![]);
            }
        };
        Ok(vec![transfer])
    }
}
