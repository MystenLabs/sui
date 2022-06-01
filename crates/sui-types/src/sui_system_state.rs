// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    account_address::AccountAddress, ident_str, identifier::IdentStr, language_storage::StructTag,
};
use serde::{Deserialize, Serialize};

use crate::{balance::Balance, coin::TreasuryCap, id::VersionedID, SUI_FRAMEWORK_ADDRESS};

const SUI_SYSTEM_STATE_STRUCT_NAME: &IdentStr = ident_str!("SuiSystemState");
pub const SUI_SYSTEM_MODULE_NAME: &IdentStr = ident_str!("SuiSystem");
pub const ADVANCE_EPOCH_FUNCTION_NAME: &IdentStr = ident_str!("advance_epoch");

/// Rust version of the Move Sui::SuiSystem::SystemParameters type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SystemParameters {
    pub min_validator_stake: u64,
    pub max_validator_stake: u64,
    pub max_validator_candidate_count: u64,
}

/// Rust version of the Move Std::Option::Option type.
/// Putting it in this file because it's only used here.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct MoveOption<T> {
    pub vec: Vec<T>,
}

/// Rust version of the Move Sui::Validator::Validator type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Validator {
    pub sui_address: AccountAddress,
    pub pubkey_bytes: Vec<u8>,
    pub name: Vec<u8>,
    pub net_address: Vec<u8>,
    pub stake: Balance,
    pub delegation: u64,
    pub pending_stake: MoveOption<Balance>,
    pub pending_withdraw: u64,
    pub pending_delegation: u64,
    pub pending_delegation_withdraw: u64,
    pub delegator_count: u64,
    pub pending_delegator_count: u64,
    pub pending_delegator_withdraw_count: u64,
}

/// Rust version of the Move Sui::ValidatorSet::ValidatorSet type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct ValidatorSet {
    pub validator_stake: u64,
    pub delegation_stake: u64,
    pub quorum_stake_threshold: u64,
    pub active_validators: Vec<Validator>,
    pub pending_validators: Vec<Validator>,
    pub pending_removals: Vec<u64>,
}

/// Rust version of the Move Sui::SuiSystem::SuiSystemState type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SuiSystemState {
    pub id: VersionedID,
    pub epoch: u64,
    pub validators: ValidatorSet,
    pub treasury_cap: TreasuryCap,
    pub storage_fund: Balance,
    pub parameters: SystemParameters,
    pub delegation_reward: Balance,
    // TODO: Use getters instead of all pub.
}

impl SuiSystemState {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: SUI_SYSTEM_STATE_STRUCT_NAME.to_owned(),
            module: SUI_SYSTEM_MODULE_NAME.to_owned(),
            type_params: vec![],
        }
    }
}
