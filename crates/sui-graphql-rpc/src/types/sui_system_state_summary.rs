// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::db_data_provider::PgManager;

use super::big_int::BigInt;
use super::date_time::DateTime;
use super::epoch::Epoch;
use super::protocol_config::ProtocolConfigs;
use super::safe_mode::SafeMode;
use super::stake_subsidy::StakeSubsidy;
use super::storage_fund::StorageFund;
use super::system_parameters::SystemParameters;
use super::validator_set::ValidatorSet;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct SuiSystemStateSummary {
    #[graphql(skip)]
    pub epoch_id: u64,
    pub system_state_version: Option<BigInt>,
    #[graphql(skip)]
    pub protocol_version: u64,
    pub reference_gas_price: Option<BigInt>,
    pub system_parameters: Option<SystemParameters>,
    pub stake_subsidy: Option<StakeSubsidy>,
    pub validator_set: Option<ValidatorSet>,
    pub storage_fund: Option<StorageFund>,
    pub safe_mode: Option<SafeMode>,
    pub start_timestamp: Option<DateTime>,
}

#[ComplexObject]
impl SuiSystemStateSummary {
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let epoch = ctx
            .data_unchecked::<PgManager>()
            .fetch_epoch_strict(self.epoch_id)
            .await
            .extend()?;

        Ok(Some(epoch))
    }

    async fn protocol_configs(&self, ctx: &Context<'_>) -> Result<Option<ProtocolConfigs>> {
        Ok(Some(
            ctx.data_unchecked::<PgManager>()
                .fetch_protocol_configs(Some(self.protocol_version))
                .await?,
        ))
    }
}
