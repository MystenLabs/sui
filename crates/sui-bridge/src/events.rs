// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use once_cell::sync::OnceCell;

use ethers::types::{Address as EthAddress, U256};
use move_core_types::language_storage::StructTag;
use serde::{Deserialize, Serialize};
use sui_json_rpc_types::SuiEvent;
use sui_types::base_types::SuiAddress;

// TODO: Placeholder, this will need to match the actual event types defined in Move
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct SuiToEthBridgeEvent {
    pub source_address: SuiAddress,
    pub destination_address: EthAddress,
    pub coin_name: String,
    // this is probably not the right type here
    pub amount: U256,
}

crate::declare_events!(
    // TODO: Placeholder, use right struct tag
    SuiToEthTokenBridge(SuiToEthBridgeEvent) => "0x01::SuiToEthTokenBridge::SuiToEthTokenBridge",
    // Add new event types here. Format: EnumVariantName(Struct) => "StructTagString",
);

#[macro_export]
macro_rules! declare_events {
    ($($variant:ident($type:path) => $tag:expr),* $(,)?) => {

        #[derive(Debug, Eq, PartialEq, Clone)]
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
            pub fn try_from_sui_event(event: &SuiEvent) -> anyhow::Result<Option<SuiBridgeEvent>> {
                init_all_struct_tags(); // Ensure all tags are initialized

                // Unwrap safe: we inited above
                $(
                    if &event.type_ == $variant.get().unwrap() {
                        return Ok(Some(SuiBridgeEvent::$variant(bcs::from_bytes(&event.bcs)?)));
                    }
                )*
                Ok(None)
            }
        }
    };
}
