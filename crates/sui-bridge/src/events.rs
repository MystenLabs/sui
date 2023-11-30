// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use once_cell::sync::OnceCell;

use ethers::types::{Address as EthAddress, U256};
use move_core_types::language_storage::StructTag;
use serde::{Deserialize, Serialize};
use sui_json_rpc_types::SuiEvent;
use sui_types::base_types::SuiAddress;

// Placeholder, this will need to match the actual event types defined in Move
#[derive(Debug, Serialize, Deserialize)]
pub struct SuiToEthBridgeEvent {
    pub source_address: SuiAddress,
    pub destination_address: EthAddress,
    pub coin_name: String,
    pub amount: U256,
}

crate::declare_events!(
    SuiToEthTokenBridge(SuiToEthBridgeEvent) => "0x01::SuiToEthTokenBridge::SuiToEthTokenBridge",
    // Add new event types here. Format: EnumVariantName(Struct) => "StructTagString",
);

#[macro_export]
macro_rules! declare_events {
    ($($variant:ident($type:path) => $tag:expr),* $(,)?) => {
        // Declare the enum with its variants
        pub enum SuiBridgeEvent {
            $($variant($type),)*
        }

        // Declare a static `OnceCell` for each event type
        #[allow(non_upper_case_globals)]
        $(static $variant: OnceCell<StructTag> = OnceCell::new();)*

        // Initialize all declared struct tags
        fn init_all_struct_tags() {
            $($variant.get_or_init(|| {
                StructTag::from_str($tag).unwrap()
            });)*
        }

        // Try to convert a SuiEvent into SuiBridgeEvent
        impl SuiBridgeEvent {
            pub fn try_from_sui_event(event: &SuiEvent) -> anyhow::Result<Option<SuiBridgeEvent>> {
                init_all_struct_tags(); // Ensure all tags are initialized

                // Match against each event type
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
