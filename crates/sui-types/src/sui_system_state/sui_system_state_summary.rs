// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::base_types::SuiAddress;

/// This is the JSON-RPC type for the SUI system state object.
/// It flatterns all fields to make them top-level fields such that it as minimum
/// dependencies to the internal data structures of the SUI system state type.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct SuiSystemStateSummary {
    pub epoch: u64,
    pub protocol_version: u64,
    pub storage_fund: u64,
    pub reference_gas_price: u64,
    pub safe_mode: bool,
    pub epoch_start_timestamp_ms: u64,

    // System parameters
    pub min_validator_stake: u64,
    pub max_validator_candidate_count: u64,
    pub governance_start_epoch: u64,

    // Stake subsidy information
    pub stake_subsidy_epoch_counter: u64,
    pub stake_subsidy_balance: u64,
    pub stake_subsidy_current_epoch_amount: u64,

    // Validator set
    pub total_stake: u64,
    pub active_validators: Vec<SuiValidatorSummary>,
}

/// This is the JSON-RPC type for the SUI validator. It flattens all inner strucutures
/// to top-level fields so that they are decoupled from the internal definitions.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct SuiValidatorSummary {
    // Metadata
    pub sui_address: SuiAddress,
    pub protocol_pubkey_bytes: Vec<u8>,
    pub network_pubkey_bytes: Vec<u8>,
    pub worker_pubkey_bytes: Vec<u8>,
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
    pub net_address: Vec<u8>,
    pub p2p_address: Vec<u8>,
    pub primary_address: Vec<u8>,
    pub worker_address: Vec<u8>,
    pub next_epoch_protocol_pubkey_bytes: Option<Vec<u8>>,
    pub next_epoch_proof_of_possession: Option<Vec<u8>>,
    pub next_epoch_network_pubkey_bytes: Option<Vec<u8>>,
    pub next_epoch_worker_pubkey_bytes: Option<Vec<u8>>,
    pub next_epoch_net_address: Option<Vec<u8>>,
    pub next_epoch_p2p_address: Option<Vec<u8>>,
    pub next_epoch_primary_address: Option<Vec<u8>>,
    pub next_epoch_worker_address: Option<Vec<u8>>,

    pub voting_power: u64,
    pub gas_price: u64,
    pub commission_rate: u64,
    pub next_epoch_stake: u64,
    pub next_epoch_gas_price: u64,
    pub next_epoch_commission_rate: u64,

    // Staking pool information
    pub staking_pool_starting_epoch: u64,
    pub staking_pool_deactivation_epoch: Option<u64>,
    pub staking_pool_sui_balance: u64,
    pub rewards_pool: u64,
    pub pool_token_balance: u64,
    pub pending_delegation: u64,
    pub pending_total_sui_withdraw: u64,
    pub pending_pool_token_withdraw: u64,
}
