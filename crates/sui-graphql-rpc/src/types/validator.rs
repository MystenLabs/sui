// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::address::Address;
use super::big_int::BigInt;
// use super::sui_address::SuiAddress;
use super::validator_credentials::ValidatorCredentials;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct Validator {
    pub address: Address,
    pub credentials: Option<ValidatorCredentials>,
    pub next_epoch_credentials: Option<ValidatorCredentials>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub project_url: Option<String>,
    // operationCap: Option<MoveObject>,
    // stakingPool: Option<MoveObject>,
    // exchangeRates: Option<MoveObject>,
    pub exchange_rates_size: Option<u64>,
    pub staking_pool_activation_epoch: Option<u64>,
    pub staking_pool_sui_balance: Option<BigInt>,
    pub rewards_pool: Option<BigInt>,
    pub pool_token_balance: Option<BigInt>,
    pub pending_stake: Option<BigInt>,
    pub pending_total_sui_withdraw: Option<BigInt>,
    pub pending_pool_token_withdraw: Option<BigInt>,
    pub voting_power: Option<u64>,
    // pub stake_units: Option<u64>,
    pub gas_price: Option<BigInt>,
    pub commission_rate: Option<u64>,
    pub next_epoch_stake: Option<BigInt>,
    pub next_epoch_gas_price: Option<BigInt>,
    pub next_epoch_commission_rate: Option<u64>,
    // pub at_risk: Option<u64>,
    // pub report_records: Option<Vec<SuiAddress>>,
    // pub apy: Option<u64>,
}
