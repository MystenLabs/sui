// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This file contains the definition of the SuiBridgeEvent enum, of
//! which each variant is an emitted Event struct defind in the Move
//! Bridge module. We rely on structures in this file to decode
//! the bcs content of the emitted events.

use std::str::FromStr;

use crate::error::BridgeError;
use crate::error::BridgeResult;
use crate::types::BridgeAction;
use crate::types::BridgeChainId;
use crate::types::SuiToEthBridgeAction;
use crate::types::TokenId;
use ethers::types::Address as EthAddress;
use move_core_types::language_storage::StructTag;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use sui_json_rpc_types::SuiEvent;
use sui_types::base_types::SuiAddress;
use sui_types::digests::TransactionDigest;

// This is the event structure defined and emitted in Move
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct EmittedSuiToEthTokenBridgeV1 {
    pub nonce: u64,
    pub sui_chain_id: BridgeChainId,
    pub eth_chain_id: BridgeChainId,
    pub sui_address: SuiAddress,
    pub eth_address: EthAddress,
    pub token_id: TokenId,
    pub amount: u64,
}

const EMITTED_SUI_TO_ETH_TOKEN_BRIDGE_V1_STUCT_TAG: &str =
    "0x01::SuiToEthTokenBridge::SuiToEthTokenBridge";

crate::declare_events!(
    // TODO: Placeholder, use right struct tag
    SuiToEthTokenBridgeV1(EmittedSuiToEthTokenBridgeV1) => EMITTED_SUI_TO_ETH_TOKEN_BRIDGE_V1_STUCT_TAG,
    // Add new event types here. Format: EnumVariantName(Struct) => "StructTagString",
);

#[macro_export]
macro_rules! declare_events {
    ($($variant:ident($type:path) => $tag:expr),* $(,)?) => {

        #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
        pub enum SuiBridgeEvent {
            $($variant($type),)*
        }

        #[allow(non_upper_case_globals)]
        $(pub(crate) static $variant: OnceCell<StructTag> = OnceCell::new();)*

        pub(crate) fn init_all_struct_tags() {
            $($variant.get_or_init(|| {
                StructTag::from_str($tag).unwrap()
            });)*
        }

        // Try to convert a SuiEvent into SuiBridgeEvent
        impl SuiBridgeEvent {
            pub fn try_from_sui_event(event: &SuiEvent) -> BridgeResult<Option<SuiBridgeEvent>> {
                init_all_struct_tags(); // Ensure all tags are initialized

                // Unwrap safe: we inited above
                $(
                    if &event.type_ == $variant.get().unwrap() {
                        return Ok(Some(SuiBridgeEvent::$variant(bcs::from_bytes(&event.bcs).map_err(|e| BridgeError::InternalError(format!("Failed to deserialize event to SuiBridgeEvent: {:?}", e)))
                        ?)));
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
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::{EmittedSuiToEthTokenBridgeV1, EMITTED_SUI_TO_ETH_TOKEN_BRIDGE_V1_STUCT_TAG};
    use crate::types::BridgeAction;
    use crate::types::BridgeChainId;
    use crate::types::SuiToEthBridgeAction;
    use crate::types::TokenId;
    use ethers::types::Address as EthAddress;
    use move_core_types::language_storage::StructTag;
    use std::str::FromStr;
    use sui_json_rpc_types::SuiEvent;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;
    use sui_types::digests::TransactionDigest;
    use sui_types::event::EventID;
    use sui_types::Identifier;

    /// Returns a test SuiEvent and corresponding BridgeAction
    pub fn get_test_sui_event_and_action(identifier: Identifier) -> (SuiEvent, BridgeAction) {
        let emitted_event = EmittedSuiToEthTokenBridgeV1 {
            nonce: 1,
            sui_chain_id: BridgeChainId::SuiTestnet,
            eth_chain_id: BridgeChainId::EthSepolia,
            sui_address: SuiAddress::random_for_testing_only(),
            eth_address: EthAddress::random(),
            token_id: TokenId::Sui,
            amount: 100,
        };
        let tx_digest = TransactionDigest::random();
        let event_idx = 10u16;
        let bridge_action = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest: tx_digest,
            sui_tx_event_index: event_idx,
            sui_bridge_event: emitted_event.clone(),
        });
        let event = SuiEvent {
            // For this test to pass, match what is in events.rs
            type_: StructTag::from_str(EMITTED_SUI_TO_ETH_TOKEN_BRIDGE_V1_STUCT_TAG).unwrap(),
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
}
