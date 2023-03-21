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
use serde::Deserialize;
use serde::Serialize;

/// Minimum amount of stake required for a validator to be in the validator set
pub const MINIMUM_VALIDATOR_STAKE_SUI: u64 = 25_000_000;

pub const STAKING_POOL_MODULE_NAME: &IdentStr = ident_str!("staking_pool");
pub const STAKED_SUI_STRUCT_NAME: &IdentStr = ident_str!("StakedSui");

pub const ADD_STAKE_MUL_COIN_FUN_NAME: &IdentStr = ident_str!("request_add_stake_mul_coin");
pub const ADD_STAKE_FUN_NAME: &IdentStr = ident_str!("request_add_stake");
pub const WITHDRAW_STAKE_FUN_NAME: &IdentStr = ident_str!("request_withdraw_stake");

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct StakedSui {
    id: UID,
    pool_id: ID,
    validator_address: SuiAddress,
    stake_activation_epoch: u64,
    principal: Balance,
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

    pub fn is_staked_sui(s: &StructTag) -> bool {
        s.address == SUI_FRAMEWORK_ADDRESS
            && s.module.as_ident_str() == STAKING_POOL_MODULE_NAME
            && s.name.as_ident_str() == STAKED_SUI_STRUCT_NAME
            && s.type_params.is_empty()
    }

    pub fn id(&self) -> ObjectID {
        self.id.id.bytes
    }

    pub fn pool_id(&self) -> ObjectID {
        self.pool_id.bytes
    }

    pub fn request_epoch(&self) -> EpochId {
        self.stake_activation_epoch
    }

    pub fn principal(&self) -> u64 {
        self.principal.value()
    }

    pub fn validator_address(&self) -> SuiAddress {
        self.validator_address
    }
}

impl TryFrom<&Object> for StakedSui {
    type Error = SuiError;
    fn try_from(object: &Object) -> Result<Self, Self::Error> {
        match &object.data {
            Data::Move(o) => {
                if o.type_().is_staked_sui() {
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
