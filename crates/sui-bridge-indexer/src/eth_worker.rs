// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::postgres_manager::{write, PgPool};
use crate::{BridgeDataSource, TokenTransfer, TokenTransferData, TokenTransferStatus};
use ethers::providers::Provider;
use ethers::providers::{Http, Middleware};
use ethers::types::Address as EthAddress;
use std::sync::Arc;
use sui_bridge::abi::{EthBridgeEvent, EthSuiBridgeEvents};
use sui_bridge::types::EthLog;
use tracing::info;
use tracing::log::error;

pub async fn process_eth_events(
    mut eth_events_rx: mysten_metrics::metered_channel::Receiver<(EthAddress, u64, Vec<EthLog>)>,
    provider: Arc<Provider<Http>>,
    pool: &PgPool,
    finalized: bool,
) {
    while let Some((_, _, logs)) = eth_events_rx.recv().await {
        for log in logs.iter() {
            let eth_bridge_event = EthBridgeEvent::try_from_eth_log(log);
            if eth_bridge_event.is_none() {
                continue;
            }
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
                            "Observed {} Eth Deposit",
                            if finalized {
                                "Finalized"
                            } else {
                                "Unfinalized"
                            }
                        );
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
                        continue;
                    }
                },
                EthBridgeEvent::EthBridgeCommitteeEvents(_)
                | EthBridgeEvent::EthBridgeLimiterEvents(_)
                | EthBridgeEvent::EthBridgeConfigEvents(_)
                | EthBridgeEvent::EthCommitteeUpgradeableContractEvents(_) => {
                    continue;
                }
            };

            if let Err(e) = write(pool, vec![transfer]) {
                error!("Error writing token transfer to database: {:?}", e);
            }
        }
    }

    panic!("Eth event stream ended unexpectedly");
}
