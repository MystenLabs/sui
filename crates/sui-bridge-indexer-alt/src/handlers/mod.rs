// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use sui_indexer_alt_framework::types::SUI_BRIDGE_OBJECT_ID;
use sui_indexer_alt_framework::types::full_checkpoint_content::CheckpointTransaction;

pub mod error_handler;
pub mod governance_action_handler;
pub mod token_transfer_data_handler;
pub mod token_transfer_handler;

const LIMITER: &IdentStr = ident_str!("limiter");
const BRIDGE: &IdentStr = ident_str!("bridge");
const COMMITTEE: &IdentStr = ident_str!("committee");
const TREASURY: &IdentStr = ident_str!("treasury");

const TOKEN_DEPOSITED_EVENT: &IdentStr = ident_str!("TokenDepositedEvent");
const TOKEN_TRANSFER_APPROVED: &IdentStr = ident_str!("TokenTransferApproved");
const TOKEN_TRANSFER_CLAIMED: &IdentStr = ident_str!("TokenTransferClaimed");

#[macro_export]
macro_rules! struct_tag {
    ($address:ident, $module:ident, $name:ident) => {{
        StructTag {
            address: $address,
            module: $module.into(),
            name: $name.into(),
            type_params: vec![],
        }
    }};
}

pub fn is_bridge_txn(txn: &CheckpointTransaction) -> bool {
    txn.input_objects
        .iter()
        .any(|obj| obj.id() == SUI_BRIDGE_OBJECT_ID)
}
