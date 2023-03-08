// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;

use crate::balance::Balance;
use crate::base_types::{ObjectID, SuiAddress};
use crate::committee::EpochId;
use crate::error::SuiError;
use crate::id::{ID, UID};
use crate::object::{Data, Object};
use crate::SUI_FRAMEWORK_ADDRESS;
use schemars::JsonSchema;
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

// TODO: this no longer exists at Move level, we need to remove this and update the governance API.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct Delegation {
    pub id: UID,
    pub staked_sui_id: ID,
    pub pool_tokens: Balance,
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

    pub fn principal(&self) -> u64 {
        self.principal.value()
    }

    pub fn sui_token_lock(&self) -> Option<EpochId> {
        self.sui_token_lock
    }
}

impl TryFrom<&Object> for StakedSui {
    type Error = SuiError;
    fn try_from(object: &Object) -> Result<Self, Self::Error> {
        match &object.data {
            Data::Move(o) => {
                if o.type_ == StakedSui::type_() {
                    return bcs::from_bytes(o.contents()).map_err(|err| SuiError::TypeError {
                        error: format!("Unable to deserialize StakedSui object: {:?}", err),
                    });
                }
            }
            Data::Package(_) => {}
        }

        Err(SuiError::TypeError {
            error: format!("Object type is not a StakedSui: {:?}", object),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct DelegatedStake {
    pub staked_sui: StakedSui,
    pub delegation_status: DelegationStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub enum DelegationStatus {
    Pending,
    // TODO: remove the `Delegation` object here.
    Active(Delegation),
}
