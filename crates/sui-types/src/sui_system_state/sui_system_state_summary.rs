// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{AuthorityName, ObjectID, SuiAddress};
use crate::committee::{Committee, CommitteeWithNetworkMetadata, NetworkMetadata};
use crate::multiaddr::Multiaddr;
use fastcrypto::encoding::Base58;
use fastcrypto::traits::ToFromBytes;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::BTreeMap;

use crate::id::ID;

/// This is the JSON-RPC type for the SUI system state object.
/// It flattens all fields to make them top-level fields such that it as minimum
/// dependencies to the internal data structures of the SUI system state type.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SuiSystemStateSummary {
    /// The current epoch ID, starting from 0.
    pub epoch: u64,
    /// The current protocol version, starting from 1.
    pub protocol_version: u64,
    /// The current version of the system state data structure type.
    pub system_state_version: u64,
    /// The storage fund balance.
    pub storage_fund: u64,
    /// The reference gas price for the current epoch.
    pub reference_gas_price: u64,
    /// Whether the system is running in a downgraded safe mode due to a non-recoverable bug.
    /// This is set whenever we failed to execute advance_epoch, and ended up executing advance_epoch_safe_mode.
    /// It can be reset once we are able to successfully execute advance_epoch.
    pub safe_mode: bool,
    /// Unix timestamp of the current epoch start
    pub epoch_start_timestamp_ms: u64,

    // System parameters
    /// The starting epoch in which various on-chain governance features take effect.
    pub governance_start_epoch: u64,

    /// The duration of an epoch, in milliseconds.
    pub epoch_duration_ms: u64,

    // Stake subsidy information
    /// This counter may be different from the current epoch number if
    /// in some epochs we decide to skip the subsidy.
    pub stake_subsidy_epoch_counter: u64,
    /// Balance of SUI set aside for stake subsidies that will be drawn down over time.
    pub stake_subsidy_balance: u64,
    /// The amount of stake subsidy to be drawn down per epoch.
    /// This amount decays and decreases over time.
    pub stake_subsidy_current_epoch_amount: u64,

    // Validator set
    /// Total amount of stake from all active validators at the beginning of the epoch.
    pub total_stake: u64,
    /// The list of active validators in the current epoch.
    pub active_validators: Vec<SuiValidatorSummary>,
    /// ID of the object that contains the list of new validators that will join at the end of the epoch.
    pub pending_active_validators_id: ObjectID,
    /// Number of new validators that will join at the end of the epoch.
    pub pending_active_validators_size: u64,
    /// Removal requests from the validators. Each element is an index
    /// pointing to `active_validators`.
    pub pending_removals: Vec<u64>,
    /// ID of the object that maps from staking pool's ID to the sui address of a validator.
    pub staking_pool_mappings_id: ObjectID,
    /// Number of staking pool mappings.
    pub staking_pool_mappings_size: u64,
    /// ID of the object that maps from a staking pool ID to the inactive validator that has that pool as its staking pool.
    pub inactive_pools_id: ObjectID,
    /// Number of inactive staking pools.
    pub inactive_pools_size: u64,
    /// ID of the object that stores preactive validators, mapping their addresses to their `Validator` structs.
    pub validator_candidates_id: ObjectID,
    /// Number of preactive validators.
    pub validator_candidates_size: u64,
    /// Map storing the number of epochs for which each validator has been below the low stake threshold.
    pub at_risk_validators: Vec<(SuiAddress, u64)>,
    /// A map storing the records of validator reporting each other.
    pub validator_report_records: Vec<(SuiAddress, Vec<SuiAddress>)>,
}

impl SuiSystemStateSummary {
    pub fn get_sui_committee_for_benchmarking(&self) -> CommitteeWithNetworkMetadata {
        let mut voting_rights = BTreeMap::new();
        let mut network_metadata = BTreeMap::new();
        for validator in &self.active_validators {
            let name = AuthorityName::from_bytes(&validator.protocol_pubkey_bytes).unwrap();
            voting_rights.insert(name, validator.voting_power);
            network_metadata.insert(
                name,
                NetworkMetadata {
                    network_address: Multiaddr::try_from(validator.net_address.clone()).unwrap(),
                },
            );
        }
        CommitteeWithNetworkMetadata {
            committee: Committee::new(self.epoch, voting_rights),
            network_metadata,
        }
    }
}

/// This is the JSON-RPC type for the SUI validator. It flattens all inner structures
/// to top-level fields so that they are decoupled from the internal definitions.
#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SuiValidatorSummary {
    // Metadata
    pub sui_address: SuiAddress,
    #[schemars(with = "Base58")]
    #[serde_as(as = "Base58")]
    pub protocol_pubkey_bytes: Vec<u8>,
    #[schemars(with = "Base58")]
    #[serde_as(as = "Base58")]
    pub network_pubkey_bytes: Vec<u8>,
    #[schemars(with = "Base58")]
    #[serde_as(as = "Base58")]
    pub worker_pubkey_bytes: Vec<u8>,
    #[schemars(with = "Base58")]
    #[serde_as(as = "Base58")]
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
    pub net_address: String,
    pub p2p_address: String,
    pub primary_address: String,
    pub worker_address: String,
    #[schemars(with = "Option<Base58>")]
    #[serde_as(as = "Option<Base58>")]
    pub next_epoch_protocol_pubkey_bytes: Option<Vec<u8>>,
    #[schemars(with = "Option<Base58>")]
    #[serde_as(as = "Option<Base58>")]
    pub next_epoch_proof_of_possession: Option<Vec<u8>>,
    #[schemars(with = "Option<Base58>")]
    #[serde_as(as = "Option<Base58>")]
    pub next_epoch_network_pubkey_bytes: Option<Vec<u8>>,
    #[schemars(with = "Option<Base58>")]
    #[serde_as(as = "Option<Base58>")]
    pub next_epoch_worker_pubkey_bytes: Option<Vec<u8>>,
    pub next_epoch_net_address: Option<String>,
    pub next_epoch_p2p_address: Option<String>,
    pub next_epoch_primary_address: Option<String>,
    pub next_epoch_worker_address: Option<String>,

    pub voting_power: u64,
    pub operation_cap_id: ID,
    pub gas_price: u64,
    pub commission_rate: u64,
    pub next_epoch_stake: u64,
    pub next_epoch_gas_price: u64,
    pub next_epoch_commission_rate: u64,

    // Staking pool information
    /// ID of the staking pool object.
    pub staking_pool_id: ObjectID,
    /// The epoch at which this pool became active.
    pub staking_pool_activation_epoch: Option<u64>,
    /// The epoch at which this staking pool ceased to be active. `None` = {pre-active, active},
    pub staking_pool_deactivation_epoch: Option<u64>,
    /// The total number of SUI tokens in this pool.
    pub staking_pool_sui_balance: u64,
    /// The epoch stake rewards will be added here at the end of each epoch.
    pub rewards_pool: u64,
    /// Total number of pool tokens issued by the pool.
    pub pool_token_balance: u64,
    /// Pending stake amount for this epoch.
    pub pending_stake: u64,
    /// Pending stake withdrawn during the current epoch, emptied at epoch boundaries.
    pub pending_total_sui_withdraw: u64,
    /// Pending pool token withdrawn during the current epoch, emptied at epoch boundaries.
    pub pending_pool_token_withdraw: u64,
    /// ID of the exchange rate table object.
    pub exchange_rates_id: ObjectID,
    /// Number of exchange rates in the table.
    pub exchange_rates_size: u64,
}
