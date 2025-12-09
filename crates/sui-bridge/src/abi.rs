// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::encoding::{
    ADD_TOKENS_ON_EVM_MESSAGE_VERSION, ASSET_PRICE_UPDATE_MESSAGE_VERSION, BridgeMessageEncoding,
    EVM_CONTRACT_UPGRADE_MESSAGE_VERSION, LIMIT_UPDATE_MESSAGE_VERSION,
};
use crate::encoding::{
    COMMITTEE_BLOCKLIST_MESSAGE_VERSION, EMERGENCY_BUTTON_MESSAGE_VERSION,
    TOKEN_TRANSFER_MESSAGE_VERSION,
};
use crate::error::{BridgeError, BridgeResult};
use crate::types::ParsedTokenTransferMessage;
use crate::types::{
    AddTokensOnEvmAction, AssetPriceUpdateAction, BlocklistCommitteeAction, BridgeAction,
    BridgeActionType, EmergencyAction, EthLog, EthToSuiBridgeAction, EvmContractUpgradeAction,
    LimitUpdateAction, SuiToEthBridgeAction, SuiToEthTokenTransfer,
};
use alloy::primitives::{Address as EthAddress, TxHash};
use alloy::rpc::types::eth::Log;
use alloy::sol;
use alloy::sol_types::SolEventInterface;
use serde::{Deserialize, Serialize};
use sui_types::base_types::SuiAddress;
use sui_types::bridge::BridgeChainId;

macro_rules! gen_eth_events {
    // Contracts with Events
    ($($contract:ident, $module:ident, $contract_event:ident, $abi_path:literal),* $(,)?) => {
        $(
            // We must isolate the code sol! generates in a module or we will get an error for
            // BridgeUtils being duplicated
            pub mod $module {
                alloy::sol!(
                    #[sol(rpc, all_derives, extra_derives(serde::Serialize, serde::Deserialize))]
                    $contract,
                    $abi_path,
                );
            }

            // Re-export everything to the main scope
            pub use $module::$contract;
            #[allow(ambiguous_glob_reexports)]
            pub use $module::$contract::*;
        )*

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        pub enum EthBridgeEvent {
            $(
                $contract_event($contract_event),
            )*
        }

        impl EthBridgeEvent {
            pub fn try_from_eth_log(log: &EthLog) -> Option<EthBridgeEvent> {
                Self::try_from_log(&log.log)
            }

            pub fn try_from_log(log: &Log) -> Option<EthBridgeEvent> {
                $(
                    if let Ok(decoded) = $contract_event::decode_raw_log(log.topics(), &log.data().data) {
                        return Some(EthBridgeEvent::$contract_event(decoded));
                    }
                )*

                None
            }
        }
    };

    // Contracts without Events
    ($($contract:ident, $module:ident, $abi_path:literal),* $(,)?) => {
        $(
            pub mod $module {
                alloy::sol!(
                    #[sol(rpc, extra_derives(serde::Serialize, serde::Deserialize))]
                    $contract,
                    $abi_path,
                );
            }

            pub use $module::$contract;
        )*
    };
}

#[rustfmt::skip]
gen_eth_events!(
    // Format: ContractStruct, ModuleName, EventStruct, AbiPath
    EthSuiBridge, eth_sui_bridge, EthSuiBridgeEvents, "abi/sui_bridge.json",
    EthBridgeCommittee, eth_bridge_committee, EthBridgeCommitteeEvents, "abi/bridge_committee.json",
    EthBridgeLimiter, eth_bridge_limiter, EthBridgeLimiterEvents, "abi/bridge_limiter.json",
    EthBridgeConfig, eth_bridge_config, EthBridgeConfigEvents, "abi/bridge_config.json",
    EthCommitteeUpgradeableContract, eth_committee_upgradeable_contract, EthCommitteeUpgradeableContractEvents, "abi/bridge_committee_upgradeable.json",
);

gen_eth_events!(
    // Format: ContractStruct, ModuleName, AbiPath
    EthBridgeVault,
    eth_bridge_vault,
    "abi/bridge_vault.json"
);

sol!(
    #[sol(rpc, extra_derives(serde::Serialize, serde::Deserialize))]
    EthERC20,
    "abi/erc20.json",
);

impl EthBridgeEvent {
    pub fn try_into_bridge_action(
        self,
        eth_tx_hash: TxHash,
        eth_event_index: u16,
    ) -> BridgeResult<Option<BridgeAction>> {
        Ok(match self {
            EthBridgeEvent::EthSuiBridgeEvents(event) => {
                match event {
                    EthSuiBridgeEvents::TokensDeposited(event) => {
                        let bridge_event = match EthToSuiTokenBridgeV1::try_from(&event) {
                            Ok(bridge_event) => {
                                if bridge_event.sui_adjusted_amount == 0 {
                                    return Err(BridgeError::ZeroValueBridgeTransfer(format!(
                                        "Manual intervention is required: {}",
                                        eth_tx_hash
                                    )));
                                }
                                bridge_event
                            }
                            // This only happens when solidity code does not align with rust code.
                            // When this happens in production, there is a risk of stuck bridge transfers.
                            // We log error here.
                            // TODO: add metrics and alert
                            Err(e) => {
                                return Err(BridgeError::Generic(format!(
                                    "Manual intervention is required. Failed to convert TokensDepositedFilter log to EthToSuiTokenBridgeV1. This indicates incorrect parameters or a bug in the code: {:?}. Err: {:?}",
                                    event, e
                                )));
                            }
                        };

                        Some(BridgeAction::EthToSuiBridgeAction(EthToSuiBridgeAction {
                            eth_tx_hash,
                            eth_event_index,
                            eth_bridge_event: bridge_event,
                        }))
                    }
                    EthSuiBridgeEvents::TokensClaimed(_event) => None,
                    EthSuiBridgeEvents::Paused(_event) => None,
                    EthSuiBridgeEvents::Unpaused(_event) => None,
                    EthSuiBridgeEvents::Upgraded(_event) => None,
                    EthSuiBridgeEvents::Initialized(_event) => None,
                    EthSuiBridgeEvents::ContractUpgraded(_event) => None,
                    EthSuiBridgeEvents::EmergencyOperation(_event) => None,
                }
            }
            EthBridgeEvent::EthBridgeCommitteeEvents(event) => match event {
                EthBridgeCommitteeEvents::BlocklistUpdated(_event) => None,
                EthBridgeCommitteeEvents::Initialized(_event) => None,
                EthBridgeCommitteeEvents::Upgraded(_event) => None,
                EthBridgeCommitteeEvents::BlocklistUpdatedV2(_event) => None,
                EthBridgeCommitteeEvents::ContractUpgraded(_event) => None,
            },
            EthBridgeEvent::EthBridgeLimiterEvents(event) => match event {
                EthBridgeLimiterEvents::LimitUpdated(_event) => None,
                EthBridgeLimiterEvents::Initialized(_event) => None,
                EthBridgeLimiterEvents::Upgraded(_event) => None,
                EthBridgeLimiterEvents::HourlyTransferAmountUpdated(_event) => None,
                EthBridgeLimiterEvents::OwnershipTransferred(_event) => None,
                EthBridgeLimiterEvents::ContractUpgraded(_event) => None,
                EthBridgeLimiterEvents::LimitUpdatedV2(_event) => None,
            },
            EthBridgeEvent::EthBridgeConfigEvents(event) => match event {
                EthBridgeConfigEvents::Initialized(_event) => None,
                EthBridgeConfigEvents::Upgraded(_event) => None,
                EthBridgeConfigEvents::TokenAdded(_event) => None,
                EthBridgeConfigEvents::TokenPriceUpdated(_event) => None,
                EthBridgeConfigEvents::ContractUpgraded(_event) => None,
                EthBridgeConfigEvents::TokenPriceUpdatedV2(_event) => None,
                EthBridgeConfigEvents::TokensAddedV2(_event) => None,
            },
            EthBridgeEvent::EthCommitteeUpgradeableContractEvents(event) => match event {
                EthCommitteeUpgradeableContractEvents::Initialized(_event) => None,
                EthCommitteeUpgradeableContractEvents::Upgraded(_event) => None,
            },
        })
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

impl TryFrom<&TokensDeposited> for EthToSuiTokenBridgeV1 {
    type Error = BridgeError;

    fn try_from(event: &TokensDeposited) -> BridgeResult<Self> {
        Ok(Self {
            nonce: event.nonce,
            sui_chain_id: BridgeChainId::try_from(event.destinationChainID)?,
            eth_chain_id: BridgeChainId::try_from(event.sourceChainID)?,
            sui_address: SuiAddress::from_bytes(event.recipientAddress.as_ref())?,
            eth_address: event.senderAddress,
            token_id: event.tokenID,
            sui_adjusted_amount: event.suiAdjustedAmount,
        })
    }
}

////////////////////////////////////////////////////////////////////////
//                        Eth Message Conversion                      //
////////////////////////////////////////////////////////////////////////

impl TryFrom<SuiToEthBridgeAction> for eth_sui_bridge::BridgeUtils::Message {
    type Error = BridgeError;

    fn try_from(action: SuiToEthBridgeAction) -> BridgeResult<Self> {
        Ok(eth_sui_bridge::BridgeUtils::Message {
            messageType: BridgeActionType::TokenTransfer as u8,
            version: TOKEN_TRANSFER_MESSAGE_VERSION,
            nonce: action.sui_bridge_event.nonce,
            chainID: action.sui_bridge_event.sui_chain_id as u8,
            payload: action
                .as_payload_bytes()
                .map_err(|e| BridgeError::Generic(format!("Failed to encode payload: {}", e)))?
                .into(),
        })
    }
}

impl TryFrom<SuiToEthTokenTransfer> for eth_sui_bridge::BridgeUtils::Message {
    type Error = BridgeError;

    fn try_from(action: SuiToEthTokenTransfer) -> BridgeResult<Self> {
        Ok(eth_sui_bridge::BridgeUtils::Message {
            messageType: BridgeActionType::TokenTransfer as u8,
            version: TOKEN_TRANSFER_MESSAGE_VERSION,
            nonce: action.nonce,
            chainID: action.sui_chain_id as u8,
            payload: action
                .as_payload_bytes()
                .map_err(|e| BridgeError::Generic(format!("Failed to encode payload: {}", e)))?
                .into(),
        })
    }
}

impl From<ParsedTokenTransferMessage> for eth_sui_bridge::BridgeUtils::Message {
    fn from(parsed_message: ParsedTokenTransferMessage) -> Self {
        eth_sui_bridge::BridgeUtils::Message {
            messageType: BridgeActionType::TokenTransfer as u8,
            version: parsed_message.message_version,
            nonce: parsed_message.seq_num,
            chainID: parsed_message.source_chain as u8,
            payload: parsed_message.payload.into(),
        }
    }
}

impl TryFrom<EmergencyAction> for eth_sui_bridge::BridgeUtils::Message {
    type Error = BridgeError;

    fn try_from(action: EmergencyAction) -> BridgeResult<Self> {
        Ok(eth_sui_bridge::BridgeUtils::Message {
            messageType: BridgeActionType::EmergencyButton as u8,
            version: EMERGENCY_BUTTON_MESSAGE_VERSION,
            nonce: action.nonce,
            chainID: action.chain_id as u8,
            payload: action
                .as_payload_bytes()
                .map_err(|e| BridgeError::Generic(format!("Failed to encode payload: {}", e)))?
                .into(),
        })
    }
}

impl TryFrom<BlocklistCommitteeAction> for eth_bridge_committee::BridgeUtils::Message {
    type Error = BridgeError;

    fn try_from(action: BlocklistCommitteeAction) -> BridgeResult<Self> {
        Ok(eth_bridge_committee::BridgeUtils::Message {
            messageType: BridgeActionType::UpdateCommitteeBlocklist as u8,
            version: COMMITTEE_BLOCKLIST_MESSAGE_VERSION,
            nonce: action.nonce,
            chainID: action.chain_id as u8,
            payload: action
                .as_payload_bytes()
                .map_err(|e| BridgeError::Generic(format!("Failed to encode payload: {}", e)))?
                .into(),
        })
    }
}

impl TryFrom<LimitUpdateAction> for eth_bridge_limiter::BridgeUtils::Message {
    type Error = BridgeError;

    fn try_from(action: LimitUpdateAction) -> BridgeResult<Self> {
        Ok(eth_bridge_limiter::BridgeUtils::Message {
            messageType: BridgeActionType::LimitUpdate as u8,
            version: LIMIT_UPDATE_MESSAGE_VERSION,
            nonce: action.nonce,
            chainID: action.chain_id as u8,
            payload: action
                .as_payload_bytes()
                .map_err(|e| BridgeError::Generic(format!("Failed to encode payload: {}", e)))?
                .into(),
        })
    }
}

impl TryFrom<AssetPriceUpdateAction> for eth_bridge_config::BridgeUtils::Message {
    type Error = BridgeError;

    fn try_from(action: AssetPriceUpdateAction) -> BridgeResult<Self> {
        Ok(eth_bridge_config::BridgeUtils::Message {
            messageType: BridgeActionType::AssetPriceUpdate as u8,
            version: ASSET_PRICE_UPDATE_MESSAGE_VERSION,
            nonce: action.nonce,
            chainID: action.chain_id as u8,
            payload: action
                .as_payload_bytes()
                .map_err(|e| BridgeError::Generic(format!("Failed to encode payload: {}", e)))?
                .into(),
        })
    }
}

impl TryFrom<AddTokensOnEvmAction> for eth_bridge_config::BridgeUtils::Message {
    type Error = BridgeError;

    fn try_from(action: AddTokensOnEvmAction) -> BridgeResult<Self> {
        Ok(eth_bridge_config::BridgeUtils::Message {
            messageType: BridgeActionType::AddTokensOnEvm as u8,
            version: ADD_TOKENS_ON_EVM_MESSAGE_VERSION,
            nonce: action.nonce,
            chainID: action.chain_id as u8,
            payload: action
                .as_payload_bytes()
                .map_err(|e| BridgeError::Generic(format!("Failed to encode payload: {}", e)))?
                .into(),
        })
    }
}

impl TryFrom<EvmContractUpgradeAction>
    for eth_committee_upgradeable_contract::BridgeUtils::Message
{
    type Error = BridgeError;

    fn try_from(action: EvmContractUpgradeAction) -> BridgeResult<Self> {
        Ok(eth_committee_upgradeable_contract::BridgeUtils::Message {
            messageType: BridgeActionType::EvmContractUpgrade as u8,
            version: EVM_CONTRACT_UPGRADE_MESSAGE_VERSION,
            nonce: action.nonce,
            chainID: action.chain_id as u8,
            payload: action
                .as_payload_bytes()
                .map_err(|e| BridgeError::Generic(format!("Failed to encode payload: {}", e)))?
                .into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::BridgeAuthorityPublicKeyBytes,
        types::{BlocklistType, EmergencyActionType},
    };
    use alloy::primitives::{B256, Bytes, LogData};
    use alloy::sol_types::SolValue;
    use fastcrypto::encoding::{Encoding, Hex};
    use hex_literal::hex;
    use std::str::FromStr;
    use sui_types::{bridge::TOKEN_ID_ETH, crypto::ToFromBytes};

    #[test]
    fn test_eth_message_conversion_emergency_action_regression() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();

        let action = EmergencyAction {
            nonce: 2,
            chain_id: BridgeChainId::EthSepolia,
            action_type: EmergencyActionType::Pause,
        };
        let message: eth_sui_bridge::BridgeUtils::Message = action.try_into().unwrap();
        assert_eq!(
            message,
            eth_sui_bridge::BridgeUtils::Message {
                messageType: BridgeActionType::EmergencyButton as u8,
                version: EMERGENCY_BUTTON_MESSAGE_VERSION,
                nonce: 2,
                chainID: BridgeChainId::EthSepolia as u8,
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
            members_to_update: vec![pub_key_bytes],
        };
        let message: eth_bridge_committee::BridgeUtils::Message = action.try_into().unwrap();
        assert_eq!(
            message,
            eth_bridge_committee::BridgeUtils::Message {
                messageType: BridgeActionType::UpdateCommitteeBlocklist as u8,
                version: COMMITTEE_BLOCKLIST_MESSAGE_VERSION,
                nonce: 0,
                chainID: BridgeChainId::EthSepolia as u8,
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
        let message: eth_bridge_limiter::BridgeUtils::Message = action.try_into().unwrap();
        assert_eq!(
            message,
            eth_bridge_limiter::BridgeUtils::Message {
                messageType: BridgeActionType::LimitUpdate as u8,
                version: LIMIT_UPDATE_MESSAGE_VERSION,
                nonce: 2,
                chainID: BridgeChainId::EthSepolia as u8,
                payload: Hex::decode("010000000000401640").unwrap().into(),
            }
        );
        Ok(())
    }

    #[test]
    fn test_eth_message_conversion_contract_upgrade_action_regression() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let action = EvmContractUpgradeAction {
            nonce: 2,
            chain_id: BridgeChainId::EthSepolia,
            proxy_address: EthAddress::repeat_byte(1),
            new_impl_address: EthAddress::repeat_byte(2),
            call_data: Vec::from("deadbeef"),
        };
        let message: eth_committee_upgradeable_contract::BridgeUtils::Message =
            action.try_into().unwrap();
        assert_eq!(
            message,
            eth_committee_upgradeable_contract::BridgeUtils::Message {
                messageType: BridgeActionType::EvmContractUpgrade as u8,
                version: EVM_CONTRACT_UPGRADE_MESSAGE_VERSION,
                nonce: 2,
                chainID: BridgeChainId::EthSepolia as u8,
                payload: Hex::decode("0x00000000000000000000000001010101010101010101010101010101010101010000000000000000000000000202020202020202020202020202020202020202000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000086465616462656566000000000000000000000000000000000000000000000000").unwrap().into(),
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
        let message: eth_bridge_config::BridgeUtils::Message = action.try_into().unwrap();
        assert_eq!(
            message,
            eth_bridge_config::BridgeUtils::Message {
                messageType: BridgeActionType::AssetPriceUpdate as u8,
                version: ASSET_PRICE_UPDATE_MESSAGE_VERSION,
                nonce: 2,
                chainID: BridgeChainId::EthSepolia as u8,
                payload: Hex::decode("020000000004c4b400").unwrap().into(),
            }
        );
        Ok(())
    }

    #[test]
    fn test_eth_message_conversion_add_tokens_on_evm_action_regression() -> anyhow::Result<()> {
        let action = AddTokensOnEvmAction {
            nonce: 5,
            chain_id: BridgeChainId::EthCustom,
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
        let message: eth_bridge_config::BridgeUtils::Message = action.try_into().unwrap();
        assert_eq!(
            message,
            eth_bridge_config::BridgeUtils::Message {
                messageType: BridgeActionType::AddTokensOnEvm as u8,
                version: ADD_TOKENS_ON_EVM_MESSAGE_VERSION,
                nonce: 5,
                chainID: BridgeChainId::EthCustom as u8,
                payload: Hex::decode("0103636465030101010101010101010101010101010101010101020202020202020202020202020202020202020203030303030303030303030303030303030303030305060703000000003b9aca00000000007735940000000000b2d05e00").unwrap().into(),
            }
        );
        Ok(())
    }

    #[test]
    fn test_token_deposit_eth_log_to_sui_bridge_event_regression() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let tx_hash = TxHash::random();
        let topics: Vec<B256> = vec![
            hex!("a0f1d54820817ede8517e70a3d0a9197c015471c5360d2119b759f0359858ce6").into(),
            hex!("000000000000000000000000000000000000000000000000000000000000000c").into(),
            hex!("0000000000000000000000000000000000000000000000000000000000000000").into(),
            hex!("0000000000000000000000000000000000000000000000000000000000000002").into(),
        ];
        let encoded = (
            Hex::decode("0x000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000fa56ea0000000000000000000000000014dc79964da2c08b23698b3d3cc7ca32193d9955000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000203b1eb23133e94d08d0da9303cfd38e7d4f8f6951f235daa62cd64ea5b6d96d77").unwrap(),
        )
            .abi_encode();
        let log_data = LogData::new(topics, encoded.into()).unwrap();
        let action = EthLog {
            block_number: 33,
            tx_hash,
            log_index_in_tx: 1,
            log: Log {
                inner: alloy::primitives::Log {
                    address: EthAddress::repeat_byte(1),
                    data: log_data,
                },
                block_hash: None,
                block_number: None,
                transaction_hash: Some(tx_hash),
                transaction_index: Some(0),
                log_index: Some(1),
                ..Default::default()
            },
        };
        let event = EthBridgeEvent::try_from_eth_log(&action).unwrap();
        assert_eq!(
            event,
            EthBridgeEvent::EthSuiBridgeEvents(EthSuiBridgeEvents::TokensDeposited(
                TokensDeposited {
                    sourceChainID: 12,
                    nonce: 0,
                    destinationChainID: 2,
                    tokenID: 2,
                    suiAdjustedAmount: 4200000000,
                    senderAddress: EthAddress::from_str(
                        "0x14dc79964da2c08b23698b3d3cc7ca32193d9955"
                    )
                    .unwrap(),
                    recipientAddress: Bytes::from(
                        Hex::decode(
                            "0x3b1eb23133e94d08d0da9303cfd38e7d4f8f6951f235daa62cd64ea5b6d96d77"
                        )
                        .unwrap(),
                    ),
                }
            ))
        );
        Ok(())
    }

    #[test]
    fn test_0_sui_amount_conversion_for_eth_event() {
        let e = EthBridgeEvent::EthSuiBridgeEvents(EthSuiBridgeEvents::TokensDeposited(
            TokensDeposited {
                sourceChainID: BridgeChainId::EthSepolia as u8,
                nonce: 0,
                destinationChainID: BridgeChainId::SuiTestnet as u8,
                tokenID: 2,
                suiAdjustedAmount: 1,
                senderAddress: EthAddress::random(),
                recipientAddress: Bytes::from(SuiAddress::random_for_testing_only().to_vec()),
            },
        ));
        assert!(
            e.try_into_bridge_action(TxHash::random(), 0)
                .unwrap()
                .is_some()
        );

        let e = EthBridgeEvent::EthSuiBridgeEvents(EthSuiBridgeEvents::TokensDeposited(
            TokensDeposited {
                sourceChainID: BridgeChainId::EthSepolia as u8,
                nonce: 0,
                destinationChainID: BridgeChainId::SuiTestnet as u8,
                tokenID: 2,
                suiAdjustedAmount: 0, // <------------
                senderAddress: EthAddress::random(),
                recipientAddress: Bytes::from(SuiAddress::random_for_testing_only().to_vec()),
            },
        ));
        match e.try_into_bridge_action(TxHash::random(), 0).unwrap_err() {
            BridgeError::ZeroValueBridgeTransfer(_) => {}
            e => panic!("Unexpected error: {:?}", e),
        }
    }
}
