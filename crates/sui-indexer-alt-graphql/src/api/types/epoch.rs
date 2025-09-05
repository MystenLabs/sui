// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    checkpoint::{filter::CheckpointFilter, CCheckpoint, Checkpoint},
    move_package::{self, CSysPackage, MovePackage},
    object::{self, Object},
    protocol_configs::ProtocolConfigs,
    transaction::{filter::TransactionFilter, CTransaction, Transaction},
};
use crate::api::types::safe_mode::{from_system_state, SafeMode};
use crate::api::types::stake_subsidy::{from_stake_subsidy_v1, StakeSubsidy};
use crate::api::types::storage_fund::StorageFund;
use crate::api::types::system_parameters::{
    from_system_parameters_v1, from_system_parameters_v2, SystemParameters,
};
use crate::{
    api::scalars::{big_int::BigInt, date_time::DateTime, uint53::UInt53},
    api::types::validator_set::ValidatorSet,
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};
use anyhow::Context as _;
use async_graphql::{connection::Connection, dataloader::DataLoader, Context, Error, Object};
use fastcrypto::encoding::{Base58, Encoding};
use futures::try_join;
use std::sync::Arc;
use sui_indexer_alt_reader::cp_sequence_numbers::CpSequenceNumberKey;
use sui_indexer_alt_reader::{
    epochs::{CheckpointBoundedEpochStartKey, EpochEndKey, EpochStartKey},
    pg_reader::PgReader,
};
use sui_indexer_alt_schema::cp_sequence_numbers::StoredCpSequenceNumbers;
use sui_indexer_alt_schema::epochs::{StoredEpochEnd, StoredEpochStart};
use sui_types::messages_checkpoint::CheckpointCommitment;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::SUI_DENY_LIST_OBJECT_ID;
use tokio::sync::OnceCell;

pub(crate) struct Epoch {
    pub(crate) epoch_id: u64,
    scope: Scope,
    start: OnceCell<Option<StoredEpochStart>>,
    end: OnceCell<Option<StoredEpochEnd>>,
    cp_sequence_numbers: OnceCell<Option<StoredCpSequenceNumbers>>,
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

    /// The epoch's corresponding checkpoints.
    async fn checkpoints(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CCheckpoint>,
        last: Option<u64>,
        before: Option<CCheckpoint>,
        filter: Option<CheckpointFilter>,
    ) -> Result<Option<Connection<String, Checkpoint>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Epoch", "checkpoints");
        let page = Page::from_params(limits, first, after, last, before)?;

        let Some(filter) = filter.unwrap_or_default().intersect(CheckpointFilter {
            at_epoch: Some(self.epoch_id.into()),
            ..Default::default()
        }) else {
            return Ok(Some(Connection::new(false, false)));
        };

        Ok(Some(
            Checkpoint::paginate(ctx, self.scope.clone(), page, filter).await?,
        ))
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

    /// The transactions in this epoch, optionally filtered by transaction filters.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
        filter: Option<TransactionFilter>,
    ) -> Result<Option<Connection<String, Transaction>>, RpcError> {
        let (Some(start), end) = try_join!(self.start(ctx), self.end(ctx))? else {
            return Ok(None);
        };
        let Some(checkpoint_viewed_at_exclusive_bound) =
            self.scope.checkpoint_viewed_at_exclusive_bound()
        else {
            return Ok(None);
        };

        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Epoch", "transactions");
        let page = Page::from_params(limits, first, after, last, before)?;

        let cp_lo_exclusive = (start.cp_lo as u64).checked_sub(1);
        let cp_hi = end.as_ref().map_or_else(
            || checkpoint_viewed_at_exclusive_bound,
            |end| end.cp_hi as u64,
        );

        let Some(filter) = filter.unwrap_or_default().intersect(TransactionFilter {
            after_checkpoint: cp_lo_exclusive.map(UInt53::from),
            before_checkpoint: Some(UInt53::from(cp_hi)),
            ..Default::default()
        }) else {
            return Ok(Some(Connection::new(false, false)));
        };

        Ok(Some(
            Transaction::paginate(ctx, self.scope.clone(), page, filter).await?,
        ))
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
        let Some(system_state) = self.system_state(ctx).await? else {
            return Ok(None);
        };

        let validator_set = match system_state {
            SuiSystemState::V1(inner) => inner.validators.into(),
            SuiSystemState::V2(inner) => inner.validators.into(),
            #[cfg(msim)]
            SuiSystemState::SimTestV1(_)
            | SuiSystemState::SimTestShallowV2(_)
            | SuiSystemState::SimTestDeepV2(_) => return Ok(None),
        };

        Ok(Some(validator_set))
    }

    /// The total number of checkpoints in this epoch.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    async fn total_checkpoints(&self, ctx: &Context<'_>) -> Result<Option<UInt53>, RpcError> {
        let (Some(start), end) = try_join!(self.start(ctx), self.end(ctx))? else {
            return Ok(None);
        };

        let lo = start.cp_lo as u64;
        let hi = match end.as_ref() {
            Some(end) => end.cp_hi as u64,
            None => {
                let Some(bound) = self.scope.checkpoint_viewed_at_exclusive_bound() else {
                    return Ok(None);
                };
                bound
            }
        };

        Ok(Some(UInt53::from(hi - lo)))
    }

    /// The total number of transaction blocks in this epoch (or `null` if the epoch has not finished yet).
    async fn total_transactions(&self, ctx: &Context<'_>) -> Result<Option<UInt53>, RpcError> {
        let (Some(cp_sequence_numbers), Some(end)) =
            try_join!(self.cp_sequence_numbers(ctx), self.end(ctx))?
        else {
            return Ok(None);
        };

        let lo = cp_sequence_numbers.tx_lo as u64;
        let hi = end.tx_hi as u64;

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

    /// The amount added to total gas fees to make up the total stake rewards (or `null` if the epoch has not finished yet).
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

    /// The storage fund available in this epoch (or `null` if the epoch has not finished yet).
    /// This fund is used to redistribute storage fees from past transactions to future validators.
    async fn fund_size(&self, ctx: &Context<'_>) -> Result<Option<BigInt>, RpcError> {
        let Some(StoredEpochEnd {
            storage_fund_balance,
            ..
        }) = self.end(ctx).await?
        else {
            return Ok(None);
        };

        Ok(storage_fund_balance.map(BigInt::from))
    }

    /// The difference between the fund inflow and outflow, representing the net amount of storage fees accumulated in this epoch (or `null` if the epoch has not finished yet).
    async fn net_inflow(&self, ctx: &Context<'_>) -> Result<Option<BigInt>, RpcError> {
        let Some(StoredEpochEnd {
            storage_charge: Some(storage_charge),
            storage_rebate: Some(storage_rebate),
            ..
        }) = self.end(ctx).await?
        else {
            return Ok(None);
        };

        Ok(Some(BigInt::from(storage_charge - storage_rebate)))
    }

    /// The storage fees paid for transactions executed during the epoch (or `null` if the epoch has not finished yet).
    async fn fund_inflow(&self, ctx: &Context<'_>) -> Result<Option<BigInt>, RpcError> {
        let Some(StoredEpochEnd { storage_charge, .. }) = self.end(ctx).await? else {
            return Ok(None);
        };

        Ok(storage_charge.map(BigInt::from))
    }

    /// The storage fee rebates paid to users who deleted the data associated with past transactions (or `null` if the epoch has not finished yet).
    async fn fund_outflow(&self, ctx: &Context<'_>) -> Result<Option<BigInt>, RpcError> {
        let Some(StoredEpochEnd { storage_rebate, .. }) = self.end(ctx).await? else {
            return Ok(None);
        };

        Ok(storage_rebate.map(BigInt::from))
    }

    /// SUI set aside to account for objects stored on-chain, at the start of the epoch.
    /// This is also used for storage rebates.
    async fn storage_fund(&self, ctx: &Context<'_>) -> Result<Option<StorageFund>, RpcError> {
        let Some(system_state) = self.system_state(ctx).await? else {
            return Ok(None);
        };

        let storage_fund = match system_state {
            SuiSystemState::V1(inner) => inner.storage_fund.into(),
            SuiSystemState::V2(inner) => inner.storage_fund.into(),
            #[cfg(msim)]
            SuiSystemState::SimTestV1(_)
            | SuiSystemState::SimTestShallowV2(_)
            | SuiSystemState::SimTestDeepV2(_) => return Ok(None),
        };

        Ok(Some(storage_fund))
    }

    /// Information about whether this epoch was started in safe mode, which happens if the full epoch change logic fails.
    async fn safe_mode(&self, ctx: &Context<'_>) -> Result<Option<SafeMode>, RpcError> {
        let Some(system_state) = self.system_state(ctx).await? else {
            return Ok(None);
        };

        let safe_mode = from_system_state(&system_state);

        Ok(Some(safe_mode))
    }

    /// The value of the `version` field of `0x5`, the `0x3::sui::SuiSystemState` object.
    /// This version changes whenever the fields contained in the system state object (held in a dynamic field attached to `0x5`) change.
    async fn system_state_version(&self, ctx: &Context<'_>) -> Result<Option<UInt53>, RpcError> {
        let Some(system_state) = self.system_state(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(system_state.system_state_version().into()))
    }

    /// Details of the system that are decided during genesis.
    async fn system_parameters(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<SystemParameters>, RpcError> {
        let Some(system_state) = self.system_state(ctx).await? else {
            return Ok(None);
        };

        let system_parameters = match system_state {
            SuiSystemState::V1(inner) => from_system_parameters_v1(inner.parameters),
            SuiSystemState::V2(inner) => from_system_parameters_v2(inner.parameters),
            #[cfg(msim)]
            SuiSystemState::SimTestV1(_)
            | SuiSystemState::SimTestShallowV2(_)
            | SuiSystemState::SimTestDeepV2(_) => return Ok(None),
        };

        Ok(Some(system_parameters))
    }

    /// Parameters related to the subsidy that supplements staking rewards
    async fn system_stake_subsidy(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<StakeSubsidy>, RpcError> {
        let Some(system_state) = self.system_state(ctx).await? else {
            return Ok(None);
        };

        let stake_subsidy = match system_state {
            SuiSystemState::V1(inner) => from_stake_subsidy_v1(inner.stake_subsidy),
            SuiSystemState::V2(inner) => from_stake_subsidy_v1(inner.stake_subsidy),
            #[cfg(msim)]
            SuiSystemState::SimTestV1(_)
            | SuiSystemState::SimTestShallowV2(_)
            | SuiSystemState::SimTestDeepV2(_) => return Ok(None),
        };

        Ok(Some(stake_subsidy))
    }

    /// A commitment by the committee at the end of epoch on the contents of the live object set at that time.
    /// This can be used to verify state snapshots.
    async fn live_object_set_digest(&self, ctx: &Context<'_>) -> Result<Option<String>, RpcError> {
        let Some(end) = self.end(ctx).await? else {
            return Ok(None);
        };

        let commitments: Vec<CheckpointCommitment> = bcs::from_bytes(&end.epoch_commitments)
            .context("Failed to deserialize epoch commitments")?;

        let digest = commitments.into_iter().next().map(
            |CheckpointCommitment::ECMHLiveObjectSetDigest(digest)| {
                Base58::encode(digest.digest.into_inner())
            },
        );

        Ok(digest)
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
            cp_sequence_numbers: OnceCell::new(),
        }
    }

    /// Load the epoch from the store, and return it fully inflated (with contents already
    /// fetched). If `epoch_id` is provided, the epoch with that ID is loaded. Otherwise, the
    /// latest epoch for the current checkpoint is loaded.
    ///
    /// Returns `None` if the epoch does not exist, started after the checkpoint being viewed,
    /// or when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        scope: Scope,
        epoch_id: Option<UInt53>,
    ) -> Result<Option<Self>, RpcError> {
        // In execution scope, epoch queries return None
        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(None);
        };

        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

        let stored = match epoch_id {
            Some(id) => pg_loader
                .load_one(EpochStartKey(id.into()))
                .await
                .context("Failed to fetch epoch start information by start key")?,
            None => pg_loader
                .load_one(CheckpointBoundedEpochStartKey(checkpoint_viewed_at))
                .await
                .context(
                    "Failed to fetch epoch start information by checkpoint bounded start key",
                )?,
        }
        .filter(|start| start.cp_lo as u64 <= checkpoint_viewed_at);

        Ok(stored.map(|start| Self {
            epoch_id: start.epoch as u64,
            scope: scope.clone(),
            start: OnceCell::from(Some(start)),
            end: OnceCell::new(),
            cp_sequence_numbers: OnceCell::new(),
        }))
    }

    /// Get the epoch start information.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    async fn start(&self, ctx: &Context<'_>) -> Result<&Option<StoredEpochStart>, Error> {
        let Some(checkpoint_viewed_at) = self.scope.checkpoint_viewed_at() else {
            return Ok(&None);
        };

        self.start
            .get_or_try_init(async || {
                let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

                let stored = pg_loader
                    .load_one(EpochStartKey(self.epoch_id))
                    .await
                    .context("Failed to fetch epoch start information")?
                    .filter(|start| start.cp_lo as u64 <= checkpoint_viewed_at);

                Ok(stored)
            })
            .await
    }

    async fn system_state(&self, ctx: &Context<'_>) -> Result<Option<SuiSystemState>, RpcError> {
        let Some(start) = self.start(ctx).await? else {
            return Ok(None);
        };

        let system_state = bcs::from_bytes::<SuiSystemState>(&start.system_state)
            .context("Failed to deserialize system state")?;

        Ok(Some(system_state))
    }

    /// Attempt to fetch information about the end of an epoch from the store. May return an empty
    /// response if the epoch has not ended yet, as of the checkpoint being viewed, or when
    /// no checkpoint is set in scope (e.g. execution scope).
    async fn end(&self, ctx: &Context<'_>) -> Result<&Option<StoredEpochEnd>, Error> {
        let Some(checkpoint_bound) = self.scope.checkpoint_viewed_at_exclusive_bound() else {
            return Ok(&None);
        };

        self.end
            .get_or_try_init(async || {
                let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;
                let stored = pg_loader
                    .load_one(EpochEndKey(self.epoch_id))
                    .await
                    .context("Failed to fetch epoch end information")?
                    .filter(|end| end.cp_hi as u64 <= checkpoint_bound);

                Ok(stored)
            })
            .await
    }

    async fn cp_sequence_numbers(
        &self,
        ctx: &Context<'_>,
    ) -> Result<&Option<StoredCpSequenceNumbers>, Error> {
        let Some(start) = self.start(ctx).await? else {
            return Ok(&None);
        };

        self.cp_sequence_numbers
            .get_or_try_init(async || {
                let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

                let stored = pg_loader
                    .load_one(CpSequenceNumberKey(start.cp_lo as u64))
                    .await
                    .context("Failed to fetch cp sequence number information")?;

                Ok(stored)
            })
            .await
    }
}
