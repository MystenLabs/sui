// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::traits::ToFromBytes;
use move_core_types::{
    account_address::AccountAddress, ident_str, identifier::IdentStr, language_storage::StructTag,
};
use serde::{Deserialize, Serialize};

use crate::base_types::AuthorityName;
use crate::committee::{Committee, StakeUnit};
use crate::crypto::AuthorityPublicKeyBytes;
use crate::{
    balance::{Balance, Supply},
    id::UID,
    SUI_FRAMEWORK_ADDRESS,
};

const SUI_SYSTEM_STATE_STRUCT_NAME: &IdentStr = ident_str!("SuiSystemState");
pub const SUI_SYSTEM_MODULE_NAME: &IdentStr = ident_str!("sui_system");
pub const ADVANCE_EPOCH_FUNCTION_NAME: &IdentStr = ident_str!("advance_epoch");

/// Rust version of the Move sui::sui_system::SystemParameters type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SystemParameters {
    pub min_validator_stake: u64,
    pub max_validator_candidate_count: u64,
    pub storage_gas_price: u64,
}

/// Rust version of the Move std::option::Option type.
/// Putting it in this file because it's only used here.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct MoveOption<T> {
    pub vec: Vec<T>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct ValidatorMetadata {
    pub sui_address: AccountAddress,
    pub pubkey_bytes: Vec<u8>,
    pub network_pubkey_bytes: Vec<u8>,
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: Vec<u8>,
    pub net_address: Vec<u8>,
    pub next_epoch_stake: u64,
    pub next_epoch_delegation: u64,
    pub next_epoch_gas_price: u64,
}

impl ValidatorMetadata {
    pub fn to_validator_and_stake_pair(&self) -> (AuthorityName, StakeUnit) {
        (
            // TODO: Make sure we are actually verifying this on-chain.
            AuthorityPublicKeyBytes::from_bytes(self.pubkey_bytes.as_ref())
                .expect("Validity of public key bytes should be verified on-chain"),
            self.next_epoch_stake + self.next_epoch_delegation,
        )
    }
}

/// Rust version of the Move sui::validator::Validator type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Validator {
    pub metadata: ValidatorMetadata,
    pub stake_amount: u64,
    pub pending_stake: u64,
    pub pending_withdraw: u64,
    pub gas_price: u64,
    pub delegation_staking_pool: StakingPool,
}

/// Rust version of the Move sui::staking_pool::PendingDelegationEntry type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct PendingDelegationEntry {
    pub delegator: AccountAddress,
    pub sui_amount: u64,
}

/// Rust version of the Move sui::staking_pool::StakingPool type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct StakingPool {
    pub validator_address: AccountAddress,
    pub starting_epoch: u64,
    pub epoch_starting_sui_balance: u64,
    pub sui_balance: u64,
    pub rewards_pool: Balance,
    pub delegation_token_supply: Supply,
    pub pending_delegations: Vec<PendingDelegationEntry>,
}

/// Rust version of the Move sui::validator_set::ValidatorSet type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct ValidatorSet {
    pub validator_stake: u64,
    pub delegation_stake: u64,
    pub quorum_stake_threshold: u64,
    pub active_validators: Vec<Validator>,
    pub pending_validators: Vec<Validator>,
    pub pending_removals: Vec<u64>,
    pub next_epoch_validators: Vec<ValidatorMetadata>,
}

/// Rust version of the Move sui::sui_system::SuiSystemState type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SuiSystemState {
    pub info: UID,
    pub epoch: u64,
    pub validators: ValidatorSet,
    pub treasury_cap: Supply,
    pub storage_fund: Balance,
    pub parameters: SystemParameters,
    pub reference_gas_price: u64,
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

    pub fn get_next_epoch_committee(&self) -> Committee {
        Committee::new(
            self.epoch + 1,
            self.validators
                .next_epoch_validators
                .iter()
                .map(ValidatorMetadata::to_validator_and_stake_pair)
                .collect(),
        )
        // unwrap is safe because we should have verified the committee on-chain.
        // TODO: Make sure we actually verify it.
        .unwrap()
    }
}
