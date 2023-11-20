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

/// Aspects that affect the running of the system that are managed by the validators either
/// directly, or through system transactions.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct SuiSystemStateSummary {
    #[graphql(skip)]
    pub epoch_id: u64,

    /// The value of the `version` field of `0x5`, the `0x3::sui::SuiSystemState` object.  This
    /// version changes whenever the fields contained in the system state object (held in a dynamic
    /// field attached to `0x5`) change.
    pub system_state_version: Option<BigInt>,

    #[graphql(skip)]
    pub protocol_version: u64,

    /// The minimum gas price that a quorum of validators are guaranteed to sign a transaction for.
    pub reference_gas_price: Option<BigInt>,

    /// Details of the system that are decided during genesis.
    pub system_parameters: Option<SystemParameters>,

    /// Parameters related to subsiding staking rewards
    pub stake_subsidy: Option<StakeSubsidy>,

    /// Details of the currently active validators and pending changes to that set.
    pub validator_set: Option<ValidatorSet>,

    /// SUI set aside to account for objects stored on-chain, at the start of the epoch.
    pub storage_fund: Option<StorageFund>,

    /// Information about whether last epoch change used safe mode, which happens if the full epoch
    /// change logic fails for some reason.
    pub safe_mode: Option<SafeMode>,

    /// The start of the current epoch.
    pub start_timestamp: Option<DateTime>,
}

#[ComplexObject]
impl SuiSystemStateSummary {
    /// The epoch for which this is the system state.
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let epoch = ctx
            .data_unchecked::<PgManager>()
            .fetch_epoch_strict(self.epoch_id)
            .await
            .extend()?;

        Ok(Some(epoch))
    }

    /// Configuration for how the chain operates that can change from epoch to epoch (due to a
    /// protocol version upgrade).
    async fn protocol_configs(&self, ctx: &Context<'_>) -> Result<Option<ProtocolConfigs>> {
        Ok(Some(
            ctx.data_unchecked::<PgManager>()
                .fetch_protocol_configs(Some(self.protocol_version))
                .await
                .extend()?,
        ))
    }
}
