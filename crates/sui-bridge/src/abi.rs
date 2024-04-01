// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::encoding::{
    BridgeMessageEncoding, ADD_TOKENS_ON_EVM_MESSAGE_VERSION, ASSET_PRICE_UPDATE_MESSAGE_VERSION,
    LIMIT_UPDATE_MESSAGE_VERSION,
};
use crate::encoding::{
    COMMITTEE_BLOCKLIST_MESSAGE_VERSION, EMERGENCY_BUTTON_MESSAGE_VERSION,
    TOKEN_TRANSFER_MESSAGE_VERSION,
};
use crate::error::{BridgeError, BridgeResult};
use crate::types::{
    AddTokensOnEvmAction, AssetPriceUpdateAction, BlocklistCommitteeAction, BridgeAction,
    BridgeActionType, EmergencyAction, EthLog, EthToSuiBridgeAction, LimitUpdateAction,
    SuiToEthBridgeAction,
};
use ethers::types::Log;
use ethers::{
    abi::RawLog,
    contract::{abigen, EthLogDecode},
    types::Address as EthAddress,
};
use serde::{Deserialize, Serialize};
use sui_types::base_types::SuiAddress;
use sui_types::bridge::BridgeChainId;

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

    pub fn try_from_log(log: &Log) -> Option<EthBridgeEvent> {
        let raw_log = RawLog {
            topics: log.topics.clone(),
            data: log.data.to_vec(),
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
    pub token_id: u8,
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
            token_id: event.token_id,
            sui_adjusted_amount: event.sui_adjusted_amount,
        })
    }
}

////////////////////////////////////////////////////////////////////////
//                        Eth Message Conversion                      //
////////////////////////////////////////////////////////////////////////

// TODO: add EvmContractUpgradeAction and tests

impl From<SuiToEthBridgeAction> for eth_sui_bridge::Message {
    fn from(action: SuiToEthBridgeAction) -> Self {
        eth_sui_bridge::Message {
            message_type: BridgeActionType::TokenTransfer as u8,
            version: TOKEN_TRANSFER_MESSAGE_VERSION,
            nonce: action.sui_bridge_event.nonce,
            chain_id: action.sui_bridge_event.sui_chain_id as u8,
            payload: action.as_payload_bytes().into(),
        }
    }
}

impl From<EmergencyAction> for eth_sui_bridge::Message {
    fn from(action: EmergencyAction) -> Self {
        eth_sui_bridge::Message {
            message_type: BridgeActionType::EmergencyButton as u8,
            version: EMERGENCY_BUTTON_MESSAGE_VERSION,
            nonce: action.nonce,
            chain_id: action.chain_id as u8,
            payload: action.as_payload_bytes().into(),
        }
    }
}

impl From<BlocklistCommitteeAction> for eth_bridge_committee::Message {
    fn from(action: BlocklistCommitteeAction) -> Self {
        eth_bridge_committee::Message {
            message_type: BridgeActionType::UpdateCommitteeBlocklist as u8,
            version: COMMITTEE_BLOCKLIST_MESSAGE_VERSION,
            nonce: action.nonce,
            chain_id: action.chain_id as u8,
            payload: action.as_payload_bytes().into(),
        }
    }
}

impl From<LimitUpdateAction> for eth_bridge_limiter::Message {
    fn from(action: LimitUpdateAction) -> Self {
        eth_bridge_limiter::Message {
            message_type: BridgeActionType::LimitUpdate as u8,
            version: LIMIT_UPDATE_MESSAGE_VERSION,
            nonce: action.nonce,
            chain_id: action.chain_id as u8,
            payload: action.as_payload_bytes().into(),
        }
    }
}

impl From<AssetPriceUpdateAction> for eth_bridge_config::Message {
    fn from(action: AssetPriceUpdateAction) -> Self {
        eth_bridge_config::Message {
            message_type: BridgeActionType::AssetPriceUpdate as u8,
            version: ASSET_PRICE_UPDATE_MESSAGE_VERSION,
            nonce: action.nonce,
            chain_id: action.chain_id as u8,
            payload: action.as_payload_bytes().into(),
        }
    }
}

impl From<AddTokensOnEvmAction> for eth_bridge_config::Message {
    fn from(action: AddTokensOnEvmAction) -> Self {
        eth_bridge_config::Message {
            message_type: BridgeActionType::AddTokensOnEvm as u8,
            version: ADD_TOKENS_ON_EVM_MESSAGE_VERSION,
            nonce: action.nonce,
            chain_id: action.chain_id as u8,
            payload: action.as_payload_bytes().into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::BridgeAuthorityPublicKeyBytes,
        types::{BlocklistType, EmergencyActionType},
    };
    use fastcrypto::encoding::{Encoding, Hex};
    use sui_types::{bridge::TOKEN_ID_ETH, crypto::ToFromBytes};

    #[test]
    fn test_eth_message_conversion_emergency_action_regression() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();

        let action = EmergencyAction {
            nonce: 2,
            chain_id: BridgeChainId::EthSepolia,
            action_type: EmergencyActionType::Pause,
        };
        let message: eth_sui_bridge::Message = action.into();
        assert_eq!(
            message,
            eth_sui_bridge::Message {
                message_type: BridgeActionType::EmergencyButton as u8,
                version: EMERGENCY_BUTTON_MESSAGE_VERSION,
                nonce: 2,
                chain_id: BridgeChainId::EthSepolia as u8,
                payload: vec![0].into(),
            }
        );
        Ok(())
    }

    #[test]
    fn test_eth_message_conversion_update_blocklist_action_regression() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let pub_key_bytes = BridgeAuthorityPublicKeyBytes::from_bytes(
            &Hex::decode("02321ede33d2c2d7a8a152f275a1484edef2098f034121a602cb7d767d38680aa4")
                .unwrap(),
        )
        .unwrap();
        let action = BlocklistCommitteeAction {
            nonce: 0,
            chain_id: BridgeChainId::EthSepolia,
            blocklist_type: BlocklistType::Blocklist,
            blocklisted_members: vec![pub_key_bytes],
        };
        let message: eth_bridge_committee::Message = action.into();
        assert_eq!(
            message,
            eth_bridge_committee::Message {
                message_type: BridgeActionType::UpdateCommitteeBlocklist as u8,
                version: COMMITTEE_BLOCKLIST_MESSAGE_VERSION,
                nonce: 0,
                chain_id: BridgeChainId::EthSepolia as u8,
                payload: Hex::decode("000168b43fd906c0b8f024a18c56e06744f7c6157c65")
                    .unwrap()
                    .into(),
            }
        );
        Ok(())
    }

    #[test]
    fn test_eth_message_conversion_update_limit_action_regression() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let action = LimitUpdateAction {
            nonce: 2,
            chain_id: BridgeChainId::EthSepolia,
            sending_chain_id: BridgeChainId::SuiTestnet,
            new_usd_limit: 4200000,
        };
        let message: eth_bridge_limiter::Message = action.into();
        assert_eq!(
            message,
            eth_bridge_limiter::Message {
                message_type: BridgeActionType::LimitUpdate as u8,
                version: LIMIT_UPDATE_MESSAGE_VERSION,
                nonce: 2,
                chain_id: BridgeChainId::EthSepolia as u8,
                payload: Hex::decode("010000000000401640").unwrap().into(),
            }
        );
        Ok(())
    }

    #[test]
    fn test_eth_message_conversion_update_price_action_regression() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let action = AssetPriceUpdateAction {
            nonce: 2,
            chain_id: BridgeChainId::EthSepolia,
            token_id: TOKEN_ID_ETH,
            new_usd_price: 80000000,
        };
        let message: eth_bridge_config::Message = action.into();
        assert_eq!(
            message,
            eth_bridge_config::Message {
                message_type: BridgeActionType::AssetPriceUpdate as u8,
                version: ASSET_PRICE_UPDATE_MESSAGE_VERSION,
                nonce: 2,
                chain_id: BridgeChainId::EthSepolia as u8,
                payload: Hex::decode("020000000004c4b400").unwrap().into(),
            }
        );
        Ok(())
    }

    #[test]
    fn test_eth_message_conversion_add_tokens_on_evm_action_regression() -> anyhow::Result<()> {
        let action = AddTokensOnEvmAction {
            nonce: 5,
            chain_id: BridgeChainId::EthLocalTest,
            native: true,
            token_ids: vec![99, 100, 101],
            token_addresses: vec![
                EthAddress::repeat_byte(1),
                EthAddress::repeat_byte(2),
                EthAddress::repeat_byte(3),
            ],
            token_sui_decimals: vec![5, 6, 7],
            token_prices: vec![1_000_000_000, 2_000_000_000, 3_000_000_000],
        };
        let message: eth_bridge_config::Message = action.into();
        assert_eq!(
            message,
            eth_bridge_config::Message {
                message_type: BridgeActionType::AddTokensOnEvm as u8,
                version: ADD_TOKENS_ON_EVM_MESSAGE_VERSION,
                nonce: 5,
                chain_id: BridgeChainId::EthLocalTest as u8,
                payload: Hex::decode("0103636465030101010101010101010101010101010101010101020202020202020202020202020202020202020203030303030303030303030303030303030303030305060703000000003b9aca00000000007735940000000000b2d05e00").unwrap().into(),
            }
        );
        Ok(())
    }
}
