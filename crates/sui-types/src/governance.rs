// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;

use crate::balance::Balance;
use crate::base_types::{ObjectID, SuiAddress};
use crate::committee::EpochId;
use crate::id::{ID, UID};
use crate::SUI_FRAMEWORK_ADDRESS;
use serde::Deserialize;
use serde::Serialize;

/// Minimum amount of stake required for a validator to be in the validator set
pub const MINIMUM_VALIDATOR_STAKE_SUI: u64 = 25_000_000;

pub const STAKING_POOL_MODULE_NAME: &IdentStr = ident_str!("staking_pool");
pub const STAKED_SUI_STRUCT_NAME: &IdentStr = ident_str!("StakedSui");
pub const DELEGATION_STRUCT_NAME: &IdentStr = ident_str!("Delegation");

pub const ADD_DELEGATION_MUL_COIN_FUN_NAME: &IdentStr =
    ident_str!("request_add_delegation_mul_coin");
pub const ADD_DELEGATION_FUN_NAME: &IdentStr = ident_str!("request_add_delegation_mul_coin");
pub const ADD_DELEGATION_LOCKED_COIN_FUN_NAME: &IdentStr =
    ident_str!("request_add_delegation_mul_locked_coin");
pub const WITHDRAW_DELEGATION_FUN_NAME: &IdentStr = ident_str!("request_withdraw_delegation");

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct StakedSui {
    id: UID,
    pool_id: ID,
    validator_address: SuiAddress,
    delegation_request_epoch: u64,
    principal: Balance,
    sui_token_lock: Option<EpochId>,
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

    pub fn pool_id(&self) -> ObjectID {
        self.pool_id.bytes
    }

    pub fn request_epoch(&self) -> EpochId {
        self.delegation_request_epoch
    }

    pub fn principal(&self) -> u64 {
        self.principal.value()
    }

    pub fn sui_token_lock(&self) -> Option<EpochId> {
        self.sui_token_lock
    }
}
