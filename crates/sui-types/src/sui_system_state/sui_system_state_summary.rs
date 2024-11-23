// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{SuiSystemState, SuiSystemStateTrait};
use crate::base_types::{AuthorityName, ObjectID, SuiAddress};
use crate::committee::{CommitteeWithNetworkMetadata, NetworkMetadata};
use crate::crypto::NetworkPublicKey;
use crate::dynamic_field::get_dynamic_field_from_store;
use crate::error::SuiError;
use crate::id::ID;
use crate::multiaddr::Multiaddr;
use crate::storage::ObjectStore;
use crate::sui_serde::BigInt;
use crate::sui_serde::Readable;
use crate::sui_system_state::get_validator_from_table;
use fastcrypto::encoding::Base64;
use fastcrypto::traits::ToFromBytes;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

/// This is the JSON-RPC type for the SUI system state object.
/// It flattens all fields to make them top-level fields such that it as minimum
/// dependencies to the internal data structures of the SUI system state type.

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SuiSystemStateSummary {
    /// The current epoch ID, starting from 0.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub epoch: u64,
    /// The current protocol version, starting from 1.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub protocol_version: u64,
    /// The current version of the system state data structure type.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub system_state_version: u64,
    /// The storage rebates of all the objects on-chain stored in the storage fund.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub storage_fund_total_object_storage_rebates: u64,
    /// The non-refundable portion of the storage fund coming from storage reinvestment, non-refundable
    /// storage rebates and any leftover staking rewards.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub storage_fund_non_refundable_balance: u64,
    /// The reference gas price for the current epoch.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub reference_gas_price: u64,
    /// Whether the system is running in a downgraded safe mode due to a non-recoverable bug.
    /// This is set whenever we failed to execute advance_epoch, and ended up executing advance_epoch_safe_mode.
    /// It can be reset once we are able to successfully execute advance_epoch.
    pub safe_mode: bool,
    /// Amount of storage rewards accumulated (and not yet distributed) during safe mode.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub safe_mode_storage_rewards: u64,
    /// Amount of computation rewards accumulated (and not yet distributed) during safe mode.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub safe_mode_computation_rewards: u64,
    /// Amount of storage rebates accumulated (and not yet burned) during safe mode.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub safe_mode_storage_rebates: u64,
    /// Amount of non-refundable storage fee accumulated during safe mode.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub safe_mode_non_refundable_storage_fee: u64,
    /// Unix timestamp of the current epoch start
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub epoch_start_timestamp_ms: u64,

    // System parameters
    /// The duration of an epoch, in milliseconds.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub epoch_duration_ms: u64,

    /// The starting epoch in which stake subsidies start being paid out
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub stake_subsidy_start_epoch: u64,

    /// Maximum number of active validators at any moment.
    /// We do not allow the number of validators in any epoch to go above this.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub max_validator_count: u64,

    /// Lower-bound on the amount of stake required to become a validator.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub min_validator_joining_stake: u64,

    /// Validators with stake amount below `validator_low_stake_threshold` are considered to
    /// have low stake and will be escorted out of the validator set after being below this
    /// threshold for more than `validator_low_stake_grace_period` number of epochs.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub validator_low_stake_threshold: u64,

    /// Validators with stake below `validator_very_low_stake_threshold` will be removed
    /// immediately at epoch change, no grace period.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub validator_very_low_stake_threshold: u64,

    /// A validator can have stake below `validator_low_stake_threshold`
    /// for this many epochs before being kicked out.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub validator_low_stake_grace_period: u64,

    // Stake subsidy information
    /// Balance of SUI set aside for stake subsidies that will be drawn down over time.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub stake_subsidy_balance: u64,
    /// This counter may be different from the current epoch number if
    /// in some epochs we decide to skip the subsidy.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub stake_subsidy_distribution_counter: u64,
    /// The amount of stake subsidy to be drawn down per epoch.
    /// This amount decays and decreases over time.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub stake_subsidy_current_distribution_amount: u64,
    /// Number of distributions to occur before the distribution amount decays.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub stake_subsidy_period_length: u64,
    /// The rate at which the distribution amount decays at the end of each
    /// period. Expressed in basis points.
    pub stake_subsidy_decrease_rate: u16,

    // Validator set
    /// Total amount of stake from all active validators at the beginning of the epoch.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub total_stake: u64,
    /// The list of active validators in the current epoch.
    pub active_validators: Vec<SuiValidatorSummary>,
    /// ID of the object that contains the list of new validators that will join at the end of the epoch.
    pub pending_active_validators_id: ObjectID,
    /// Number of new validators that will join at the end of the epoch.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub pending_active_validators_size: u64,
    /// Removal requests from the validators. Each element is an index
    /// pointing to `active_validators`.
    #[schemars(with = "Vec<BigInt<u64>>")]
    #[serde_as(as = "Vec<Readable<BigInt<u64>, _>>")]
    pub pending_removals: Vec<u64>,
    /// ID of the object that maps from staking pool's ID to the sui address of a validator.
    pub staking_pool_mappings_id: ObjectID,
    /// Number of staking pool mappings.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub staking_pool_mappings_size: u64,
    /// ID of the object that maps from a staking pool ID to the inactive validator that has that pool as its staking pool.
    pub inactive_pools_id: ObjectID,
    /// Number of inactive staking pools.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub inactive_pools_size: u64,
    /// ID of the object that stores preactive validators, mapping their addresses to their `Validator` structs.
    pub validator_candidates_id: ObjectID,
    /// Number of preactive validators.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub validator_candidates_size: u64,
    /// Map storing the number of epochs for which each validator has been below the low stake threshold.
    #[schemars(with = "Vec<(SuiAddress, BigInt<u64>)>")]
    #[serde_as(as = "Vec<(_, Readable<BigInt<u64>, _>)>")]
    pub at_risk_validators: Vec<(SuiAddress, u64)>,
    /// A map storing the records of validator reporting each other.
    pub validator_report_records: Vec<(SuiAddress, Vec<SuiAddress>)>,
}

impl SuiSystemStateSummary {
    pub fn get_sui_committee_for_benchmarking(&self) -> CommitteeWithNetworkMetadata {
        let validators = self
            .active_validators
            .iter()
            .map(|validator| {
                let name = AuthorityName::from_bytes(&validator.protocol_pubkey_bytes).unwrap();
                (
                    name,
                    (
                        validator.voting_power,
                        NetworkMetadata {
                            network_address: Multiaddr::try_from(validator.net_address.clone())
                                .unwrap(),
                            narwhal_primary_address: Multiaddr::try_from(
                                validator.primary_address.clone(),
                            )
                            .unwrap(),
                            network_public_key: NetworkPublicKey::from_bytes(
                                &validator.network_pubkey_bytes,
                            )
                            .ok(),
                        },
                    ),
                )
            })
            .collect();
        CommitteeWithNetworkMetadata::new(self.epoch, validators)
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
    #[schemars(with = "Base64")]
    #[serde_as(as = "Base64")]
    pub protocol_pubkey_bytes: Vec<u8>,
    #[schemars(with = "Base64")]
    #[serde_as(as = "Base64")]
    pub network_pubkey_bytes: Vec<u8>,
    #[schemars(with = "Base64")]
    #[serde_as(as = "Base64")]
    pub worker_pubkey_bytes: Vec<u8>,
    #[schemars(with = "Base64")]
    #[serde_as(as = "Base64")]
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
    pub net_address: String,
    pub p2p_address: String,
    pub primary_address: String,
    pub worker_address: String,
    #[schemars(with = "Option<Base64>")]
    #[serde_as(as = "Option<Base64>")]
    pub next_epoch_protocol_pubkey_bytes: Option<Vec<u8>>,
    #[schemars(with = "Option<Base64>")]
    #[serde_as(as = "Option<Base64>")]
    pub next_epoch_proof_of_possession: Option<Vec<u8>>,
    #[schemars(with = "Option<Base64>")]
    #[serde_as(as = "Option<Base64>")]
    pub next_epoch_network_pubkey_bytes: Option<Vec<u8>>,
    #[schemars(with = "Option<Base64>")]
    #[serde_as(as = "Option<Base64>")]
    pub next_epoch_worker_pubkey_bytes: Option<Vec<u8>>,
    pub next_epoch_net_address: Option<String>,
    pub next_epoch_p2p_address: Option<String>,
    pub next_epoch_primary_address: Option<String>,
    pub next_epoch_worker_address: Option<String>,

    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub voting_power: u64,
    pub operation_cap_id: ObjectID,
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub gas_price: u64,
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub commission_rate: u64,
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub next_epoch_stake: u64,
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub next_epoch_gas_price: u64,
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub next_epoch_commission_rate: u64,

    // Staking pool information
    /// ID of the staking pool object.
    pub staking_pool_id: ObjectID,
    /// The epoch at which this pool became active.
    #[schemars(with = "Option<BigInt<u64>>")]
    #[serde_as(as = "Option<Readable<BigInt<u64>, _>>")]
    pub staking_pool_activation_epoch: Option<u64>,
    /// The epoch at which this staking pool ceased to be active. `None` = {pre-active, active},
    #[schemars(with = "Option<BigInt<u64>>")]
    #[serde_as(as = "Option<Readable<BigInt<u64>, _>>")]
    pub staking_pool_deactivation_epoch: Option<u64>,
    /// The total number of SUI tokens in this pool.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub staking_pool_sui_balance: u64,
    /// The epoch stake rewards will be added here at the end of each epoch.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub rewards_pool: u64,
    /// Total number of pool tokens issued by the pool.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub pool_token_balance: u64,
    /// Pending stake amount for this epoch.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub pending_stake: u64,
    /// Pending stake withdrawn during the current epoch, emptied at epoch boundaries.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub pending_total_sui_withdraw: u64,
    /// Pending pool token withdrawn during the current epoch, emptied at epoch boundaries.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub pending_pool_token_withdraw: u64,
    /// ID of the exchange rate table object.
    pub exchange_rates_id: ObjectID,
    /// Number of exchange rates in the table.
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "Readable<BigInt<u64>, _>")]
    pub exchange_rates_size: u64,
}

impl Default for SuiSystemStateSummary {
    fn default() -> Self {
        Self {
            epoch: 0,
            protocol_version: 1,
            system_state_version: 1,
            storage_fund_total_object_storage_rebates: 0,
            storage_fund_non_refundable_balance: 0,
            reference_gas_price: 1,
            safe_mode: false,
            safe_mode_storage_rewards: 0,
            safe_mode_computation_rewards: 0,
            safe_mode_storage_rebates: 0,
            safe_mode_non_refundable_storage_fee: 0,
            epoch_start_timestamp_ms: 0,
            epoch_duration_ms: 0,
            stake_subsidy_start_epoch: 0,
            max_validator_count: 0,
            min_validator_joining_stake: 0,
            validator_low_stake_threshold: 0,
            validator_very_low_stake_threshold: 0,
            validator_low_stake_grace_period: 0,
            stake_subsidy_balance: 0,
            stake_subsidy_distribution_counter: 0,
            stake_subsidy_current_distribution_amount: 0,
            stake_subsidy_period_length: 0,
            stake_subsidy_decrease_rate: 0,
            total_stake: 0,
            active_validators: vec![],
            pending_active_validators_id: ObjectID::ZERO,
            pending_active_validators_size: 0,
            pending_removals: vec![],
            staking_pool_mappings_id: ObjectID::ZERO,
            staking_pool_mappings_size: 0,
            inactive_pools_id: ObjectID::ZERO,
            inactive_pools_size: 0,
            validator_candidates_id: ObjectID::ZERO,
            validator_candidates_size: 0,
            at_risk_validators: vec![],
            validator_report_records: vec![],
        }
    }
}

impl Default for SuiValidatorSummary {
    fn default() -> Self {
        Self {
            sui_address: SuiAddress::default(),
            protocol_pubkey_bytes: vec![],
            network_pubkey_bytes: vec![],
            worker_pubkey_bytes: vec![],
            proof_of_possession_bytes: vec![],
            name: String::new(),
            description: String::new(),
            image_url: String::new(),
            project_url: String::new(),
            net_address: String::new(),
            p2p_address: String::new(),
            primary_address: String::new(),
            worker_address: String::new(),
            next_epoch_protocol_pubkey_bytes: None,
            next_epoch_proof_of_possession: None,
            next_epoch_network_pubkey_bytes: None,
            next_epoch_worker_pubkey_bytes: None,
            next_epoch_net_address: None,
            next_epoch_p2p_address: None,
            next_epoch_primary_address: None,
            next_epoch_worker_address: None,
            voting_power: 0,
            operation_cap_id: ObjectID::ZERO,
            gas_price: 0,
            commission_rate: 0,
            next_epoch_stake: 0,
            next_epoch_gas_price: 0,
            next_epoch_commission_rate: 0,
            staking_pool_id: ObjectID::ZERO,
            staking_pool_activation_epoch: None,
            staking_pool_deactivation_epoch: None,
            staking_pool_sui_balance: 0,
            rewards_pool: 0,
            pool_token_balance: 0,
            pending_stake: 0,
            pending_total_sui_withdraw: 0,
            pending_pool_token_withdraw: 0,
            exchange_rates_id: ObjectID::ZERO,
            exchange_rates_size: 0,
        }
    }
}

/// Given the staking pool id of a validator, return the validator's `SuiValidatorSummary`,
/// works for validator candidates, active validators, as well as inactive validators.
pub fn get_validator_by_pool_id<S>(
    object_store: &S,
    system_state: &SuiSystemState,
    system_state_summary: &SuiSystemStateSummary,
    pool_id: ObjectID,
) -> Result<SuiValidatorSummary, SuiError>
where
    S: ObjectStore + ?Sized,
{
    // First try to find in active validator set.
    let active_validator = system_state_summary
        .active_validators
        .iter()
        .find(|v| v.staking_pool_id == pool_id);
    if let Some(active) = active_validator {
        return Ok(active.clone());
    }
    // Then try to find in pending active validator set.
    let pending_active_validators = system_state.get_pending_active_validators(object_store)?;
    let pending_active = pending_active_validators
        .iter()
        .find(|v| v.staking_pool_id == pool_id);
    if let Some(pending) = pending_active {
        return Ok(pending.clone());
    }
    // After that try to find in inactive pools.
    let inactive_table_id = system_state_summary.inactive_pools_id;
    if let Ok(inactive) =
        get_validator_from_table(&object_store, inactive_table_id, &ID::new(pool_id))
    {
        return Ok(inactive);
    }
    // Finally look up the candidates pool.
    let candidate_address: SuiAddress = get_dynamic_field_from_store(
        &object_store,
        system_state_summary.staking_pool_mappings_id,
        &ID::new(pool_id),
    )
    .map_err(|err| {
        SuiError::SuiSystemStateReadError(format!(
            "Failed to load candidate address from pool mappings: {:?}",
            err
        ))
    })?;
    let candidate_table_id = system_state_summary.validator_candidates_id;
    get_validator_from_table(&object_store, candidate_table_id, &candidate_address)
}
