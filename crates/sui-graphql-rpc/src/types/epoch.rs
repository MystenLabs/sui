// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::db_data_provider::{convert_to_validators, PgManager};
use crate::data::{Db, DbConnection, QueryExecutor};
use crate::error::Error;

use super::big_int::BigInt;
use super::checkpoint::{self, Checkpoint, CheckpointId};
use super::cursor::Page;
use super::date_time::DateTime;
use super::protocol_config::ProtocolConfigs;
use super::system_state_summary::SystemStateSummary;
use super::transaction_block::{self, TransactionBlock, TransactionBlockFilter};
use super::validator_set::ValidatorSet;
use async_graphql::connection::Connection;
use async_graphql::*;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, SelectableHelper};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer::models::epoch::QueryableEpochInfo;
use sui_indexer::schema::epochs;
use sui_types::messages_checkpoint::CheckpointCommitment as EpochCommitment;

pub(crate) struct Epoch {
    pub stored: QueryableEpochInfo,
    pub checkpoint_viewed_at: Option<u64>,
}

/// Operation of the Sui network is temporally partitioned into non-overlapping epochs,
/// and the network aims to keep epochs roughly the same duration as each other.
/// During a particular epoch the following data is fixed:
///
/// - the protocol version
/// - the reference gas price
/// - the set of participating validators
#[Object]
impl Epoch {
    /// The epoch's id as a sequence number that starts at 0 and is incremented by one at every epoch change.
    async fn epoch_id(&self) -> u64 {
        self.stored.epoch as u64
    }

    /// The minimum gas price that a quorum of validators are guaranteed to sign a transaction for.
    async fn reference_gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.stored.reference_gas_price as u64))
    }

    /// Validator related properties, including the active validators.
    async fn validator_set(&self, ctx: &Context<'_>) -> Result<Option<ValidatorSet>> {
        let system_state = ctx
            .data_unchecked::<PgManager>()
            .fetch_sui_system_state(Some(self.stored.epoch as u64))
            .await?;

        let checkpoint_viewed_at = match self.checkpoint_viewed_at {
            Some(value) => Ok(value),
            None => Checkpoint::query_latest_checkpoint_sequence_number(ctx.data_unchecked()).await,
        }?;

        let active_validators =
            convert_to_validators(system_state.active_validators, None, checkpoint_viewed_at);
        let validator_set = ValidatorSet {
            total_stake: Some(BigInt::from(self.stored.total_stake)),
            active_validators: Some(active_validators),
            pending_removals: Some(system_state.pending_removals),
            pending_active_validators_id: Some(system_state.pending_active_validators_id.into()),
            pending_active_validators_size: Some(system_state.pending_active_validators_size),
            staking_pool_mappings_id: Some(system_state.staking_pool_mappings_id.into()),
            staking_pool_mappings_size: Some(system_state.staking_pool_mappings_size),
            inactive_pools_id: Some(system_state.inactive_pools_id.into()),
            inactive_pools_size: Some(system_state.inactive_pools_size),
            validator_candidates_id: Some(system_state.validator_candidates_id.into()),
            validator_candidates_size: Some(system_state.validator_candidates_size),
            checkpoint_viewed_at,
        };
        Ok(Some(validator_set))
    }

    /// The epoch's starting timestamp.
    async fn start_timestamp(&self) -> Result<DateTime, Error> {
        DateTime::from_ms(self.stored.epoch_start_timestamp)
    }

    /// The epoch's ending timestamp.
    async fn end_timestamp(&self) -> Result<Option<DateTime>, Error> {
        self.stored
            .epoch_end_timestamp
            .map(DateTime::from_ms)
            .transpose()
    }

    /// The total number of checkpoints in this epoch.
    async fn total_checkpoints(&self, ctx: &Context<'_>) -> Result<Option<BigInt>> {
        let last = match self.stored.last_checkpoint_id {
            Some(last) => last as u64,
            None => Checkpoint::query(ctx.data_unchecked(), CheckpointId::default(), None)
                .await
                .extend()?
                .map_or(self.stored.first_checkpoint_id as u64, |c| {
                    c.sequence_number_impl()
                }),
        };
        Ok(Some(BigInt::from(
            last - self.stored.first_checkpoint_id as u64,
        )))
    }

    /// The total number of transaction blocks in this epoch.
    async fn total_transactions(&self) -> Result<Option<u64>> {
        // TODO: this currently returns None for the current epoch. Fix this.
        Ok(self.stored.epoch_total_transactions.map(|v| v as u64))
    }

    /// The total amount of gas fees (in MIST) that were paid in this epoch.
    async fn total_gas_fees(&self) -> Option<BigInt> {
        self.stored.total_gas_fees.map(BigInt::from)
    }

    /// The total MIST rewarded as stake.
    async fn total_stake_rewards(&self) -> Option<BigInt> {
        self.stored
            .total_stake_rewards_distributed
            .map(BigInt::from)
    }

    /// The amount added to total gas fees to make up the total stake rewards.
    async fn total_stake_subsidies(&self) -> Option<BigInt> {
        self.stored.stake_subsidy_amount.map(BigInt::from)
    }

    /// The storage fund available in this epoch.
    /// This fund is used to redistribute storage fees from past transactions
    /// to future validators.
    async fn fund_size(&self) -> Option<BigInt> {
        Some(BigInt::from(self.stored.storage_fund_balance))
    }

    /// The difference between the fund inflow and outflow, representing
    /// the net amount of storage fees accumulated in this epoch.
    async fn net_inflow(&self) -> Option<BigInt> {
        if let (Some(fund_inflow), Some(fund_outflow)) =
            (self.stored.storage_charge, self.stored.storage_rebate)
        {
            Some(BigInt::from(fund_inflow - fund_outflow))
        } else {
            None
        }
    }

    /// The storage fees paid for transactions executed during the epoch.
    async fn fund_inflow(&self) -> Option<BigInt> {
        self.stored.storage_charge.map(BigInt::from)
    }

    /// The storage fee rebates paid to users who deleted the data associated with past
    /// transactions.
    async fn fund_outflow(&self) -> Option<BigInt> {
        self.stored.storage_rebate.map(BigInt::from)
    }

    /// The epoch's corresponding protocol configuration, including the feature flags and the
    /// configuration options.
    async fn protocol_configs(&self, ctx: &Context<'_>) -> Result<ProtocolConfigs> {
        ProtocolConfigs::query(ctx.data_unchecked(), Some(self.protocol_version()))
            .await
            .extend()
    }

    #[graphql(flatten)]
    async fn system_state_summary(&self, ctx: &Context<'_>) -> Result<SystemStateSummary> {
        let state = ctx
            .data_unchecked::<PgManager>()
            .fetch_sui_system_state(Some(self.stored.epoch as u64))
            .await?;
        Ok(SystemStateSummary { native: state })
    }

    /// A commitment by the committee at the end of epoch on the contents of the live object set at
    /// that time. This can be used to verify state snapshots.
    async fn live_object_set_digest(&self) -> Result<Option<String>> {
        let Some(commitments) = self.stored.epoch_commitments.as_ref() else {
            return Ok(None);
        };
        let commitments: Vec<EpochCommitment> = bcs::from_bytes(commitments).map_err(|e| {
            Error::Internal(format!("Error deserializing commitments: {e}")).extend()
        })?;

        let digest = commitments.into_iter().next().map(|commitment| {
            let EpochCommitment::ECMHLiveObjectSetDigest(digest) = commitment;
            Base58::encode(digest.digest.into_inner())
        });

        Ok(digest)
    }

    /// The epoch's corresponding checkpoints.
    async fn checkpoints(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<checkpoint::Cursor>,
        last: Option<u64>,
        before: Option<checkpoint::Cursor>,
    ) -> Result<Connection<String, Checkpoint>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let epoch = self.stored.epoch as u64;
        Checkpoint::paginate(
            ctx.data_unchecked(),
            page,
            Some(epoch),
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    /// The epoch's corresponding transaction blocks.
    async fn transaction_blocks(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<transaction_block::Cursor>,
        last: Option<u64>,
        before: Option<transaction_block::Cursor>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Connection<String, TransactionBlock>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        #[allow(clippy::unnecessary_lazy_evaluations)] // rust-lang/rust-clippy#9422
        let Some(filter) = filter
            .unwrap_or_default()
            .intersect(TransactionBlockFilter {
                after_checkpoint: (self.stored.first_checkpoint_id > 0)
                    .then(|| self.stored.first_checkpoint_id as u64 - 1),
                before_checkpoint: self.stored.last_checkpoint_id.map(|id| id as u64 + 1),
                ..Default::default()
            })
        else {
            return Ok(Connection::new(false, false));
        };

        TransactionBlock::paginate(
            ctx.data_unchecked(),
            page,
            filter,
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }
}

impl Epoch {
    /// The epoch's protocol version.
    pub(crate) fn protocol_version(&self) -> u64 {
        self.stored.protocol_version as u64
    }

    /// Look up an `Epoch` in the database, optionally filtered by its Epoch ID. If no ID is
    /// supplied, defaults to fetching the latest epoch.
    pub(crate) async fn query(
        db: &Db,
        filter: Option<u64>,
        checkpoint_viewed_at: Option<u64>,
    ) -> Result<Option<Self>, Error> {
        use epochs::dsl;

        let id = filter.map(|id| id as i64);
        let (stored, checkpoint_viewed_at): (Option<QueryableEpochInfo>, u64) = db
            .execute_repeatable(move |conn| {
                let checkpoint_viewed_at = match checkpoint_viewed_at {
                    Some(value) => Ok(value),
                    None => Checkpoint::available_range(conn).map(|(_, rhs)| rhs),
                }?;

                let stored = conn
                    .first(move || {
                        let mut query = dsl::epochs
                            .select(QueryableEpochInfo::as_select())
                            .order_by(dsl::epoch.desc())
                            .into_boxed();

                        // Bound the query on `checkpoint_viewed_at` by filtering for the epoch
                        // whose `first_checkpoint_id <= checkpoint_viewed_at`, selecting the epoch
                        // with the largest `first_checkpoint_id` among the filtered set.
                        query = query
                            .filter(dsl::first_checkpoint_id.le(checkpoint_viewed_at as i64))
                            .order_by(dsl::first_checkpoint_id.desc());

                        if let Some(id) = id {
                            query = query.filter(dsl::epoch.eq(id));
                        }

                        query
                    })
                    .optional()?;

                Ok::<_, diesel::result::Error>((stored, checkpoint_viewed_at))
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch epoch: {e}")))?;

        Ok(stored.map(|stored| Epoch {
            stored,
            checkpoint_viewed_at: Some(checkpoint_viewed_at),
        }))
    }
}
