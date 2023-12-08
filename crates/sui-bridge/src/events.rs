// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This file contains the definition of the SuiBridgeEvent enum, of
//! which each variant is an emitted Event struct defind in the Move
//! Bridge module. We rely on structures in this file to decode
//! the bcs content of the emitted events.

use std::str::FromStr;

use crate::error::BridgeError;
use crate::error::BridgeResult;
use crate::types::BridgeChainId;
use crate::types::TokenId;
use ethers::types::Address as EthAddress;
use move_core_types::language_storage::StructTag;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use sui_json_rpc_types::SuiEvent;
use sui_types::base_types::SuiAddress;

// TODO: Placeholder, this will need to match the actual event types defined in Move
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct EmittedSuiToEthTokenBridgeV1 {
    pub nonce: u64,
    pub sui_chain_id: BridgeChainId,
    pub eth_chain_id: BridgeChainId,
    pub sui_address: SuiAddress,
    pub eth_address: EthAddress,
    pub token_id: TokenId,
    pub amount: u128,
}

crate::declare_events!(
    // TODO: Placeholder, use right struct tag
    SuiToEthTokenBridgeV1(EmittedSuiToEthTokenBridgeV1) => "0x01::SuiToEthTokenBridge::SuiToEthTokenBridge",
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
