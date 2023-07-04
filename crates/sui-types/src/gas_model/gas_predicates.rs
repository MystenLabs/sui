// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//
// Predicates and utility functions based on gas versions.
//

use crate::gas_model::tables::{
    initial_cost_schedule_v1, initial_cost_schedule_v2, initial_cost_schedule_v3,
    initial_cost_schedule_v4,
};
use crate::gas_model::units_types::CostTable;
use sui_protocol_config::ProtocolConfig;

// If true, do not charge the entire budget on storage OOG
pub fn dont_charge_budget_on_storage_oog(gas_model_version: u64) -> bool {
    gas_model_version >= 4
}

// If true, enable the check for gas price too high
pub fn gas_price_too_high(gas_model_version: u64) -> bool {
    gas_model_version >= 4
}

// If true, input object bytes are treated as memory allocated in Move and
// charged according to the bucket they end up in.
pub fn charge_input_as_memory(gas_model_version: u64) -> bool {
    gas_model_version == 4
}

// If true, use the value of txn_base_cost as a multiplier of transaction gas price
// to determine the minimum cost of a transaction.
pub fn txn_base_cost_as_multiplier(protocol_config: &ProtocolConfig) -> bool {
    protocol_config.txn_base_cost_as_multiplier()
}

// Return the version supported cost table
pub fn cost_table_for_version(gas_model: u64) -> CostTable {
    if gas_model <= 3 {
        initial_cost_schedule_v1()
    } else if gas_model == 4 {
        initial_cost_schedule_v2()
    } else if gas_model == 5 {
        initial_cost_schedule_v3()
    } else {
        initial_cost_schedule_v4()
    }
}
