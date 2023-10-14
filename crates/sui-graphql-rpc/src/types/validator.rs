// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::db_data_provider::PgManager;

use super::address::Address;
use super::big_int::BigInt;
use super::move_object::MoveObject;
use super::sui_address::SuiAddress;
use super::validator_credentials::ValidatorCredentials;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct Validator {
    pub address: Address,
    pub credentials: Option<ValidatorCredentials>,
    pub next_epoch_credentials: Option<ValidatorCredentials>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub project_url: Option<String>,
    #[graphql(skip)]
    pub operation_cap_id: SuiAddress,
    #[graphql(skip)]
    pub staking_pool_id: SuiAddress,
    #[graphql(skip)]
    pub exchange_rates_id: SuiAddress,
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
    pub at_risk: Option<u64>, // only available on sui_system_state_summary
    pub report_records: Option<Vec<SuiAddress>>, // only available on sui_system_state_summary
                              // pub apy: Option<u64>, // TODO: Defer for StakedSui implementation
}

#[ComplexObject]
impl Validator {
    async fn operation_cap(&self, ctx: &Context<'_>) -> Result<Option<MoveObject>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_move_obj(self.operation_cap_id, None)
            .await
            .extend()
    }

    async fn staking_pool(&self, ctx: &Context<'_>) -> Result<Option<MoveObject>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_move_obj(self.operation_cap_id, None)
            .await
            .extend()
    }

    async fn exchange_rates(&self, ctx: &Context<'_>) -> Result<Option<MoveObject>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_move_obj(self.operation_cap_id, None)
            .await
            .extend()
    }
}
