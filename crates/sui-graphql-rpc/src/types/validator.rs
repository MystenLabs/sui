// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::address::Address;
use super::big_int::BigInt;
use super::sui_address::SuiAddress;
// use super::sui_address::SuiAddress;
use super::validator_credentials::ValidatorCredentials;
use async_graphql::*;
use sui_sdk::types::sui_system_state::sui_system_state_summary::SuiValidatorSummary;

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

impl From<SuiValidatorSummary> for Validator {
    fn from(v: SuiValidatorSummary) -> Self {
        let credentials = ValidatorCredentials::from(&v);
        Validator {
            address: Address {
                address: SuiAddress::from(v.sui_address),
            },
            next_epoch_credentials: Some(credentials.clone()),
            credentials: Some(credentials),
            name: Some(v.name),
            description: Some(v.description),
            image_url: Some(v.image_url),
            project_url: Some(v.project_url),
            exchange_rates_size: Some(v.exchange_rates_size),

            staking_pool_activation_epoch: Some(v.staking_pool_activation_epoch.unwrap()),
            staking_pool_sui_balance: Some(BigInt::from(v.staking_pool_sui_balance)),
            rewards_pool: Some(BigInt::from(v.rewards_pool)),
            pool_token_balance: Some(BigInt::from(v.pool_token_balance)),
            pending_stake: Some(BigInt::from(v.pending_stake)),
            pending_total_sui_withdraw: Some(BigInt::from(v.pending_total_sui_withdraw)),
            pending_pool_token_withdraw: Some(BigInt::from(v.pending_pool_token_withdraw)),
            voting_power: Some(v.voting_power),
            // stake_units: todo!(),
            gas_price: Some(BigInt::from(v.gas_price)),
            commission_rate: Some(v.commission_rate),
            next_epoch_stake: Some(BigInt::from(v.next_epoch_stake)),
            next_epoch_gas_price: Some(BigInt::from(v.next_epoch_gas_price)),
            next_epoch_commission_rate: Some(v.next_epoch_commission_rate),
            // at_risk: todo!(),
            // report_records: todo!(),
            // apy: todo!(),
        }
    }
}
