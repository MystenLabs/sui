// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::{BridgeError, BridgeResult};
use crate::types::{BridgeAction, EthLog, EthToSuiBridgeAction};
use ethers::{
    abi::RawLog,
    contract::{abigen, EthLogDecode},
    types::Address as EthAddress,
};
use serde::{Deserialize, Serialize};
use sui_types::base_types::SuiAddress;
use sui_types::bridge::{BridgeChainId, TokenId};

// TODO: write a macro to handle variants

// TODO: Add other events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EthBridgeEvent {
    EthSuiBridgeEvents(EthSuiBridgeEvents),
}

abigen!(
    EthSuiBridge,
    "abi/sui_bridge.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    EthBridgeCommittee,
    "abi/bridge_committee.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    EthBridgeVault,
    "abi/bridge_vault.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    EthBridgeLimiter,
    "abi/bridge_limiter.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

abigen!(
    EthBridgeConfig,
    "abi/bridge_config.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

impl EthBridgeEvent {
    pub fn try_from_eth_log(log: &EthLog) -> Option<EthBridgeEvent> {
        let raw_log = RawLog {
            topics: log.log.topics.clone(),
            data: log.log.data.to_vec(),
        };

        if let Ok(decoded) = EthSuiBridgeEvents::decode_log(&raw_log) {
            return Some(EthBridgeEvent::EthSuiBridgeEvents(decoded));
        }

        // TODO: try other variants
        None
    }
}

impl EthBridgeEvent {
    pub fn try_into_bridge_action(
        self,
        eth_tx_hash: ethers::types::H256,
        eth_event_index: u16,
    ) -> Option<BridgeAction> {
        match self {
            EthBridgeEvent::EthSuiBridgeEvents(event) => {
                match event {
                    EthSuiBridgeEvents::TokensDepositedFilter(event) => {
                        let bridge_event = match EthToSuiTokenBridgeV1::try_from(&event) {
                            Ok(bridge_event) => bridge_event,
                            // This only happens when solidity code does not align with rust code.
                            // When this happens in production, there is a risk of stuck bridge transfers.
                            // We log error here.
                            // TODO: add metrics and alert
                            Err(e) => {
                                tracing::error!(?eth_tx_hash, eth_event_index, "Failed to convert TokensDepositedFilter log to EthToSuiTokenBridgeV1. This indicates incorrect parameters or a bug in the code: {:?}. Err: {:?}", event, e);
                                return None;
                            }
                        };

                        Some(BridgeAction::EthToSuiBridgeAction(EthToSuiBridgeAction {
                            eth_tx_hash,
                            eth_event_index,
                            eth_bridge_event: bridge_event,
                        }))
                    }
                    EthSuiBridgeEvents::TokensClaimedFilter(_event) => None,
                    EthSuiBridgeEvents::PausedFilter(_event) => None,
                    EthSuiBridgeEvents::UnpausedFilter(_event) => None,
                    EthSuiBridgeEvents::UpgradedFilter(_event) => None,
                    EthSuiBridgeEvents::InitializedFilter(_event) => None,
                }
            }
        }
    }
}

/// The event emitted when tokens are deposited into the bridge on Ethereum.
/// Sanity checked version of TokensDepositedFilter
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Hash)]
pub struct EthToSuiTokenBridgeV1 {
    pub nonce: u64,
    pub sui_chain_id: BridgeChainId,
    pub eth_chain_id: BridgeChainId,
    pub sui_address: SuiAddress,
    pub eth_address: EthAddress,
    pub token_id: TokenId,
    pub sui_adjusted_amount: u64,
}

impl TryFrom<&TokensDepositedFilter> for EthToSuiTokenBridgeV1 {
    type Error = BridgeError;
    fn try_from(event: &TokensDepositedFilter) -> BridgeResult<Self> {
        Ok(Self {
            nonce: event.nonce,
            sui_chain_id: BridgeChainId::try_from(event.destination_chain_id)?,
            eth_chain_id: BridgeChainId::try_from(event.source_chain_id)?,
            sui_address: SuiAddress::from_bytes(event.recipient_address.as_ref())?,
            eth_address: event.sender_address,
            token_id: TokenId::try_from(event.token_id)?,
            sui_adjusted_amount: event.sui_adjusted_amount,
        })
    }
}
