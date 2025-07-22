// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    move_package::{self, CSysPackage, MovePackage},
    object::{self, Object},
    protocol_configs::ProtocolConfigs,
};
use crate::api::types::validator_set::ValidatorSet;
use crate::{
    api::scalars::{big_int::BigInt, date_time::DateTime, uint53::UInt53},
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};
use anyhow::Context as _;
use async_graphql::{connection::Connection, dataloader::DataLoader, Context, Error, Object};
use futures::try_join;
use std::sync::Arc;
use sui_indexer_alt_reader::{
    epochs::{CheckpointBoundedEpochStartKey, EpochEndKey, EpochStartKey},
    pg_reader::PgReader,
};
use sui_indexer_alt_schema::epochs::{StoredEpochEnd, StoredEpochStart};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::SUI_DENY_LIST_OBJECT_ID;
use tokio::sync::OnceCell;

pub(crate) struct Epoch {
    pub(crate) epoch_id: u64,
    scope: Scope,
    start: OnceCell<Option<StoredEpochStart>>,
    end: OnceCell<Option<StoredEpochEnd>>,
}

/// Activity on Sui is partitioned in time, into epochs.
///
/// Epoch changes are opportunities for the network to reconfigure itself (perform protocol or system package upgrades, or change the committee) and distribute staking rewards. The network aims to keep epochs roughly the same duration as each other.
///
/// During a particular epoch the following data is fixed:
///
/// - protocol version,
/// - reference gas price,
/// - system package versions,
/// - validators in the committee.
#[Object]
impl Epoch {
    /// The epoch's id as a sequence number that starts at 0 and is incremented by one at every epoch change.
    async fn epoch_id(&self) -> UInt53 {
        self.epoch_id.into()
    }

    /// State of the Coin DenyList object (0x403) at the start of this epoch.
    ///
    /// The DenyList controls access to Regulated Coins. Writes to the DenyList are accumulated and only take effect on the next epoch boundary. Consequently, it's possible to determine the state of the DenyList for a transaction by reading it at the start of the epoch the transaction is in.
    async fn coin_deny_list(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Object>, RpcError<object::Error>> {
        let Some(start) = self.start(ctx).await? else {
            return Ok(None);
        };

        Object::checkpoint_bounded(
            ctx,
            self.scope.clone(),
            SUI_DENY_LIST_OBJECT_ID.into(),
            (start.cp_lo as u64).saturating_sub(1).into(),
        )
        .await
    }

    /// The epoch's corresponding protocol configuration, including the feature flags and the configuration options.
    async fn protocol_configs(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<ProtocolConfigs>, RpcError> {
        let Some(start) = self.start(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(ProtocolConfigs::with_protocol_version(
            start.protocol_version as u64,
        )))
    }

    /// The minimum gas price that a quorum of validators are guaranteed to sign a transaction for in this epoch.
    async fn reference_gas_price(&self, ctx: &Context<'_>) -> Result<Option<BigInt>, RpcError> {
        let Some(start) = self.start(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(BigInt::from(start.reference_gas_price)))
    }

    /// The timestamp associated with the first checkpoint in the epoch.
    async fn start_timestamp(&self, ctx: &Context<'_>) -> Result<Option<DateTime>, RpcError> {
        let Some(contents) = self.start(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(DateTime::from_ms(contents.start_timestamp_ms)?))
    }

    /// The system packages used by all transactions in this epoch.
    async fn system_packages(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CSysPackage>,
        last: Option<u64>,
        before: Option<CSysPackage>,
    ) -> Result<Option<Connection<String, MovePackage>>, RpcError<move_package::Error>> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Epoch", "systemPackages");
        let page = Page::from_params(limits, first, after, last, before)?;

        let Some(contents) = self.start(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(
            MovePackage::paginate_system_packages(
                ctx,
                self.scope.clone(),
                page,
                contents.cp_lo as u64,
            )
            .await?,
        ))
    }

    /// The timestamp associated with the last checkpoint in the epoch (or `null` if the epoch has not finished yet).
    async fn end_timestamp(&self, ctx: &Context<'_>) -> Result<Option<DateTime>, RpcError> {
        let Some(end) = self.end(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(DateTime::from_ms(end.end_timestamp_ms)?))
    }

    /// Validator-related properties, including the active validators.
    async fn validator_set(&self, ctx: &Context<'_>) -> Result<Option<ValidatorSet>, RpcError> {
        let Some(start) = self.start(ctx).await? else {
            return Ok(None);
        };

        let validator_set: ValidatorSet =
            match bcs::from_bytes::<SuiSystemState>(&start.system_state) {
                Ok(SuiSystemState::V1(inner)) => inner.validators.into(),
                Ok(SuiSystemState::V2(inner)) => inner.validators.into(),
                #[cfg(msim)]
                Ok(SuiSystemState::SimTestV1(_))
                | Ok(SuiSystemState::SimTestShallowV2(_))
                | Ok(SuiSystemState::SimTestDeepV2(_)) => return Ok(None),
                Err(e) => return Err(RpcError::InternalError(Arc::new(e.into()))),
            };

        Ok(Some(validator_set))
    }

    /// The total number of checkpoints in this epoch.
    async fn total_checkpoints(&self, ctx: &Context<'_>) -> Result<Option<UInt53>, RpcError> {
        let (Some(start), end) = try_join!(self.start(ctx), self.end(ctx))? else {
            return Ok(None);
        };

        let lo = start.cp_lo as u64;
        let hi = end.as_ref().map_or_else(
            || self.scope.checkpoint_viewed_at_exclusive_bound(),
            |end| end.cp_hi as u64,
        );

        Ok(Some(UInt53::from(hi - lo)))
    }

    /// The total amount of gas fees (in MIST) that were paid in this epoch (or `null` if the epoch has not finished yet).
    async fn total_gas_fees(&self, ctx: &Context<'_>) -> Result<Option<BigInt>, RpcError> {
        let Some(StoredEpochEnd { total_gas_fees, .. }) = self.end(ctx).await? else {
            return Ok(None);
        };

        Ok(total_gas_fees.map(BigInt::from))
    }

    /// The total MIST rewarded as stake (or `null` if the epoch has not finished yet).
    async fn total_stake_rewards(&self, ctx: &Context<'_>) -> Result<Option<BigInt>, RpcError> {
        let Some(StoredEpochEnd {
            total_stake_rewards_distributed,
            ..
        }) = self.end(ctx).await?
        else {
            return Ok(None);
        };

        Ok(total_stake_rewards_distributed.map(BigInt::from))
    }

    /// The amount added to total gas fees to make up the total stake rewards.
    async fn total_stake_subsidies(&self, ctx: &Context<'_>) -> Result<Option<BigInt>, RpcError> {
        let Some(StoredEpochEnd {
            stake_subsidy_amount,
            ..
        }) = self.end(ctx).await?
        else {
            return Ok(None);
        };

        Ok(stake_subsidy_amount.map(BigInt::from))
    }
}

impl Epoch {
    /// Construct an epoch that is represented by just its identifier (its sequence number). This
    /// does not check whether the epoch exists, so should not be used to "fetch" an epoch based on
    /// an ID provided as user input.
    pub(crate) fn with_id(scope: Scope, epoch_id: u64) -> Self {
        Self {
            epoch_id,
            scope,
            start: OnceCell::new(),
            end: OnceCell::new(),
        }
    }

    /// Load the epoch from the store, and return it fully inflated (with contents already
    /// fetched). If `epoch_id` is provided, the epoch with that ID is loaded. Otherwise, the
    /// latest epoch for the current checkpoint is loaded.
    ///
    /// Returns `None` if the epoch does not exist (or started after the checkpoint being viewed).
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        scope: Scope,
        epoch_id: Option<UInt53>,
    ) -> Result<Option<Self>, RpcError> {
        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

        let stored = match epoch_id {
            Some(id) => pg_loader
                .load_one(EpochStartKey(id.into()))
                .await
                .context("Failed to fetch epoch start information by start key")?,
            None => pg_loader
                .load_one(CheckpointBoundedEpochStartKey(scope.checkpoint_viewed_at()))
                .await
                .context(
                    "Failed to fetch epoch start information by checkpoint bounded start key",
                )?,
        }
        .filter(|start| start.cp_lo as u64 <= scope.checkpoint_viewed_at());

        Ok(stored.map(|start| Self {
            epoch_id: start.epoch as u64,
            scope: scope.clone(),
            start: OnceCell::from(Some(start)),
            end: OnceCell::new(),
        }))
    }

    async fn start(&self, ctx: &Context<'_>) -> Result<&Option<StoredEpochStart>, Error> {
        self.start
            .get_or_try_init(async || {
                let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

                let stored = pg_loader
                    .load_one(EpochStartKey(self.epoch_id))
                    .await
                    .context("Failed to fetch epoch start information")?
                    .filter(|start| start.cp_lo as u64 <= self.scope.checkpoint_viewed_at());

                Ok(stored)
            })
            .await
    }

    /// Attempt to fetch information about the end of an epoch from the store. May return an empty
    /// response if the epoch has not ended yet, as of the checkpoint being viewed.
    async fn end(&self, ctx: &Context<'_>) -> Result<&Option<StoredEpochEnd>, Error> {
        self.end
            .get_or_try_init(async || {
                let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

                let stored = pg_loader
                    .load_one(EpochEndKey(self.epoch_id))
                    .await
                    .context("Failed to fetch epoch end information")?
                    .filter(|end| {
                        end.cp_hi as u64 <= self.scope.checkpoint_viewed_at_exclusive_bound()
                    });

                Ok(stored)
            })
            .await
    }
}
