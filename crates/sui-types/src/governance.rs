// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;

use crate::balance::Balance;
use crate::base_types::{ObjectID, SuiAddress};
use crate::committee::EpochId;
use crate::id::UID;
use crate::SUI_FRAMEWORK_ADDRESS;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

pub const STAKING_POOL_MODULE_NAME: &IdentStr = ident_str!("staking_pool");
pub const STAKED_SUI_STRUCT_NAME: &IdentStr = ident_str!("StakedSui");
pub const DELEGATION_STRUCT_NAME: &IdentStr = ident_str!("Delegation");

pub const ADD_DELEGATION_MUL_COIN_FUN_NAME: &IdentStr =
    ident_str!("request_add_delegation_mul_coin");
pub const ADD_DELEGATION_FUN_NAME: &IdentStr = ident_str!("request_add_delegation_mul_coin");
pub const ADD_DELEGATION_LOCKED_COIN_FUN_NAME: &IdentStr =
    ident_str!("request_add_delegation_mul_locked_coin");
pub const WITHDRAW_DELEGATION_FUN_NAME: &IdentStr = ident_str!("request_withdraw_delegation");
pub const SWITCH_DELEGATION_FUN_NAME: &IdentStr = ident_str!("request_switch_delegation");

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct Delegation {
    id: UID,
    validator_address: SuiAddress,
    pool_starting_epoch: EpochId,
    pool_tokens: Balance,
    principal_sui_amount: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct PendingDelegation {
    pub validator_address: SuiAddress,
    pub pool_starting_epoch: EpochId,
    pub principal_sui_amount: u64,
}

impl Delegation {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: STAKING_POOL_MODULE_NAME.to_owned(),
            name: DELEGATION_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct StakedSui {
    id: UID,
    principal: Balance,
    locked_until_epoch: Option<EpochId>,
}

impl StakedSui {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: STAKING_POOL_MODULE_NAME.to_owned(),
            name: STAKED_SUI_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }

    pub fn id(&self) -> ObjectID {
        self.id.id.bytes
    }

    pub fn principal(&self) -> u64 {
        self.principal.value()
    }

    pub fn locked_until_epoch(&self) -> Option<EpochId> {
        self.locked_until_epoch
    }
}
