// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This file contains the definition of the SuiBridgeEvent enum, of
//! which each variant is an emitted Event struct defind in the Move
//! Bridge module. We rely on structures in this file to decode
//! the bcs content of the emitted events.

#![allow(non_upper_case_globals)]

use crate::crypto::BridgeAuthorityPublicKey;
use crate::error::BridgeError;
use crate::error::BridgeResult;
use crate::types::BridgeAction;
use crate::types::SuiToEthBridgeAction;
use ethers::types::Address as EthAddress;
use fastcrypto::encoding::Encoding;
use fastcrypto::encoding::Hex;
use move_core_types::language_storage::StructTag;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use sui_json_rpc_types::SuiEvent;
use sui_types::base_types::SuiAddress;
use sui_types::bridge::BridgeChainId;
use sui_types::bridge::MoveTypeBridgeMessageKey;
use sui_types::bridge::MoveTypeCommitteeMember;
use sui_types::bridge::MoveTypeCommitteeMemberRegistration;
use sui_types::collection_types::VecMap;
use sui_types::crypto::ToFromBytes;
use sui_types::digests::TransactionDigest;
use sui_types::BRIDGE_PACKAGE_ID;

// `TokendDepositedEvent` emitted in bridge.move
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveTokenDepositedEvent {
    pub seq_num: u64,
    pub source_chain: u8,
    pub sender_address: Vec<u8>,
    pub target_chain: u8,
    pub target_address: Vec<u8>,
    pub token_type: u8,
    pub amount_sui_adjusted: u64,
}

// `TokenTransferApproved` emitted in bridge.move
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveTokenTransferApproved {
    pub message_key: MoveTypeBridgeMessageKey,
}

// `TokenTransferClaimed` emitted in bridge.move
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveTokenTransferClaimed {
    pub message_key: MoveTypeBridgeMessageKey,
}

// `TokenTransferAlreadyApproved` emitted in bridge.move
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveTokenTransferAlreadyApproved {
    pub message_key: MoveTypeBridgeMessageKey,
}

// `TokenTransferAlreadyClaimed` emitted in bridge.move
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct MoveTokenTransferAlreadyClaimed {
    pub message_key: MoveTypeBridgeMessageKey,
}

// `CommitteeUpdateEvent` emitted in committee.move
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MoveCommitteeUpdateEvent {
    pub members: VecMap<Vec<u8>, MoveTypeCommitteeMember>,
    pub stake_participation_percentage: u64,
}

// `BlocklistValidatorEvent` emitted in committee.move
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MoveBlocklistValidatorEvent {
    pub blocklisted: bool,
    pub public_keys: Vec<Vec<u8>>,
}

// Sanitized version of MoveTokenDepositedEvent
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Hash)]
pub struct EmittedSuiToEthTokenBridgeV1 {
    pub nonce: u64,
    pub sui_chain_id: BridgeChainId,
    pub eth_chain_id: BridgeChainId,
    pub sui_address: SuiAddress,
    pub eth_address: EthAddress,
    pub token_id: u8,
    // The amount of tokens deposited with decimal points on Sui side
    pub amount_sui_adjusted: u64,
}

// Sanitized version of MoveTokenTransferApproved
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Hash)]
pub struct TokenTransferApproved {
    pub nonce: u64,
    pub source_chain: BridgeChainId,
}

impl TryFrom<MoveTokenTransferApproved> for TokenTransferApproved {
    type Error = BridgeError;

    fn try_from(event: MoveTokenTransferApproved) -> BridgeResult<Self> {
        let source_chain = BridgeChainId::try_from(event.message_key.source_chain).map_err(|_e| {
            BridgeError::Generic(format!(
                "Failed to convert MoveTokenTransferApproved to TokenTransferApproved. Failed to convert source chain {} to BridgeChainId",
                event.message_key.source_chain,
            ))
        })?;
        Ok(Self {
            nonce: event.message_key.bridge_seq_num,
            source_chain,
        })
    }
}

// Sanitized version of MoveTokenTransferClaimed
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Hash)]
pub struct TokenTransferClaimed {
    pub nonce: u64,
    pub source_chain: BridgeChainId,
}

impl TryFrom<MoveTokenTransferClaimed> for TokenTransferClaimed {
    type Error = BridgeError;

    fn try_from(event: MoveTokenTransferClaimed) -> BridgeResult<Self> {
        let source_chain = BridgeChainId::try_from(event.message_key.source_chain).map_err(|_e| {
            BridgeError::Generic(format!(
                "Failed to convert MoveTokenTransferClaimed to TokenTransferClaimed. Failed to convert source chain {} to BridgeChainId",
                event.message_key.source_chain,
            ))
        })?;
        Ok(Self {
            nonce: event.message_key.bridge_seq_num,
            source_chain,
        })
    }
}

// Sanitized version of MoveTokenTransferAlreadyApproved
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Hash)]
pub struct TokenTransferAlreadyApproved {
    pub nonce: u64,
    pub source_chain: BridgeChainId,
}

impl TryFrom<MoveTokenTransferAlreadyApproved> for TokenTransferAlreadyApproved {
    type Error = BridgeError;

    fn try_from(event: MoveTokenTransferAlreadyApproved) -> BridgeResult<Self> {
        let source_chain = BridgeChainId::try_from(event.message_key.source_chain).map_err(|_e| {
            BridgeError::Generic(format!(
                "Failed to convert MoveTokenTransferAlreadyApproved to TokenTransferAlreadyApproved. Failed to convert source chain {} to BridgeChainId",
                event.message_key.source_chain,
            ))
        })?;
        Ok(Self {
            nonce: event.message_key.bridge_seq_num,
            source_chain,
        })
    }
}

// Sanitized version of MoveTokenTransferAlreadyClaimed
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Hash)]
pub struct TokenTransferAlreadyClaimed {
    pub nonce: u64,
    pub source_chain: BridgeChainId,
}

impl TryFrom<MoveTokenTransferAlreadyClaimed> for TokenTransferAlreadyClaimed {
    type Error = BridgeError;

    fn try_from(event: MoveTokenTransferAlreadyClaimed) -> BridgeResult<Self> {
        let source_chain = BridgeChainId::try_from(event.message_key.source_chain).map_err(|_e| {
            BridgeError::Generic(format!(
                "Failed to convert MoveTokenTransferAlreadyClaimed to TokenTransferAlreadyClaimed. Failed to convert source chain {} to BridgeChainId",
                event.message_key.source_chain,
            ))
        })?;
        Ok(Self {
            nonce: event.message_key.bridge_seq_num,
            source_chain,
        })
    }
}

// Sanitized version of MoveCommitteeUpdateEvent
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct CommitteeUpdate {
    pub members: Vec<MoveTypeCommitteeMember>,
    pub stake_participation_percentage: u64,
}

impl TryFrom<MoveCommitteeUpdateEvent> for CommitteeUpdate {
    type Error = BridgeError;

    fn try_from(event: MoveCommitteeUpdateEvent) -> BridgeResult<Self> {
        let members = event
            .members
            .contents
            .into_iter()
            .map(|v| v.value)
            .collect();
        Ok(Self {
            members,
            stake_participation_percentage: event.stake_participation_percentage,
        })
    }
}

// Sanitized version of MoveBlocklistValidatorEvent
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct BlocklistValidatorEvent {
    pub blocklisted: bool,
    pub public_keys: Vec<BridgeAuthorityPublicKey>,
}

impl TryFrom<MoveBlocklistValidatorEvent> for BlocklistValidatorEvent {
    type Error = BridgeError;

    fn try_from(event: MoveBlocklistValidatorEvent) -> BridgeResult<Self> {
        let public_keys = event.public_keys.into_iter().map(|bytes|
            BridgeAuthorityPublicKey::from_bytes(&bytes).map_err(|e|
                BridgeError::Generic(format!("Failed to convert MoveBlocklistValidatorEvent to BlocklistValidatorEvent. Failed to convert public key to BridgeAuthorityPublicKey: {:?}", e))
            )
        ).collect::<BridgeResult<Vec<_>>>()?;
        Ok(Self {
            blocklisted: event.blocklisted,
            public_keys,
        })
    }
}

impl TryFrom<MoveTokenDepositedEvent> for EmittedSuiToEthTokenBridgeV1 {
    type Error = BridgeError;

    fn try_from(event: MoveTokenDepositedEvent) -> BridgeResult<Self> {
        let token_id = event.token_type;
        let sui_chain_id = BridgeChainId::try_from(event.source_chain).map_err(|_e| {
            BridgeError::Generic(format!(
                "Failed to convert MoveTokenDepositedEvent to EmittedSuiToEthTokenBridgeV1. Failed to convert source chain {} to BridgeChainId",
                event.token_type,
            ))
        })?;
        let eth_chain_id = BridgeChainId::try_from(event.target_chain).map_err(|_e| {
            BridgeError::Generic(format!(
                "Failed to convert MoveTokenDepositedEvent to EmittedSuiToEthTokenBridgeV1. Failed to convert target chain {} to BridgeChainId",
                event.token_type,
            ))
        })?;

        match sui_chain_id {
            BridgeChainId::SuiMainnet | BridgeChainId::SuiTestnet | BridgeChainId::SuiCustom => {}
            _ => {
                return Err(BridgeError::Generic(format!(
                    "Failed to convert MoveTokenDepositedEvent to EmittedSuiToEthTokenBridgeV1. Invalid source chain {}",
                    event.source_chain
                )));
            }
        }
        match eth_chain_id {
            BridgeChainId::EthMainnet | BridgeChainId::EthSepolia | BridgeChainId::EthCustom => {}
            _ => {
                return Err(BridgeError::Generic(format!(
                    "Failed to convert MoveTokenDepositedEvent to EmittedSuiToEthTokenBridgeV1. Invalid target chain {}",
                    event.target_chain
                )));
            }
        }

        let sui_address = SuiAddress::from_bytes(event.sender_address)
            .map_err(|e| BridgeError::Generic(format!("Failed to convert MoveTokenDepositedEvent to EmittedSuiToEthTokenBridgeV1. Failed to convert sender_address to SuiAddress: {:?}", e)))?;
        let eth_address = EthAddress::from_str(&Hex::encode(&event.target_address))?;

        Ok(Self {
            nonce: event.seq_num,
            sui_chain_id,
            eth_chain_id,
            sui_address,
            eth_address,
            token_id,
            amount_sui_adjusted: event.amount_sui_adjusted,
        })
    }
}

crate::declare_events!(
    SuiToEthTokenBridgeV1(EmittedSuiToEthTokenBridgeV1) => ("bridge::TokenDepositedEvent", MoveTokenDepositedEvent),
    TokenTransferApproved(TokenTransferApproved) => ("bridge::TokenTransferApproved", MoveTokenTransferApproved),
    TokenTransferClaimed(TokenTransferClaimed) => ("bridge::TokenTransferClaimed", MoveTokenTransferClaimed),
    TokenTransferAlreadyApproved(TokenTransferAlreadyApproved) => ("bridge::TokenTransferAlreadyApproved", MoveTokenTransferAlreadyApproved),
    TokenTransferAlreadyClaimed(TokenTransferAlreadyClaimed) => ("bridge::TokenTransferAlreadyClaimed", MoveTokenTransferAlreadyClaimed),
    // No need to define a sanitized event struct for MoveTypeCommitteeMemberRegistration
    // because the info provided by validators could be invalid
    CommitteeMemberRegistration(MoveTypeCommitteeMemberRegistration) => ("committee::CommitteeMemberRegistration", MoveTypeCommitteeMemberRegistration),
    CommitteeUpdateEvent(CommitteeUpdate) => ("committee::CommitteeUpdateEvent", MoveCommitteeUpdateEvent),
    BlocklistValidator(BlocklistValidatorEvent) => ("committee::CommitteeUpdateEvent", MoveBlocklistValidatorEvent),

    // Add new event types here. Format:
    // EnumVariantName(Struct) => ("{module}::{event_struct}", CorrespondingMoveStruct)
);

#[macro_export]
macro_rules! declare_events {
    ($($variant:ident($type:path) => ($event_tag:expr, $event_struct:path)),* $(,)?) => {

        #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
        pub enum SuiBridgeEvent {
            $($variant($type),)*
        }

        $(pub static $variant: OnceCell<StructTag> = OnceCell::new();)*

        pub(crate) fn init_all_struct_tags() {
            $($variant.get_or_init(|| {
                StructTag::from_str(&format!("0x{}::{}", BRIDGE_PACKAGE_ID.to_hex(), $event_tag)).unwrap()
            });)*
        }

        // Try to convert a SuiEvent into SuiBridgeEvent
        impl SuiBridgeEvent {
            pub fn try_from_sui_event(event: &SuiEvent) -> BridgeResult<Option<SuiBridgeEvent>> {
                init_all_struct_tags(); // Ensure all tags are initialized

                // Unwrap safe: we inited above
                $(
                    if &event.type_ == $variant.get().unwrap() {
                        let event_struct: $event_struct = bcs::from_bytes(&event.bcs).map_err(|e| BridgeError::InternalError(format!("Failed to deserialize event to {}: {:?}", stringify!($event_struct), e)))?;
                        return Ok(Some(SuiBridgeEvent::$variant(event_struct.try_into()?)));
                    }
                )*
                Ok(None)
            }
        }
    };
}

impl SuiBridgeEvent {
    pub fn try_into_bridge_action(
        self,
        sui_tx_digest: TransactionDigest,
        sui_tx_event_index: u16,
    ) -> Option<BridgeAction> {
        match self {
            SuiBridgeEvent::SuiToEthTokenBridgeV1(event) => {
                Some(BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
                    sui_tx_digest,
                    sui_tx_event_index,
                    sui_bridge_event: event.clone(),
                }))
            }
            SuiBridgeEvent::TokenTransferApproved(_event) => None,
            SuiBridgeEvent::TokenTransferClaimed(_event) => None,
            SuiBridgeEvent::TokenTransferAlreadyApproved(_event) => None,
            SuiBridgeEvent::TokenTransferAlreadyClaimed(_event) => None,
            SuiBridgeEvent::CommitteeMemberRegistration(_event) => None,
            SuiBridgeEvent::CommitteeUpdateEvent(_event) => None,
            SuiBridgeEvent::BlocklistValidator(_event) => None,
        }
    }
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::e2e_tests::test_utils::BridgeTestClusterBuilder;
    use crate::types::BridgeAction;
    use crate::types::SuiToEthBridgeAction;
    use ethers::types::Address as EthAddress;
    use sui_json_rpc_types::SuiEvent;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;
    use sui_types::bridge::BridgeChainId;
    use sui_types::bridge::TOKEN_ID_SUI;
    use sui_types::digests::TransactionDigest;
    use sui_types::event::EventID;
    use sui_types::Identifier;

    /// Returns a test SuiEvent and corresponding BridgeAction
    pub fn get_test_sui_event_and_action(identifier: Identifier) -> (SuiEvent, BridgeAction) {
        init_all_struct_tags(); // Ensure all tags are initialized
        let sanitized_event = EmittedSuiToEthTokenBridgeV1 {
            nonce: 1,
            sui_chain_id: BridgeChainId::SuiTestnet,
            sui_address: SuiAddress::random_for_testing_only(),
            eth_chain_id: BridgeChainId::EthSepolia,
            eth_address: EthAddress::random(),
            token_id: TOKEN_ID_SUI,
            amount_sui_adjusted: 100,
        };
        let emitted_event = MoveTokenDepositedEvent {
            seq_num: sanitized_event.nonce,
            source_chain: sanitized_event.sui_chain_id as u8,
            sender_address: sanitized_event.sui_address.to_vec(),
            target_chain: sanitized_event.eth_chain_id as u8,
            target_address: sanitized_event.eth_address.as_bytes().to_vec(),
            token_type: sanitized_event.token_id,
            amount_sui_adjusted: sanitized_event.amount_sui_adjusted,
        };

        let tx_digest = TransactionDigest::random();
        let event_idx = 10u16;
        let bridge_action = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest: tx_digest,
            sui_tx_event_index: event_idx,
            sui_bridge_event: sanitized_event.clone(),
        });
        let event = SuiEvent {
            type_: SuiToEthTokenBridgeV1.get().unwrap().clone(),
            bcs: bcs::to_bytes(&emitted_event).unwrap(),
            id: EventID {
                tx_digest,
                event_seq: event_idx as u64,
            },

            // The following fields do not matter as of writing,
            // but if tests start to fail, it's worth checking these fields.
            package_id: ObjectID::ZERO,
            transaction_module: identifier.clone(),
            sender: SuiAddress::random_for_testing_only(),
            parsed_json: serde_json::json!({"test": "test"}),
            timestamp_ms: None,
        };
        (event, bridge_action)
    }

    #[tokio::test]
    async fn test_bridge_events_conversion() {
        telemetry_subscribers::init_for_testing();
        init_all_struct_tags();
        let mut bridge_test_cluster = BridgeTestClusterBuilder::new()
            .with_eth_env(true)
            .with_bridge_cluster(false)
            .build()
            .await;

        let events = bridge_test_cluster
            .new_bridge_events(
                HashSet::from_iter([
                    CommitteeMemberRegistration.get().unwrap().clone(),
                    CommitteeUpdateEvent.get().unwrap().clone(),
                ]),
                false,
            )
            .await;
        for event in events.iter() {
            match SuiBridgeEvent::try_from_sui_event(event).unwrap().unwrap() {
                SuiBridgeEvent::CommitteeMemberRegistration(_event) => (),
                SuiBridgeEvent::CommitteeUpdateEvent(_event) => (),
                _ => panic!(
                    "Expected CommitteeMemberRegistration or CommitteeUpdateEvent, got {:?}",
                    event
                ),
            }
        }

        // TODO: trigger other events and make sure they are converted correctly
    }
}
