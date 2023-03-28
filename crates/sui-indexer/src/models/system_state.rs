// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use diesel::{Insertable, Queryable};

use sui_types::base_types::{EpochId, ObjectID, SuiAddress};
use sui_types::sui_system_state::sui_system_state_summary::{
    SuiSystemStateSummary, SuiValidatorSummary,
};

use crate::errors::IndexerError;
use crate::schema::{at_risk_validators, system_states, validators};

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = system_states)]
pub struct DBSystemStateSummary {
    pub epoch: i64,
    pub protocol_version: i64,
    pub system_state_version: i64,
    pub storage_fund: i64,
    pub reference_gas_price: i64,
    pub safe_mode: bool,
    pub epoch_start_timestamp_ms: i64,
    pub epoch_duration_ms: i64,
    pub stake_subsidy_start_epoch: i64,
    pub stake_subsidy_epoch_counter: i64,
    pub stake_subsidy_balance: i64,
    pub stake_subsidy_current_epoch_amount: i64,
    pub total_stake: i64,
    pub pending_active_validators_id: String,
    pub pending_active_validators_size: i64,
    pub pending_removals: Vec<i64>,
    pub staking_pool_mappings_id: String,
    pub staking_pool_mappings_size: i64,
    pub inactive_pools_id: String,
    pub inactive_pools_size: i64,
    pub validator_candidates_id: String,
    pub validator_candidates_size: i64,
}

impl From<SuiSystemStateSummary> for DBSystemStateSummary {
    fn from(s: SuiSystemStateSummary) -> Self {
        Self {
            epoch: s.epoch as i64,
            protocol_version: s.protocol_version as i64,
            system_state_version: s.system_state_version as i64,
            storage_fund: (s.storage_fund_non_refundable_balance
                + s.storage_fund_total_object_storage_rebates) as i64,
            reference_gas_price: s.reference_gas_price as i64,
            safe_mode: s.safe_mode,
            epoch_start_timestamp_ms: s.epoch_start_timestamp_ms as i64,
            stake_subsidy_start_epoch: s.stake_subsidy_start_epoch as i64,
            epoch_duration_ms: s.epoch_duration_ms as i64,
            stake_subsidy_epoch_counter: s.stake_subsidy_distribution_counter as i64,
            stake_subsidy_balance: s.stake_subsidy_balance as i64,
            stake_subsidy_current_epoch_amount: s.stake_subsidy_current_distribution_amount as i64,
            total_stake: s.total_stake as i64,
            pending_active_validators_id: s.pending_active_validators_id.to_string(),
            pending_active_validators_size: s.pending_active_validators_size as i64,
            pending_removals: s.pending_removals.iter().map(|i| *i as i64).collect(),
            staking_pool_mappings_id: s.staking_pool_mappings_id.to_string(),
            staking_pool_mappings_size: s.staking_pool_mappings_size as i64,
            inactive_pools_id: s.inactive_pools_id.to_string(),
            inactive_pools_size: s.inactive_pools_size as i64,
            validator_candidates_id: s.validator_candidates_id.to_string(),
            validator_candidates_size: s.validator_candidates_size as i64,
        }
    }
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = validators)]
pub struct DBValidatorSummary {
    pub epoch: i64,
    pub sui_address: String,
    pub protocol_pubkey_bytes: Vec<u8>,
    pub network_pubkey_bytes: Vec<u8>,
    pub worker_pubkey_bytes: Vec<u8>,
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
    pub net_address: String,
    pub p2p_address: String,
    pub primary_address: String,
    pub worker_address: String,
    pub next_epoch_protocol_pubkey_bytes: Option<Vec<u8>>,
    pub next_epoch_proof_of_possession: Option<Vec<u8>>,
    pub next_epoch_network_pubkey_bytes: Option<Vec<u8>>,
    pub next_epoch_worker_pubkey_bytes: Option<Vec<u8>>,
    pub next_epoch_net_address: Option<String>,
    pub next_epoch_p2p_address: Option<String>,
    pub next_epoch_primary_address: Option<String>,
    pub next_epoch_worker_address: Option<String>,
    pub voting_power: i64,
    pub operation_cap_id: String,
    pub gas_price: i64,
    pub commission_rate: i64,
    pub next_epoch_stake: i64,
    pub next_epoch_gas_price: i64,
    pub next_epoch_commission_rate: i64,
    pub staking_pool_id: String,
    pub staking_pool_activation_epoch: Option<i64>,
    pub staking_pool_deactivation_epoch: Option<i64>,
    pub staking_pool_sui_balance: i64,
    pub rewards_pool: i64,
    pub pool_token_balance: i64,
    pub pending_stake: i64,
    pub pending_total_sui_withdraw: i64,
    pub pending_pool_token_withdraw: i64,
    pub exchange_rates_id: String,
    pub exchange_rates_size: i64,
}

impl From<(EpochId, SuiValidatorSummary)> for DBValidatorSummary {
    fn from((epoch, v): (EpochId, SuiValidatorSummary)) -> Self {
        Self {
            epoch: epoch as i64,
            sui_address: v.sui_address.to_string(),
            protocol_pubkey_bytes: v.protocol_pubkey_bytes,
            network_pubkey_bytes: v.network_pubkey_bytes,
            worker_pubkey_bytes: v.worker_pubkey_bytes,
            proof_of_possession_bytes: v.proof_of_possession_bytes,
            name: v.name,
            description: v.description,
            image_url: v.image_url,
            project_url: v.project_url,
            net_address: v.net_address,
            p2p_address: v.p2p_address,
            primary_address: v.primary_address,
            worker_address: v.worker_address,
            next_epoch_protocol_pubkey_bytes: v.next_epoch_protocol_pubkey_bytes,
            next_epoch_proof_of_possession: v.next_epoch_proof_of_possession,
            next_epoch_network_pubkey_bytes: v.next_epoch_network_pubkey_bytes,
            next_epoch_worker_pubkey_bytes: v.next_epoch_worker_pubkey_bytes,
            next_epoch_net_address: v.next_epoch_net_address,
            next_epoch_p2p_address: v.next_epoch_p2p_address,
            next_epoch_primary_address: v.next_epoch_primary_address,
            next_epoch_worker_address: v.next_epoch_worker_address,
            voting_power: v.voting_power as i64,
            operation_cap_id: v.operation_cap_id.to_string(),
            gas_price: v.gas_price as i64,
            commission_rate: v.commission_rate as i64,
            next_epoch_stake: v.next_epoch_stake as i64,
            next_epoch_gas_price: v.next_epoch_gas_price as i64,
            next_epoch_commission_rate: v.next_epoch_commission_rate as i64,
            staking_pool_id: v.staking_pool_id.to_string(),
            staking_pool_activation_epoch: v.staking_pool_activation_epoch.map(|v| v as i64),
            staking_pool_deactivation_epoch: v.staking_pool_deactivation_epoch.map(|v| v as i64),
            staking_pool_sui_balance: v.staking_pool_sui_balance as i64,
            rewards_pool: v.rewards_pool as i64,
            pool_token_balance: v.pool_token_balance as i64,
            pending_stake: v.pending_stake as i64,
            pending_total_sui_withdraw: v.pending_total_sui_withdraw as i64,
            pending_pool_token_withdraw: v.pending_pool_token_withdraw as i64,
            exchange_rates_id: v.exchange_rates_id.to_string(),
            exchange_rates_size: v.exchange_rates_size as i64,
        }
    }
}

impl TryFrom<DBValidatorSummary> for SuiValidatorSummary {
    type Error = IndexerError;
    fn try_from(db: DBValidatorSummary) -> Result<SuiValidatorSummary, Self::Error> {
        Ok(SuiValidatorSummary {
            sui_address: SuiAddress::from_str(&db.sui_address)?,
            protocol_pubkey_bytes: db.protocol_pubkey_bytes,
            network_pubkey_bytes: db.network_pubkey_bytes,
            worker_pubkey_bytes: db.worker_pubkey_bytes,
            proof_of_possession_bytes: db.proof_of_possession_bytes,
            name: db.name,
            description: db.description,
            image_url: db.image_url,
            project_url: db.project_url,
            net_address: db.net_address,
            p2p_address: db.p2p_address,
            primary_address: db.primary_address,
            worker_address: db.worker_address,
            next_epoch_protocol_pubkey_bytes: db.next_epoch_protocol_pubkey_bytes,
            next_epoch_proof_of_possession: db.next_epoch_proof_of_possession,
            next_epoch_network_pubkey_bytes: db.next_epoch_network_pubkey_bytes,
            next_epoch_worker_pubkey_bytes: db.next_epoch_worker_pubkey_bytes,
            next_epoch_net_address: db.next_epoch_net_address,
            next_epoch_p2p_address: db.next_epoch_p2p_address,
            next_epoch_primary_address: db.next_epoch_primary_address,
            next_epoch_worker_address: db.next_epoch_worker_address,
            voting_power: db.voting_power as u64,
            operation_cap_id: ObjectID::from_str(&db.operation_cap_id)?,
            gas_price: db.gas_price as u64,
            commission_rate: db.commission_rate as u64,
            next_epoch_stake: db.next_epoch_stake as u64,
            next_epoch_gas_price: db.next_epoch_gas_price as u64,
            next_epoch_commission_rate: db.next_epoch_commission_rate as u64,
            staking_pool_id: ObjectID::from_str(&db.staking_pool_id)?,
            staking_pool_activation_epoch: db.staking_pool_activation_epoch.map(|i| i as u64),
            staking_pool_deactivation_epoch: db.staking_pool_deactivation_epoch.map(|i| i as u64),
            staking_pool_sui_balance: db.staking_pool_sui_balance as u64,
            rewards_pool: db.rewards_pool as u64,
            pool_token_balance: db.pool_token_balance as u64,
            pending_stake: db.pending_stake as u64,
            pending_total_sui_withdraw: db.pending_total_sui_withdraw as u64,
            pending_pool_token_withdraw: db.pending_pool_token_withdraw as u64,
            exchange_rates_id: ObjectID::from_str(&db.exchange_rates_id)?,
            exchange_rates_size: db.exchange_rates_size as u64,
        })
    }
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = at_risk_validators)]
pub struct DBAtRiskValidator {
    pub epoch: i64,
    pub address: String,
    pub epoch_count: i64,
    pub reported_by: Vec<String>,
}
