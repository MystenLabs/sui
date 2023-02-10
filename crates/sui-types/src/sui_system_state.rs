// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{AuthorityName, ObjectID, SuiAddress};
use crate::collection_types::{VecMap, VecSet};
use crate::committee::{Committee, CommitteeWithNetAddresses, ProtocolVersion, StakeUnit};
use crate::crypto::{AuthorityPublicKeyBytes, NetworkPublicKey};
use crate::{
    balance::{Balance, Supply},
    id::UID,
    SUI_FRAMEWORK_ADDRESS,
};
use fastcrypto::traits::ToFromBytes;
use move_core_types::{ident_str, identifier::IdentStr, language_storage::StructTag};
use multiaddr::Multiaddr;
use narwhal_config::{Committee as NarwhalCommittee, WorkerCache, WorkerIndex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const SUI_SYSTEM_STATE_STRUCT_NAME: &IdentStr = ident_str!("SuiSystemState");
pub const SUI_SYSTEM_MODULE_NAME: &IdentStr = ident_str!("sui_system");
pub const ADVANCE_EPOCH_FUNCTION_NAME: &IdentStr = ident_str!("advance_epoch");
pub const ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME: &IdentStr = ident_str!("advance_epoch_safe_mode");

/// Rust version of the Move sui::sui_system::SystemParameters type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct SystemParameters {
    pub min_validator_stake: u64,
    pub max_validator_candidate_count: u64,
}

/// Rust version of the Move std::option::Option type.
/// Putting it in this file because it's only used here.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct MoveOption<T> {
    pub vec: Vec<T>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct ValidatorMetadata {
    pub sui_address: SuiAddress,
    pub pubkey_bytes: Vec<u8>,
    pub network_pubkey_bytes: Vec<u8>,
    pub worker_pubkey_bytes: Vec<u8>,
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: Vec<u8>,
    pub description: Vec<u8>,
    pub image_url: Vec<u8>,
    pub project_url: Vec<u8>,
    pub net_address: Vec<u8>,
    pub consensus_address: Vec<u8>,
    pub worker_address: Vec<u8>,
    pub next_epoch_stake: u64,
    pub next_epoch_delegation: u64,
    pub next_epoch_gas_price: u64,
    pub next_epoch_commission_rate: u64,
}

impl ValidatorMetadata {
    pub fn to_next_epoch_validator_and_stake_pair(&self) -> (AuthorityName, StakeUnit) {
        (
            // TODO: Make sure we are actually verifying this on-chain.
            AuthorityPublicKeyBytes::from_bytes(self.pubkey_bytes.as_ref())
                .expect("Validity of public key bytes should be verified on-chain"),
            self.next_epoch_stake + self.next_epoch_delegation,
        )
    }
}

/// Rust version of the Move sui::validator::Validator type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct Validator {
    pub metadata: ValidatorMetadata,
    pub voting_power: u64,
    pub stake_amount: u64,
    pub pending_stake: u64,
    pub pending_withdraw: u64,
    pub gas_price: u64,
    pub delegation_staking_pool: StakingPool,
    pub commission_rate: u64,
}

impl Validator {
    pub fn to_current_epoch_committee_with_net_addresses(
        &self,
    ) -> (AuthorityName, StakeUnit, Vec<u8>) {
        (
            // TODO: Make sure we are actually verifying this on-chain.
            AuthorityPublicKeyBytes::from_bytes(self.metadata.pubkey_bytes.as_ref())
                .expect("Validity of public key bytes should be verified on-chain"),
            self.voting_power,
            self.metadata.net_address.clone(),
        )
    }
}

/// Rust version of the Move sui::staking_pool::PendingDelegationEntry type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct PendingDelegationEntry {
    pub delegator: SuiAddress,
    pub sui_amount: u64,
    pub staked_sui_id: ObjectID,
}

/// Rust version of the Move sui::staking_pool::PendingWithdrawEntry type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct PendingWithdrawEntry {
    delegator: SuiAddress,
    principal_withdraw_amount: u64,
    withdrawn_pool_tokens: Balance,
}

/// Rust version of the Move sui::table::Table type. Putting it here since
/// we only use it in sui_system in the framework.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct TableVec {
    pub contents: Table,
}

impl Default for TableVec {
    fn default() -> Self {
        TableVec {
            contents: Table {
                id: ObjectID::from(SuiAddress::ZERO),
                size: 0,
            },
        }
    }
}

/// Rust version of the Move sui::table::Table type. Putting it here since
/// we only use it in sui_system in the framework.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct Table {
    pub id: ObjectID,
    pub size: u64,
}

/// Rust version of the Move sui::linked_table::LinkedTable type. Putting it here since
/// we only use it in sui_system in the framework.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct LinkedTable<K> {
    pub id: ObjectID,
    pub size: u64,
    pub head: MoveOption<K>,
    pub tail: MoveOption<K>,
}

impl<K> Default for LinkedTable<K> {
    fn default() -> Self {
        LinkedTable {
            id: ObjectID::from(SuiAddress::ZERO),
            size: 0,
            head: MoveOption { vec: vec![] },
            tail: MoveOption { vec: vec![] },
        }
    }
}

/// Rust version of the Move sui::staking_pool::StakingPool type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct StakingPool {
    pub validator_address: SuiAddress,
    pub starting_epoch: u64,
    pub sui_balance: u64,
    pub rewards_pool: Balance,
    pub delegation_token_supply: Supply,
    pub pending_delegations: LinkedTable<ObjectID>,
    pub pending_withdraws: TableVec,
}

/// Rust version of the Move sui::validator_set::ValidatorPair type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct ValidatorPair {
    from: SuiAddress,
    to: SuiAddress,
}

/// Rust version of the Move sui::validator_set::ValidatorSet type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct ValidatorSet {
    pub validator_stake: u64,
    pub delegation_stake: u64,
    pub active_validators: Vec<Validator>,
    pub pending_validators: Vec<Validator>,
    pub pending_removals: Vec<u64>,
    pub next_epoch_validators: Vec<ValidatorMetadata>,
    pub pending_delegation_switches: VecMap<ValidatorPair, TableVec>,
}

/// Rust version of the Move sui::sui_system::SuiSystemState type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct SuiSystemState {
    pub info: UID,
    pub epoch: u64,
    pub protocol_version: u64,
    pub validators: ValidatorSet,
    pub treasury_cap: Supply,
    pub storage_fund: Balance,
    pub parameters: SystemParameters,
    pub reference_gas_price: u64,
    pub validator_report_records: VecMap<SuiAddress, VecSet<SuiAddress>>,
    pub stake_subsidy: StakeSubsidy,
    pub safe_mode: bool,
    pub epoch_start_timestamp_ms: u64,
    // TODO: Use getters instead of all pub.
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct StakeSubsidy {
    pub epoch_counter: u64,
    pub balance: Balance,
    pub current_epoch_amount: u64,
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

    pub fn get_current_epoch_committee(&self) -> CommitteeWithNetAddresses {
        let mut voting_rights = BTreeMap::new();
        let mut net_addresses = BTreeMap::new();
        for validator in &self.validators.active_validators {
            let (name, voting_stake, net_address) =
                validator.to_current_epoch_committee_with_net_addresses();
            voting_rights.insert(name, voting_stake);
            net_addresses.insert(name, net_address);
        }
        CommitteeWithNetAddresses {
            committee: Committee::new(
                self.epoch,
                ProtocolVersion(self.protocol_version),
                voting_rights,
            )
            // unwrap is safe because we should have verified the committee on-chain.
            // TODO: Make sure we actually verify it.
            .unwrap(),
            net_addresses,
        }
    }

    #[allow(clippy::mutable_key_type)]
    pub fn get_current_epoch_narwhal_committee(&self) -> NarwhalCommittee {
        let narwhal_committee = self
            .validators
            .active_validators
            .iter()
            .map(|validator| {
                let name = narwhal_crypto::PublicKey::from_bytes(&validator.metadata.pubkey_bytes)
                    .expect("Can't get narwhal public key");
                let network_key = narwhal_crypto::NetworkPublicKey::from_bytes(
                    &validator.metadata.network_pubkey_bytes,
                )
                .expect("Can't get narwhal network key");
                let primary_address =
                    Multiaddr::try_from(validator.metadata.consensus_address.clone())
                        .expect("Can't get narwhal primary address");
                let authority = narwhal_config::Authority {
                    stake: validator.voting_power as narwhal_config::Stake,
                    primary_address,
                    network_key,
                };
                (name, authority)
            })
            .collect();

        narwhal_config::Committee {
            authorities: narwhal_committee,
            epoch: self.epoch as narwhal_config::Epoch,
        }
    }

    #[allow(clippy::mutable_key_type)]
    pub fn get_current_epoch_narwhal_worker_cache(
        &self,
        transactions_address: &Multiaddr,
    ) -> WorkerCache {
        let workers: BTreeMap<narwhal_crypto::PublicKey, WorkerIndex> = self
            .validators
            .active_validators
            .iter()
            .map(|validator| {
                let name = narwhal_crypto::PublicKey::from_bytes(&validator.metadata.pubkey_bytes)
                    .expect("Can't get narwhal public key");
                let worker_address = Multiaddr::try_from(validator.metadata.worker_address.clone())
                    .expect("Can't get worker address");
                let workers = [(
                    0,
                    narwhal_config::WorkerInfo {
                        name: NetworkPublicKey::from_bytes(&validator.metadata.worker_pubkey_bytes)
                            .expect("Can't get worker key"),
                        transactions: transactions_address.clone(),
                        worker_address,
                    },
                )]
                .into_iter()
                .collect();
                let worker_index = WorkerIndex(workers);

                (name, worker_index)
            })
            .collect();
        WorkerCache {
            workers,
            epoch: self.epoch,
        }
    }
}
