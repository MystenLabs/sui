// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::AddTokensOnEvmAction;
use crate::types::AddTokensOnSuiAction;
use crate::types::AssetPriceUpdateAction;
use crate::types::BlocklistCommitteeAction;
use crate::types::BridgeAction;
use crate::types::BridgeActionType;
use crate::types::EmergencyAction;
use crate::types::EthToSuiBridgeAction;
use crate::types::EvmContractUpgradeAction;
use crate::types::LimitUpdateAction;
use crate::types::SuiToEthBridgeAction;
use enum_dispatch::enum_dispatch;
use ethers::types::Address as EthAddress;
use sui_types::base_types::SUI_ADDRESS_LENGTH;

pub const TOKEN_TRANSFER_MESSAGE_VERSION: u8 = 1;
pub const COMMITTEE_BLOCKLIST_MESSAGE_VERSION: u8 = 1;
pub const EMERGENCY_BUTTON_MESSAGE_VERSION: u8 = 1;
pub const LIMIT_UPDATE_MESSAGE_VERSION: u8 = 1;
pub const ASSET_PRICE_UPDATE_MESSAGE_VERSION: u8 = 1;
pub const EVM_CONTRACT_UPGRADE_MESSAGE_VERSION: u8 = 1;
pub const ADD_TOKENS_ON_SUI_MESSAGE_VERSION: u8 = 1;
pub const ADD_TOKENS_ON_EVM_MESSAGE_VERSION: u8 = 1;

pub const BRIDGE_MESSAGE_PREFIX: &[u8] = b"SUI_BRIDGE_MESSAGE";

/// Encoded bridge message consists of the following fields:
/// 1. Message type (1 byte)
/// 2. Message version (1 byte)
/// 3. Nonce (8 bytes in big endian)
/// 4. Chain id (1 byte)
/// 4. Payload (variable length)
#[enum_dispatch]
pub trait BridgeMessageEncoding {
    /// Convert the entire message to bytes
    fn as_bytes(&self) -> Vec<u8>;
    /// Convert the payload piece to bytes
    fn as_payload_bytes(&self) -> Vec<u8>;
}

impl BridgeMessageEncoding for SuiToEthBridgeAction {
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let e = &self.sui_bridge_event;
        // Add message type
        bytes.push(BridgeActionType::TokenTransfer as u8);
        // Add message version
        bytes.push(TOKEN_TRANSFER_MESSAGE_VERSION);
        // Add nonce
        bytes.extend_from_slice(&e.nonce.to_be_bytes());
        // Add source chain id
        bytes.push(e.sui_chain_id as u8);

        // Add payload bytes
        bytes.extend_from_slice(&self.as_payload_bytes());

        bytes
    }

    fn as_payload_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let e = &self.sui_bridge_event;

        // Add source address length
        bytes.push(SUI_ADDRESS_LENGTH as u8);
        // Add source address
        bytes.extend_from_slice(&e.sui_address.to_vec());
        // Add dest chain id
        bytes.push(e.eth_chain_id as u8);
        // Add dest address length
        bytes.push(EthAddress::len_bytes() as u8);
        // Add dest address
        bytes.extend_from_slice(e.eth_address.as_bytes());

        // Add token id
        bytes.push(e.token_id);

        // Add token amount
        bytes.extend_from_slice(&e.amount_sui_adjusted.to_be_bytes());

        bytes
    }
}

impl BridgeMessageEncoding for EthToSuiBridgeAction {
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let e = &self.eth_bridge_event;
        // Add message type
        bytes.push(BridgeActionType::TokenTransfer as u8);
        // Add message version
        bytes.push(TOKEN_TRANSFER_MESSAGE_VERSION);
        // Add nonce
        bytes.extend_from_slice(&e.nonce.to_be_bytes());
        // Add source chain id
        bytes.push(e.eth_chain_id as u8);

        // Add payload bytes
        bytes.extend_from_slice(&self.as_payload_bytes());

        bytes
    }

    fn as_payload_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let e = &self.eth_bridge_event;

        // Add source address length
        bytes.push(EthAddress::len_bytes() as u8);
        // Add source address
        bytes.extend_from_slice(e.eth_address.as_bytes());
        // Add dest chain id
        bytes.push(e.sui_chain_id as u8);
        // Add dest address length
        bytes.push(SUI_ADDRESS_LENGTH as u8);
        // Add dest address
        bytes.extend_from_slice(&e.sui_address.to_vec());

        // Add token id
        bytes.push(e.token_id);

        // Add token amount
        bytes.extend_from_slice(&e.sui_adjusted_amount.to_be_bytes());

        bytes
    }
}

impl BridgeMessageEncoding for BlocklistCommitteeAction {
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add message type
        bytes.push(BridgeActionType::UpdateCommitteeBlocklist as u8);
        // Add message version
        bytes.push(COMMITTEE_BLOCKLIST_MESSAGE_VERSION);
        // Add nonce
        bytes.extend_from_slice(&self.nonce.to_be_bytes());
        // Add chain id
        bytes.push(self.chain_id as u8);

        // Add payload bytes
        bytes.extend_from_slice(&self.as_payload_bytes());

        bytes
    }

    fn as_payload_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Add blocklist type
        bytes.push(self.blocklist_type as u8);
        // Add length of updated members.
        // Unwrap: It should not overflow given what we have today.
        bytes.push(u8::try_from(self.members_to_update.len()).unwrap());

        // Add list of updated members
        // Members are represented as pubkey derived evm addresses (20 bytes)
        let members_bytes = self
            .members_to_update
            .iter()
            .map(|m| m.to_eth_address().to_fixed_bytes().to_vec())
            .collect::<Vec<_>>();
        for members_bytes in members_bytes {
            bytes.extend_from_slice(&members_bytes);
        }

        bytes
    }
}

impl BridgeMessageEncoding for EmergencyAction {
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add message type
        bytes.push(BridgeActionType::EmergencyButton as u8);
        // Add message version
        bytes.push(EMERGENCY_BUTTON_MESSAGE_VERSION);
        // Add nonce
        bytes.extend_from_slice(&self.nonce.to_be_bytes());
        // Add chain id
        bytes.push(self.chain_id as u8);

        // Add payload bytes
        bytes.extend_from_slice(&self.as_payload_bytes());

        bytes
    }

    fn as_payload_bytes(&self) -> Vec<u8> {
        vec![self.action_type as u8]
    }
}

impl BridgeMessageEncoding for LimitUpdateAction {
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add message type
        bytes.push(BridgeActionType::LimitUpdate as u8);
        // Add message version
        bytes.push(LIMIT_UPDATE_MESSAGE_VERSION);
        // Add nonce
        bytes.extend_from_slice(&self.nonce.to_be_bytes());
        // Add chain id
        bytes.push(self.chain_id as u8);

        // Add payload bytes
        bytes.extend_from_slice(&self.as_payload_bytes());

        bytes
    }

    fn as_payload_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add sending chain id
        bytes.push(self.sending_chain_id as u8);
        // Add new usd limit
        bytes.extend_from_slice(&self.new_usd_limit.to_be_bytes());
        bytes
    }
}

impl BridgeMessageEncoding for AssetPriceUpdateAction {
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add message type
        bytes.push(BridgeActionType::AssetPriceUpdate as u8);
        // Add message version
        bytes.push(EMERGENCY_BUTTON_MESSAGE_VERSION);
        // Add nonce
        bytes.extend_from_slice(&self.nonce.to_be_bytes());
        // Add chain id
        bytes.push(self.chain_id as u8);

        // Add payload bytes
        bytes.extend_from_slice(&self.as_payload_bytes());

        bytes
    }

    fn as_payload_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add token id
        bytes.push(self.token_id);
        // Add new usd limit
        bytes.extend_from_slice(&self.new_usd_price.to_be_bytes());
        bytes
    }
}

impl BridgeMessageEncoding for EvmContractUpgradeAction {
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add message type
        bytes.push(BridgeActionType::EvmContractUpgrade as u8);
        // Add message version
        bytes.push(EVM_CONTRACT_UPGRADE_MESSAGE_VERSION);
        // Add nonce
        bytes.extend_from_slice(&self.nonce.to_be_bytes());
        // Add chain id
        bytes.push(self.chain_id as u8);

        // Add payload bytes
        bytes.extend_from_slice(&self.as_payload_bytes());

        bytes
    }

    fn as_payload_bytes(&self) -> Vec<u8> {
        ethers::abi::encode(&[
            ethers::abi::Token::Address(self.proxy_address),
            ethers::abi::Token::Address(self.new_impl_address),
            ethers::abi::Token::Bytes(self.call_data.clone()),
        ])
    }
}

impl BridgeMessageEncoding for AddTokensOnSuiAction {
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add message type
        bytes.push(BridgeActionType::AddTokensOnSui as u8);
        // Add message version
        bytes.push(ADD_TOKENS_ON_SUI_MESSAGE_VERSION);
        // Add nonce
        bytes.extend_from_slice(&self.nonce.to_be_bytes());
        // Add chain id
        bytes.push(self.chain_id as u8);

        // Add payload bytes
        bytes.extend_from_slice(&self.as_payload_bytes());

        bytes
    }

    fn as_payload_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add native
        bytes.push(self.native as u8);
        // Add token ids
        // Unwrap: bcs serialization should not fail
        bytes.extend_from_slice(&bcs::to_bytes(&self.token_ids).unwrap());

        // Add token type names
        // Unwrap: bcs serialization should not fail
        bytes.extend_from_slice(
            &bcs::to_bytes(
                &self
                    .token_type_names
                    .iter()
                    .map(|m| m.to_canonical_string(false))
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        );

        // Add token prices
        // Unwrap: bcs serialization should not fail
        bytes.extend_from_slice(&bcs::to_bytes(&self.token_prices).unwrap());

        bytes
    }
}

impl BridgeMessageEncoding for AddTokensOnEvmAction {
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add message type
        bytes.push(BridgeActionType::AddTokensOnEvm as u8);
        // Add message version
        bytes.push(ADD_TOKENS_ON_EVM_MESSAGE_VERSION);
        // Add nonce
        bytes.extend_from_slice(&self.nonce.to_be_bytes());
        // Add chain id
        bytes.push(self.chain_id as u8);

        // Add payload bytes
        bytes.extend_from_slice(&self.as_payload_bytes());

        bytes
    }

    fn as_payload_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add native
        bytes.push(self.native as u8);
        // Add token ids
        // Unwrap: bcs serialization should not fail
        bytes.push(u8::try_from(self.token_ids.len()).unwrap());
        for token_id in &self.token_ids {
            bytes.push(*token_id);
        }

        // Add token addresses
        // Unwrap: bcs serialization should not fail
        bytes.push(u8::try_from(self.token_addresses.len()).unwrap());
        for token_address in &self.token_addresses {
            bytes.extend_from_slice(&token_address.to_fixed_bytes());
        }

        // Add token sui decimals
        // Unwrap: bcs serialization should not fail
        bytes.push(u8::try_from(self.token_sui_decimals.len()).unwrap());
        for token_sui_decimal in &self.token_sui_decimals {
            bytes.push(*token_sui_decimal);
        }

        // Add token prices
        // Unwrap: bcs serialization should not fail
        bytes.push(u8::try_from(self.token_prices.len()).unwrap());
        for token_price in &self.token_prices {
            bytes.extend_from_slice(&token_price.to_be_bytes());
        }
        bytes
    }
}

impl BridgeAction {
    /// Convert to message bytes to verify in Move and Solidity
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add prefix
        bytes.extend_from_slice(BRIDGE_MESSAGE_PREFIX);
        // Add bytes from message itself
        bytes.extend_from_slice(&self.as_bytes());
        bytes
    }
}

#[cfg(test)]
mod tests {
    use crate::abi::EthToSuiTokenBridgeV1;
    use crate::crypto::BridgeAuthorityKeyPair;
    use crate::crypto::BridgeAuthorityPublicKeyBytes;
    use crate::crypto::BridgeAuthoritySignInfo;
    use crate::events::EmittedSuiToEthTokenBridgeV1;
    use crate::types::BlocklistType;
    use crate::types::EmergencyActionType;
    use crate::types::USD_MULTIPLIER;
    use ethers::abi::ParamType;
    use ethers::types::{Address as EthAddress, TxHash};
    use fastcrypto::encoding::Encoding;
    use fastcrypto::encoding::Hex;
    use fastcrypto::hash::HashFunction;
    use fastcrypto::hash::Keccak256;
    use fastcrypto::traits::ToFromBytes;
    use prometheus::Registry;
    use std::str::FromStr;
    use sui_types::base_types::{SuiAddress, TransactionDigest};
    use sui_types::bridge::BridgeChainId;
    use sui_types::bridge::TOKEN_ID_BTC;
    use sui_types::bridge::TOKEN_ID_USDC;
    use sui_types::TypeTag;

    use super::*;

    #[test]
    fn test_bridge_message_encoding() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let nonce = 54321u64;
        let sui_tx_digest = TransactionDigest::random();
        let sui_chain_id = BridgeChainId::SuiTestnet;
        let sui_tx_event_index = 1u16;
        let eth_chain_id = BridgeChainId::EthSepolia;
        let sui_address = SuiAddress::random_for_testing_only();
        let eth_address = EthAddress::random();
        let token_id = TOKEN_ID_USDC;
        let amount_sui_adjusted = 1_000_000;

        let sui_bridge_event = EmittedSuiToEthTokenBridgeV1 {
            nonce,
            sui_chain_id,
            eth_chain_id,
            sui_address,
            eth_address,
            token_id,
            amount_sui_adjusted,
        };

        let encoded_bytes = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest,
            sui_tx_event_index,
            sui_bridge_event,
        })
        .to_bytes();

        // Construct the expected bytes
        let prefix_bytes = BRIDGE_MESSAGE_PREFIX.to_vec(); // len: 18
        let message_type = vec![BridgeActionType::TokenTransfer as u8]; // len: 1
        let message_version = vec![TOKEN_TRANSFER_MESSAGE_VERSION]; // len: 1
        let nonce_bytes = nonce.to_be_bytes().to_vec(); // len: 8
        let source_chain_id_bytes = vec![sui_chain_id as u8]; // len: 1

        let sui_address_length_bytes = vec![SUI_ADDRESS_LENGTH as u8]; // len: 1
        let sui_address_bytes = sui_address.to_vec(); // len: 32
        let dest_chain_id_bytes = vec![eth_chain_id as u8]; // len: 1
        let eth_address_length_bytes = vec![EthAddress::len_bytes() as u8]; // len: 1
        let eth_address_bytes = eth_address.as_bytes().to_vec(); // len: 20

        let token_id_bytes = vec![token_id]; // len: 1
        let token_amount_bytes = amount_sui_adjusted.to_be_bytes().to_vec(); // len: 8

        let mut combined_bytes = Vec::new();
        combined_bytes.extend_from_slice(&prefix_bytes);
        combined_bytes.extend_from_slice(&message_type);
        combined_bytes.extend_from_slice(&message_version);
        combined_bytes.extend_from_slice(&nonce_bytes);
        combined_bytes.extend_from_slice(&source_chain_id_bytes);
        combined_bytes.extend_from_slice(&sui_address_length_bytes);
        combined_bytes.extend_from_slice(&sui_address_bytes);
        combined_bytes.extend_from_slice(&dest_chain_id_bytes);
        combined_bytes.extend_from_slice(&eth_address_length_bytes);
        combined_bytes.extend_from_slice(&eth_address_bytes);
        combined_bytes.extend_from_slice(&token_id_bytes);
        combined_bytes.extend_from_slice(&token_amount_bytes);

        assert_eq!(combined_bytes, encoded_bytes);

        // Assert fixed length
        // TODO: for each action type add a test to assert the length
        assert_eq!(
            combined_bytes.len(),
            18 + 1 + 1 + 8 + 1 + 1 + 32 + 1 + 20 + 1 + 1 + 8
        );
        Ok(())
    }

    #[test]
    fn test_bridge_message_encoding_regression_emitted_sui_to_eth_token_bridge_v1(
    ) -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let sui_tx_digest = TransactionDigest::random();
        let sui_tx_event_index = 1u16;

        let nonce = 10u64;
        let sui_chain_id = BridgeChainId::SuiTestnet;
        let eth_chain_id = BridgeChainId::EthSepolia;
        let sui_address = SuiAddress::from_str(
            "0x0000000000000000000000000000000000000000000000000000000000000064",
        )
        .unwrap();
        let eth_address =
            EthAddress::from_str("0x00000000000000000000000000000000000000c8").unwrap();
        let token_id = TOKEN_ID_USDC;
        let amount_sui_adjusted = 12345;

        let sui_bridge_event = EmittedSuiToEthTokenBridgeV1 {
            nonce,
            sui_chain_id,
            eth_chain_id,
            sui_address,
            eth_address,
            token_id,
            amount_sui_adjusted,
        };
        let encoded_bytes = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest,
            sui_tx_event_index,
            sui_bridge_event,
        })
        .to_bytes();
        assert_eq!(
            encoded_bytes,
            Hex::decode("5355495f4252494447455f4d4553534147450001000000000000000a012000000000000000000000000000000000000000000000000000000000000000640b1400000000000000000000000000000000000000c8030000000000003039").unwrap(),
        );

        let hash = Keccak256::digest(encoded_bytes).digest;
        assert_eq!(
            hash.to_vec(),
            Hex::decode("6ab34c52b6264cbc12fe8c3874f9b08f8481d2e81530d136386646dbe2f8baf4")
                .unwrap(),
        );
        Ok(())
    }

    #[test]
    fn test_bridge_message_encoding_blocklist_update_v1() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let pub_key_bytes = BridgeAuthorityPublicKeyBytes::from_bytes(
            &Hex::decode("02321ede33d2c2d7a8a152f275a1484edef2098f034121a602cb7d767d38680aa4")
                .unwrap(),
        )
        .unwrap();
        let blocklist_action = BridgeAction::BlocklistCommitteeAction(BlocklistCommitteeAction {
            nonce: 129,
            chain_id: BridgeChainId::SuiCustom,
            blocklist_type: BlocklistType::Blocklist,
            members_to_update: vec![pub_key_bytes.clone()],
        });
        let bytes = blocklist_action.to_bytes();
        /*
        5355495f4252494447455f4d455353414745: prefix
        01: msg type
        01: msg version
        0000000000000081: nonce
        03: chain id
        00: blocklist type
        01: length of updated members
        [
            68b43fd906c0b8f024a18c56e06744f7c6157c65
        ]: blocklisted members abi-encoded
        */
        assert_eq!(bytes, Hex::decode("5355495f4252494447455f4d4553534147450101000000000000008102000168b43fd906c0b8f024a18c56e06744f7c6157c65").unwrap());

        let pub_key_bytes_2 = BridgeAuthorityPublicKeyBytes::from_bytes(
            &Hex::decode("027f1178ff417fc9f5b8290bd8876f0a157a505a6c52db100a8492203ddd1d4279")
                .unwrap(),
        )
        .unwrap();
        // its evem address: 0xacaef39832cb995c4e049437a3e2ec6a7bad1ab5
        let blocklist_action = BridgeAction::BlocklistCommitteeAction(BlocklistCommitteeAction {
            nonce: 68,
            chain_id: BridgeChainId::SuiCustom,
            blocklist_type: BlocklistType::Unblocklist,
            members_to_update: vec![pub_key_bytes.clone(), pub_key_bytes_2.clone()],
        });
        let bytes = blocklist_action.to_bytes();
        /*
        5355495f4252494447455f4d455353414745: prefix
        01: msg type
        01: msg version
        0000000000000044: nonce
        02: chain id
        01: blocklist type
        02: length of updated members
        [
            68b43fd906c0b8f024a18c56e06744f7c6157c65
            acaef39832cb995c4e049437a3e2ec6a7bad1ab5
        ]: blocklisted members abi-encoded
        */
        assert_eq!(bytes, Hex::decode("5355495f4252494447455f4d4553534147450101000000000000004402010268b43fd906c0b8f024a18c56e06744f7c6157c65acaef39832cb995c4e049437a3e2ec6a7bad1ab5").unwrap());

        let blocklist_action = BridgeAction::BlocklistCommitteeAction(BlocklistCommitteeAction {
            nonce: 49,
            chain_id: BridgeChainId::EthCustom,
            blocklist_type: BlocklistType::Blocklist,
            members_to_update: vec![pub_key_bytes.clone()],
        });
        let bytes = blocklist_action.to_bytes();
        /*
        5355495f4252494447455f4d455353414745: prefix
        01: msg type
        01: msg version
        0000000000000031: nonce
        0c: chain id
        00: blocklist type
        01: length of updated members
        [
            68b43fd906c0b8f024a18c56e06744f7c6157c65
        ]: blocklisted members abi-encoded
        */
        assert_eq!(bytes, Hex::decode("5355495f4252494447455f4d455353414745010100000000000000310c000168b43fd906c0b8f024a18c56e06744f7c6157c65").unwrap());

        let blocklist_action = BridgeAction::BlocklistCommitteeAction(BlocklistCommitteeAction {
            nonce: 94,
            chain_id: BridgeChainId::EthSepolia,
            blocklist_type: BlocklistType::Unblocklist,
            members_to_update: vec![pub_key_bytes.clone(), pub_key_bytes_2.clone()],
        });
        let bytes = blocklist_action.to_bytes();
        /*
        5355495f4252494447455f4d455353414745: prefix
        01: msg type
        01: msg version
        000000000000005e: nonce
        0b: chain id
        01: blocklist type
        02: length of updated members
        [
            00000000000000000000000068b43fd906c0b8f024a18c56e06744f7c6157c65
            000000000000000000000000acaef39832cb995c4e049437a3e2ec6a7bad1ab5
        ]: blocklisted members abi-encoded
        */
        assert_eq!(bytes, Hex::decode("5355495f4252494447455f4d4553534147450101000000000000005e0b010268b43fd906c0b8f024a18c56e06744f7c6157c65acaef39832cb995c4e049437a3e2ec6a7bad1ab5").unwrap());
    }

    #[test]
    fn test_bridge_message_encoding_emergency_action() {
        let action = BridgeAction::EmergencyAction(EmergencyAction {
            nonce: 55,
            chain_id: BridgeChainId::SuiCustom,
            action_type: EmergencyActionType::Pause,
        });
        let bytes = action.to_bytes();
        /*
        5355495f4252494447455f4d455353414745: prefix
        02: msg type
        01: msg version
        0000000000000037: nonce
        03: chain id
        00: action type
        */
        assert_eq!(
            bytes,
            Hex::decode("5355495f4252494447455f4d455353414745020100000000000000370200").unwrap()
        );

        let action = BridgeAction::EmergencyAction(EmergencyAction {
            nonce: 56,
            chain_id: BridgeChainId::EthSepolia,
            action_type: EmergencyActionType::Unpause,
        });
        let bytes = action.to_bytes();
        /*
        5355495f4252494447455f4d455353414745: prefix
        02: msg type
        01: msg version
        0000000000000038: nonce
        0b: chain id
        01: action type
        */
        assert_eq!(
            bytes,
            Hex::decode("5355495f4252494447455f4d455353414745020100000000000000380b01").unwrap()
        );
    }

    #[test]
    fn test_bridge_message_encoding_limit_update_action() {
        let action = BridgeAction::LimitUpdateAction(LimitUpdateAction {
            nonce: 15,
            chain_id: BridgeChainId::SuiCustom,
            sending_chain_id: BridgeChainId::EthCustom,
            new_usd_limit: 1_000_000 * USD_MULTIPLIER, // $1M USD
        });
        let bytes = action.to_bytes();
        /*
        5355495f4252494447455f4d455353414745: prefix
        03: msg type
        01: msg version
        000000000000000f: nonce
        03: chain id
        0c: sending chain id
        00000002540be400: new usd limit
        */
        assert_eq!(
            bytes,
            Hex::decode(
                "5355495f4252494447455f4d4553534147450301000000000000000f020c00000002540be400"
            )
            .unwrap()
        );
    }

    #[test]
    fn test_bridge_message_encoding_asset_price_update_action() {
        let action = BridgeAction::AssetPriceUpdateAction(AssetPriceUpdateAction {
            nonce: 266,
            chain_id: BridgeChainId::SuiCustom,
            token_id: TOKEN_ID_BTC,
            new_usd_price: 100_000 * USD_MULTIPLIER, // $100k USD
        });
        let bytes = action.to_bytes();
        /*
        5355495f4252494447455f4d455353414745: prefix
        04: msg type
        01: msg version
        000000000000010a: nonce
        03: chain id
        01: token id
        000000003b9aca00: new usd price
        */
        assert_eq!(
            bytes,
            Hex::decode(
                "5355495f4252494447455f4d4553534147450401000000000000010a0201000000003b9aca00"
            )
            .unwrap()
        );
    }

    #[test]
    fn test_bridge_message_encoding_evm_contract_upgrade_action() {
        // Calldata with only the function selector and no parameters: `function initializeV2()`
        let function_signature = "initializeV2()";
        let selector = &Keccak256::digest(function_signature).digest[0..4];
        let call_data = selector.to_vec();
        assert_eq!(Hex::encode(call_data.clone()), "5cd8a76b");

        let action = BridgeAction::EvmContractUpgradeAction(EvmContractUpgradeAction {
            nonce: 123,
            chain_id: BridgeChainId::EthCustom,
            proxy_address: EthAddress::repeat_byte(6),
            new_impl_address: EthAddress::repeat_byte(9),
            call_data,
        });
        /*
        5355495f4252494447455f4d455353414745: prefix
        05: msg type
        01: msg version
        000000000000007b: nonce
        0c: chain id
        0000000000000000000000000606060606060606060606060606060606060606: proxy address
        0000000000000000000000000909090909090909090909090909090909090909: new impl address

        0000000000000000000000000000000000000000000000000000000000000060
        0000000000000000000000000000000000000000000000000000000000000004
        5cd8a76b00000000000000000000000000000000000000000000000000000000: call data
        */
        assert_eq!(Hex::encode(action.to_bytes().clone()), "5355495f4252494447455f4d4553534147450501000000000000007b0c00000000000000000000000006060606060606060606060606060606060606060000000000000000000000000909090909090909090909090909090909090909000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000045cd8a76b00000000000000000000000000000000000000000000000000000000");

        // Calldata with one parameter: `function newMockFunction(bool)`
        let function_signature = "newMockFunction(bool)";
        let selector = &Keccak256::digest(function_signature).digest[0..4];
        let mut call_data = selector.to_vec();
        call_data.extend(ethers::abi::encode(&[ethers::abi::Token::Bool(true)]));
        assert_eq!(
            Hex::encode(call_data.clone()),
            "417795ef0000000000000000000000000000000000000000000000000000000000000001"
        );
        let action = BridgeAction::EvmContractUpgradeAction(EvmContractUpgradeAction {
            nonce: 123,
            chain_id: BridgeChainId::EthCustom,
            proxy_address: EthAddress::repeat_byte(6),
            new_impl_address: EthAddress::repeat_byte(9),
            call_data,
        });
        /*
        5355495f4252494447455f4d455353414745: prefix
        05: msg type
        01: msg version
        000000000000007b: nonce
        0c: chain id
        0000000000000000000000000606060606060606060606060606060606060606: proxy address
        0000000000000000000000000909090909090909090909090909090909090909: new impl address

        0000000000000000000000000000000000000000000000000000000000000060
        0000000000000000000000000000000000000000000000000000000000000024
        417795ef00000000000000000000000000000000000000000000000000000000
        0000000100000000000000000000000000000000000000000000000000000000: call data
        */
        assert_eq!(Hex::encode(action.to_bytes().clone()), "5355495f4252494447455f4d4553534147450501000000000000007b0c0000000000000000000000000606060606060606060606060606060606060606000000000000000000000000090909090909090909090909090909090909090900000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000024417795ef000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000");

        // Calldata with two parameters: `function newerMockFunction(bool, uint8)`
        let function_signature = "newMockFunction(bool,uint8)";
        let selector = &Keccak256::digest(function_signature).digest[0..4];
        let mut call_data = selector.to_vec();
        call_data.extend(ethers::abi::encode(&[
            ethers::abi::Token::Bool(true),
            ethers::abi::Token::Uint(42u8.into()),
        ]));
        assert_eq!(
            Hex::encode(call_data.clone()),
            "be8fc25d0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002a"
        );
        let action = BridgeAction::EvmContractUpgradeAction(EvmContractUpgradeAction {
            nonce: 123,
            chain_id: BridgeChainId::EthCustom,
            proxy_address: EthAddress::repeat_byte(6),
            new_impl_address: EthAddress::repeat_byte(9),
            call_data,
        });
        /*
        5355495f4252494447455f4d455353414745: prefix
        05: msg type
        01: msg version
        000000000000007b: nonce
        0c: chain id
        0000000000000000000000000606060606060606060606060606060606060606: proxy address
        0000000000000000000000000909090909090909090909090909090909090909: new impl address

        0000000000000000000000000000000000000000000000000000000000000060
        0000000000000000000000000000000000000000000000000000000000000044
        be8fc25d00000000000000000000000000000000000000000000000000000000
        0000000100000000000000000000000000000000000000000000000000000000
        0000002a00000000000000000000000000000000000000000000000000000000: call data
        */
        assert_eq!(Hex::encode(action.to_bytes().clone()), "5355495f4252494447455f4d4553534147450501000000000000007b0c0000000000000000000000000606060606060606060606060606060606060606000000000000000000000000090909090909090909090909090909090909090900000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000044be8fc25d0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002a00000000000000000000000000000000000000000000000000000000");

        // Empty calldate
        let action = BridgeAction::EvmContractUpgradeAction(EvmContractUpgradeAction {
            nonce: 123,
            chain_id: BridgeChainId::EthCustom,
            proxy_address: EthAddress::repeat_byte(6),
            new_impl_address: EthAddress::repeat_byte(9),
            call_data: vec![],
        });
        /*
        5355495f4252494447455f4d455353414745: prefix
        05: msg type
        01: msg version
        000000000000007b: nonce
        0c: chain id
        0000000000000000000000000606060606060606060606060606060606060606: proxy address
        0000000000000000000000000909090909090909090909090909090909090909: new impl address

        0000000000000000000000000000000000000000000000000000000000000060
        0000000000000000000000000000000000000000000000000000000000000000: call data
        */
        let data = action.to_bytes();
        assert_eq!(Hex::encode(data.clone()), "5355495f4252494447455f4d4553534147450501000000000000007b0c0000000000000000000000000606060606060606060606060606060606060606000000000000000000000000090909090909090909090909090909090909090900000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000000");
        let types = vec![ParamType::Address, ParamType::Address, ParamType::Bytes];
        // Ensure that the call data (start from bytes 29) can be decoded
        ethers::abi::decode(&types, &data[29..]).unwrap();
    }

    #[test]
    fn test_bridge_message_encoding_regression_eth_to_sui_token_bridge_v1() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let eth_tx_hash = TxHash::random();
        let eth_event_index = 1u16;

        let nonce = 10u64;
        let sui_chain_id = BridgeChainId::SuiTestnet;
        let eth_chain_id = BridgeChainId::EthSepolia;
        let sui_address = SuiAddress::from_str(
            "0x0000000000000000000000000000000000000000000000000000000000000064",
        )
        .unwrap();
        let eth_address =
            EthAddress::from_str("0x00000000000000000000000000000000000000c8").unwrap();
        let token_id = TOKEN_ID_USDC;
        let sui_adjusted_amount = 12345;

        let eth_bridge_event = EthToSuiTokenBridgeV1 {
            nonce,
            sui_chain_id,
            eth_chain_id,
            sui_address,
            eth_address,
            token_id,
            sui_adjusted_amount,
        };
        let encoded_bytes = BridgeAction::EthToSuiBridgeAction(EthToSuiBridgeAction {
            eth_tx_hash,
            eth_event_index,
            eth_bridge_event,
        })
        .to_bytes();

        assert_eq!(
            encoded_bytes,
            Hex::decode("5355495f4252494447455f4d4553534147450001000000000000000a0b1400000000000000000000000000000000000000c801200000000000000000000000000000000000000000000000000000000000000064030000000000003039").unwrap(),
        );

        let hash = Keccak256::digest(encoded_bytes).digest;
        assert_eq!(
            hash.to_vec(),
            Hex::decode("b352508c301a37bb1b68a75dd0fc42b6f692b2650818631c8f8a4d4d3e5bef46")
                .unwrap(),
        );
        Ok(())
    }

    #[test]
    fn test_bridge_message_encoding_regression_add_coins_on_sui() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();

        let action = BridgeAction::AddTokensOnSuiAction(AddTokensOnSuiAction {
            nonce: 0,
            chain_id: BridgeChainId::SuiCustom,
            native: false,
            token_ids: vec![1, 2, 3, 4],
            token_type_names: vec![
                TypeTag::from_str("0x9b5e13bcd0cb23ff25c07698e89d48056c745338d8c9dbd033a4172b87027073::btc::BTC").unwrap(),
                TypeTag::from_str("0x7970d71c03573f540a7157f0d3970e117effa6ae16cefd50b45c749670b24e6a::eth::ETH").unwrap(),
                TypeTag::from_str("0x500e429a24478405d5130222b20f8570a746b6bc22423f14b4d4e6a8ea580736::usdc::USDC").unwrap(),
                TypeTag::from_str("0x46bfe51da1bd9511919a92eb1154149b36c0f4212121808e13e3e5857d607a9c::usdt::USDT").unwrap(),
            ],
            token_prices: vec![
                500_000_000u64,
                30_000_000u64,
                1_000u64,
                1_000u64,
            ]
        });
        let encoded_bytes = action.to_bytes();

        assert_eq!(
            Hex::encode(encoded_bytes),
            "5355495f4252494447455f4d4553534147450601000000000000000002000401020304044a396235653133626364306362323366663235633037363938653839643438303536633734353333386438633964626430333361343137326238373032373037333a3a6274633a3a4254434a373937306437316330333537336635343061373135376630643339373065313137656666613661653136636566643530623435633734393637306232346536613a3a6574683a3a4554484c353030653432396132343437383430356435313330323232623230663835373061373436623662633232343233663134623464346536613865613538303733363a3a757364633a3a555344434c343662666535316461316264393531313931396139326562313135343134396233366330663432313231323138303865313365336535383537643630376139633a3a757364743a3a55534454040065cd1d0000000080c3c90100000000e803000000000000e803000000000000",
        );
        Ok(())
    }

    #[test]
    fn test_bridge_message_encoding_regression_add_coins_on_evm() -> anyhow::Result<()> {
        let action = BridgeAction::AddTokensOnEvmAction(crate::types::AddTokensOnEvmAction {
            nonce: 0,
            chain_id: BridgeChainId::EthCustom,
            native: true,
            token_ids: vec![99, 100, 101],
            token_addresses: vec![
                EthAddress::from_str("0x6B175474E89094C44Da98b954EedeAC495271d0F").unwrap(),
                EthAddress::from_str("0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84").unwrap(),
                EthAddress::from_str("0xC18360217D8F7Ab5e7c516566761Ea12Ce7F9D72").unwrap(),
            ],
            token_sui_decimals: vec![5, 6, 7],
            token_prices: vec![1_000_000_000, 2_000_000_000, 3_000_000_000],
        });
        let encoded_bytes = action.to_bytes();

        assert_eq!(
            Hex::encode(encoded_bytes),
            "5355495f4252494447455f4d455353414745070100000000000000000c0103636465036b175474e89094c44da98b954eedeac495271d0fae7ab96520de3a18e5e111b5eaab095312d7fe84c18360217d8f7ab5e7c516566761ea12ce7f9d720305060703000000003b9aca00000000007735940000000000b2d05e00",
        );
        // To generate regression test for sol contracts
        let keys = get_bridge_encoding_regression_test_keys();
        for key in keys {
            let pub_key = key.public.as_bytes();
            println!("pub_key: {:?}", Hex::encode(pub_key));
            println!(
                "sig: {:?}",
                Hex::encode(
                    BridgeAuthoritySignInfo::new(&action, &key)
                        .signature
                        .as_bytes()
                )
            );
        }
        Ok(())
    }

    fn get_bridge_encoding_regression_test_keys() -> Vec<BridgeAuthorityKeyPair> {
        vec![
            BridgeAuthorityKeyPair::from_bytes(
                &Hex::decode("e42c82337ce12d4a7ad6cd65876d91b2ab6594fd50cdab1737c91773ba7451db")
                    .unwrap(),
            )
            .unwrap(),
            BridgeAuthorityKeyPair::from_bytes(
                &Hex::decode("1aacd610da3d0cc691a04b83b01c34c6c65cda0fe8d502df25ff4b3185c85687")
                    .unwrap(),
            )
            .unwrap(),
            BridgeAuthorityKeyPair::from_bytes(
                &Hex::decode("53e7baf8378fbc62692e3056c2e10c6666ef8b5b3a53914830f47636d1678140")
                    .unwrap(),
            )
            .unwrap(),
            BridgeAuthorityKeyPair::from_bytes(
                &Hex::decode("08b5350a091faabd5f25b6e290bfc3f505d43208775b9110dfed5ee6c7a653f0")
                    .unwrap(),
            )
            .unwrap(),
        ]
    }
}
