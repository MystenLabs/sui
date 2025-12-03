// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;

use crate::SUI_SYSTEM_ADDRESS;
use crate::balance::Balance;
use crate::base_types::ObjectID;
use crate::committee::EpochId;
use crate::error::{SuiError, SuiErrorKind};
use crate::gas_coin::MIST_PER_SUI;
use crate::id::{ID, UID};
use crate::object::{Data, Object};
use serde::Deserialize;
use serde::Serialize;

// === Pre SIP-39 Constants ===

/// Maximum number of active validators at any moment.
/// We do not allow the number of validators in any epoch to go above this.
pub const MAX_VALIDATOR_COUNT: u64 = 150;

#[deprecated(note = "SIP-39 removes min barreier for joining the validator set")]
/// Lower-bound on the amount of stake required to become a validator.
///
/// 30 million SUI
pub const MIN_VALIDATOR_JOINING_STAKE_MIST: u64 = 30_000_000 * MIST_PER_SUI;

#[deprecated(note = "SIP-39 removes low barreier for joining the validator set")]
/// Deprecated: with SIP-39 there is no longer a minimum stake requirement.
///
/// Validators with stake amount below `validator_low_stake_threshold` are considered to
/// have low stake and will be escorted out of the validator set after being below this
/// threshold for more than `validator_low_stake_grace_period` number of epochs.
///
/// 20 million SUI
pub const VALIDATOR_LOW_STAKE_THRESHOLD_MIST: u64 = 20_000_000 * MIST_PER_SUI;

#[deprecated(note = "SIP-39 removes very low barreier for joining the validator set")]
/// Validators with stake below `validator_very_low_stake_threshold` will be removed
/// immediately at epoch change, no grace period.
///
/// 15 million SUI
pub const VALIDATOR_VERY_LOW_STAKE_THRESHOLD_MIST: u64 = 15_000_000 * MIST_PER_SUI;

/// Number of epochs for a single phase of SIP-39 since the change
pub const SIP_39_PHASE_LENGTH: u64 = 14;

// === Post SIP-39 (Phase 1) ===

/// Minimum amount of voting power required to become a validator in Phase 1.
/// .12% of voting power
pub const VALIDATOR_MIN_POWER_PHASE_1: u64 = 12;

/// Low voting power threshold for validators in Phase 1.
/// Validators below this threshold fall into the "at risk" group.
/// .08% of voting power
pub const VALIDATOR_LOW_POWER_PHASE_1: u64 = 8;

/// Very low voting power threshold for validators in Phase 1.
/// Validators below this threshold will be removed immediately at epoch change.
/// .04% of voting power
pub const VALIDATOR_VERY_LOW_POWER_PHASE_1: u64 = 4;

// === Post SIP-39 (Phase 2) ===

/// Minimum amount of voting power required to become a validator in Phase 2.
/// .12% of voting power
pub const VALIDATOR_MIN_POWER_PHASE_2: u64 = 6;

/// Low voting power threshold for validators in Phase 2.
/// Validators below this threshold fall into the "at risk" group.
/// .08% of voting power
pub const VALIDATOR_LOW_POWER_PHASE_2: u64 = 4;

/// Very low voting power threshold for validators in Phase 2.
/// Validators below this threshold will be removed immediately at epoch change.
/// .04% of voting power
pub const VALIDATOR_VERY_LOW_POWER_PHASE_2: u64 = 2;

// === Post SIP-39 (Phase 3) ===

/// Minimum amount of voting power required to become a validator in Phase 3.
/// .03% of voting power
pub const VALIDATOR_MIN_POWER_PHASE_3: u64 = 3;

/// Low voting power threshold for validators in Phase 3.
/// Validators below this threshold fall into the "at risk" group.
/// .02% of voting power
pub const VALIDATOR_LOW_POWER_PHASE_3: u64 = 2;

/// Very low voting power threshold for validators in Phase 3.
/// Validators below this threshold will be removed immediately at epoch change.
/// .01% of voting power
pub const VALIDATOR_VERY_LOW_POWER_PHASE_3: u64 = 1;

/// A validator can have stake below `validator_low_stake_threshold`
/// for this many epochs before being kicked out.
pub const VALIDATOR_LOW_STAKE_GRACE_PERIOD: u64 = 7;

pub const STAKING_POOL_MODULE_NAME: &IdentStr = ident_str!("staking_pool");
pub const STAKED_SUI_STRUCT_NAME: &IdentStr = ident_str!("StakedSui");

pub const ADD_STAKE_MUL_COIN_FUN_NAME: &IdentStr = ident_str!("request_add_stake_mul_coin");
pub const ADD_STAKE_FUN_NAME: &IdentStr = ident_str!("request_add_stake");
pub const WITHDRAW_STAKE_FUN_NAME: &IdentStr = ident_str!("request_withdraw_stake");

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct StakedSui {
    id: UID,
    pool_id: ID,
    stake_activation_epoch: u64,
    principal: Balance,
}

impl StakedSui {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_SYSTEM_ADDRESS,
            module: STAKING_POOL_MODULE_NAME.to_owned(),
            name: STAKED_SUI_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }

    pub fn is_staked_sui(s: &StructTag) -> bool {
        s.address == SUI_SYSTEM_ADDRESS
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

    pub fn activation_epoch(&self) -> EpochId {
        self.stake_activation_epoch
    }

    pub fn request_epoch(&self) -> EpochId {
        // TODO: this might change when we implement warm up period.
        self.stake_activation_epoch.saturating_sub(1)
    }

    pub fn principal(&self) -> u64 {
        self.principal.value()
    }
}

impl TryFrom<&Object> for StakedSui {
    type Error = SuiError;
    fn try_from(object: &Object) -> Result<Self, Self::Error> {
        match &object.data {
            Data::Move(o) => {
                if o.type_().is_staked_sui() {
                    return bcs::from_bytes(o.contents()).map_err(|err| {
                        SuiErrorKind::TypeError {
                            error: format!("Unable to deserialize StakedSui object: {:?}", err),
                        }
                        .into()
                    });
                }
            }
            Data::Package(_) => {}
        }

        Err(SuiErrorKind::TypeError {
            error: format!("Object type is not a StakedSui: {:?}", object),
        }
        .into())
    }
}
