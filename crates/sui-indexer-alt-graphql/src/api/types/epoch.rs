// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::Object;
use async_graphql::connection::Connection;
use async_graphql::dataloader::DataLoader;
use fastcrypto::encoding::Base58;
use fastcrypto::encoding::Encoding;
use futures::future::OptionFuture;
use futures::join;
use futures::try_join;
use move_core_types::language_storage::StructTag;
use sui_indexer_alt_reader::cp_sequence_numbers::CpSequenceNumberKey;
use sui_indexer_alt_reader::epochs::CheckpointBoundedEpochStartKey;
use sui_indexer_alt_reader::epochs::EpochEndKey;
use sui_indexer_alt_reader::epochs::EpochStartKey;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_schema::cp_sequence_numbers::StoredCpSequenceNumbers;
use sui_indexer_alt_schema::epochs::StoredEpochEnd;
use sui_indexer_alt_schema::epochs::StoredEpochStart;
use sui_types::SUI_DENY_LIST_OBJECT_ID;
use sui_types::SUI_SYSTEM_ADDRESS;
use sui_types::TypeTag;
use sui_types::messages_checkpoint::CheckpointCommitment;
use sui_types::sui_system_state::SUI_SYSTEM_STATE_INNER_MODULE_NAME;
use sui_types::sui_system_state::SUI_SYSTEM_STATE_INNER_V1_STRUCT_NAME;
use sui_types::sui_system_state::SUI_SYSTEM_STATE_INNER_V2_STRUCT_NAME;
use tokio::sync::OnceCell;

use crate::api::scalars::big_int::BigInt;
use crate::api::scalars::cursor::JsonCursor;
use crate::api::scalars::date_time::DateTime;
use crate::api::scalars::id::Id;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::checkpoint::CCheckpoint;
use crate::api::types::checkpoint::Checkpoint;
use crate::api::types::checkpoint::filter::CheckpointFilter;
use crate::api::types::move_package::CSysPackage;
use crate::api::types::move_package::MovePackage;
use crate::api::types::move_type::MoveType;
use crate::api::types::move_value::MoveValue;
use crate::api::types::object::Object;
use crate::api::types::protocol_configs::ProtocolConfigs;
use crate::api::types::transaction::CTransaction;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::api::types::transaction::filter::TransactionFilterValidator as TFValidator;
use crate::api::types::validator_set::ValidatorSet;
use crate::error::RpcError;
use crate::error::upcast;
use crate::pagination::Page;
use crate::pagination::PaginationConfig;
use crate::scope::Scope;
use crate::task::watermark::Watermarks;

pub(crate) type CEpoch = JsonCursor<usize>;

pub(crate) struct Epoch {
    pub(crate) epoch_id: u64,
    scope: Scope,
    start: OnceCell<Option<StoredEpochStart>>,
    end: OnceCell<Option<StoredEpochEnd>>,
    sequence_numbers: OnceCell<SequenceNumbers>,
}

#[derive(Default)]
struct SequenceNumbers {
    /// Sequence numbers (transaction and checkpoint) at the start of this epoch.
    start: Option<StoredCpSequenceNumbers>,
    /// Sequence numbers for the checkpoint after `checkpoint_viewed_at`. Used to determine
    /// the transaction count for in-progress epochs when the epoch hasn't ended yet.
    next: Option<StoredCpSequenceNumbers>,
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
    /// The epoch's globally unique identifier, which can be passed to `Query.node` to refetch it.
    pub(crate) async fn id(&self) -> Id {
        Id::Epoch(self.epoch_id)
    }

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
    ) -> Option<Result<Connection<String, Checkpoint>, RpcError>> {
        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("Epoch", "checkpoints");
                let page = Page::from_params(limits, first, after, last, before)?;

                let Some(filter) = filter.unwrap_or_default().intersect(CheckpointFilter {
                    at_epoch: Some(self.epoch_id.into()),
                    ..Default::default()
                }) else {
                    return Ok(Connection::new(false, false));
                };

                Checkpoint::paginate(ctx, self.scope.clone(), page, filter).await
            }
            .await,
        )
    }

    /// State of the Coin DenyList object (0x403) at the start of this epoch.
    ///
    /// The DenyList controls access to Regulated Coins. Writes to the DenyList are accumulated and only take effect on the next epoch boundary. Consequently, it's possible to determine the state of the DenyList for a transaction by reading it at the start of the epoch the transaction is in.
    async fn coin_deny_list(&self, ctx: &Context<'_>) -> Option<Result<Object, RpcError>> {
        async {
            let Some(start) = self.start(ctx).await? else {
                return Ok(None);
            };

            let cp = (start.cp_lo as u64).saturating_sub(1);
            let scope = self.scope.with_root_checkpoint(cp);
            Object::latest(ctx, scope, SUI_DENY_LIST_OBJECT_ID.into()).await
        }
        .await
        .transpose()
    }

    /// The timestamp associated with the last checkpoint in the epoch (or `null` if the epoch has not finished yet).
    async fn end_timestamp(&self, ctx: &Context<'_>) -> Option<Result<DateTime, RpcError>> {
        async {
            let Some(end) = self.end(ctx).await? else {
                return Ok(None);
            };

            Ok(Some(DateTime::from_ms(end.end_timestamp_ms)?))
        }
        .await
        .transpose()
    }

    /// The storage fees paid for transactions executed during the epoch (or `null` if the epoch has not finished yet).
    async fn fund_inflow(&self, ctx: &Context<'_>) -> Option<Result<BigInt, RpcError>> {
        async {
            let Some(StoredEpochEnd { storage_charge, .. }) = self.end(ctx).await? else {
                return Ok(None);
            };

            Ok(storage_charge.map(BigInt::from))
        }
        .await
        .transpose()
    }

    /// The storage fee rebates paid to users who deleted the data associated with past transactions (or `null` if the epoch has not finished yet).
    async fn fund_outflow(&self, ctx: &Context<'_>) -> Option<Result<BigInt, RpcError>> {
        async {
            let Some(StoredEpochEnd { storage_rebate, .. }) = self.end(ctx).await? else {
                return Ok(None);
            };

            Ok(storage_rebate.map(BigInt::from))
        }
        .await
        .transpose()
    }

    /// The storage fund available in this epoch (or `null` if the epoch has not finished yet).
    /// This fund is used to redistribute storage fees from past transactions to future validators.
    async fn fund_size(&self, ctx: &Context<'_>) -> Option<Result<BigInt, RpcError>> {
        async {
            let Some(StoredEpochEnd {
                storage_fund_balance,
                ..
            }) = self.end(ctx).await?
            else {
                return Ok(None);
            };

            Ok(storage_fund_balance.map(BigInt::from))
        }
        .await
        .transpose()
    }

    /// A commitment by the committee at the end of epoch on the contents of the live object set at that time.
    /// This can be used to verify state snapshots.
    async fn live_object_set_digest(&self, ctx: &Context<'_>) -> Option<Result<String, RpcError>> {
        async {
            let Some(end) = self.end(ctx).await? else {
                return Ok(None);
            };

            let commitments: Vec<CheckpointCommitment> = bcs::from_bytes(&end.epoch_commitments)
                .context("Failed to deserialize epoch commitments")?;

            for commitment in commitments {
                if let CheckpointCommitment::ECMHLiveObjectSetDigest(digest) = commitment {
                    return Ok(Some(Base58::encode(digest.digest.into_inner())));
                }
            }
            Ok(None)
        }
        .await
        .transpose()
    }

    /// The difference between the fund inflow and outflow, representing the net amount of storage fees accumulated in this epoch (or `null` if the epoch has not finished yet).
    async fn net_inflow(&self, ctx: &Context<'_>) -> Option<Result<BigInt, RpcError>> {
        async {
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
        .await
        .transpose()
    }

    /// The epoch's corresponding protocol configuration, including the feature flags and the configuration options.
    async fn protocol_configs(
        &self,
        ctx: &Context<'_>,
    ) -> Option<Result<ProtocolConfigs, RpcError>> {
        let start = self.start(ctx).await.ok()?.as_ref()?;
        Some(Ok(ProtocolConfigs::with_protocol_version(
            start.protocol_version as u64,
        )))
    }

    /// The minimum gas price that a quorum of validators are guaranteed to sign a transaction for in this epoch.
    async fn reference_gas_price(&self, ctx: &Context<'_>) -> Option<Result<BigInt, RpcError>> {
        let start = self.start(ctx).await.ok()?.as_ref()?;
        Some(Ok(BigInt::from(start.reference_gas_price)))
    }

    /// The timestamp associated with the first checkpoint in the epoch.
    async fn start_timestamp(&self, ctx: &Context<'_>) -> Option<Result<DateTime, RpcError>> {
        async {
            let Some(contents) = self.start(ctx).await? else {
                return Ok(None);
            };

            Ok(Some(DateTime::from_ms(contents.start_timestamp_ms)?))
        }
        .await
        .transpose()
    }

    /// The system packages used by all transactions in this epoch.
    async fn system_packages(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CSysPackage>,
        last: Option<u64>,
        before: Option<CSysPackage>,
    ) -> Option<Result<Connection<String, MovePackage>, RpcError>> {
        async {
            let pagination: &PaginationConfig = ctx.data()?;
            let limits = pagination.limits("Epoch", "systemPackages");
            let page = Page::from_params(limits, first, after, last, before)?;

            let Some(contents) = self.start(ctx).await.map_err(upcast)? else {
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
        .await
        .transpose()
    }

    /// The contents of the system state inner object at the start of this epoch.
    async fn system_state(&self, ctx: &Context<'_>) -> Result<Option<MoveValue>, RpcError> {
        self.system_state_impl(ctx).await
    }

    /// The total number of checkpoints in this epoch.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    async fn total_checkpoints(&self, ctx: &Context<'_>) -> Option<Result<UInt53, RpcError>> {
        async {
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
        .await
        .transpose()
    }

    /// The total amount of gas fees (in MIST) that were paid in this epoch (or `null` if the epoch has not finished yet).
    async fn total_gas_fees(&self, ctx: &Context<'_>) -> Option<Result<BigInt, RpcError>> {
        async {
            let Some(StoredEpochEnd { total_gas_fees, .. }) = self.end(ctx).await? else {
                return Ok(None);
            };

            Ok(total_gas_fees.map(BigInt::from))
        }
        .await
        .transpose()
    }

    /// The total MIST rewarded as stake (or `null` if the epoch has not finished yet).
    async fn total_stake_rewards(&self, ctx: &Context<'_>) -> Option<Result<BigInt, RpcError>> {
        async {
            let Some(StoredEpochEnd {
                total_stake_rewards_distributed,
                ..
            }) = self.end(ctx).await?
            else {
                return Ok(None);
            };

            Ok(total_stake_rewards_distributed.map(BigInt::from))
        }
        .await
        .transpose()
    }

    /// The amount added to total gas fees to make up the total stake rewards (or `null` if the epoch has not finished yet).
    async fn total_stake_subsidies(&self, ctx: &Context<'_>) -> Option<Result<BigInt, RpcError>> {
        async {
            let Some(StoredEpochEnd {
                stake_subsidy_amount,
                ..
            }) = self.end(ctx).await?
            else {
                return Ok(None);
            };

            Ok(stake_subsidy_amount.map(BigInt::from))
        }
        .await
        .transpose()
    }

    /// The total number of transaction blocks in this epoch.
    ///
    /// If the epoch has not finished yet, this number is computed based on the number of transactions at the latest known checkpoint.
    async fn total_transactions(&self, ctx: &Context<'_>) -> Option<Result<UInt53, RpcError>> {
        async {
            let watermarks: &Arc<Watermarks> = ctx.data()?;
            let (sequence_numbers, end) = try_join!(self.sequence_numbers(ctx), self.end(ctx))?;

            let Some(start) = &sequence_numbers.start else {
                return Ok(None);
            };

            let lo = start.tx_lo as u64;
            let hi = if let Some(end) = end {
                // If the epoch has already ended as of the latest checkpoint, its end record
                // stores its transaction high watermark.
                end.tx_hi as u64
            } else if let Some(next) = &sequence_numbers.next {
                // Otherwise, we have attempted to fetch the transaction low watermark of the
                // checkpoint *after* the one being viewed at.
                next.tx_lo as u64
            } else {
                // If all else fails, assume that the checkpoint being viewed at is the latest one
                // known to the service, and use its global transaction high watermark.
                watermarks.high_watermark().transaction()
            };

            Ok(Some(UInt53::from(hi.saturating_sub(lo))))
        }
        .await
        .transpose()
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
        #[graphql(validator(custom = "TFValidator"))] filter: Option<TransactionFilter>,
    ) -> Option<Result<Connection<String, Transaction>, RpcError>> {
        async {
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
        .await
        .transpose()
    }

    /// Validator-related properties, including the active validators.
    async fn validator_set(&self, ctx: &Context<'_>) -> Option<Result<ValidatorSet, RpcError>> {
        async {
            let Some(system_state) = self.system_state_impl(ctx).await? else {
                return Ok(None);
            };

            let Some(layout) = system_state.type_.layout_impl().await? else {
                return Ok(None);
            };

            let validator_set = ValidatorSet::from_system_state(
                system_state.type_.scope,
                &system_state.native,
                &layout,
            )?;

            Ok(Some(validator_set))
        }
        .await
        .transpose()
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
            sequence_numbers: OnceCell::new(),
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
            sequence_numbers: OnceCell::new(),
        }))
    }

    /// Paginate through epochs.
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: &Scope,
        page: Page<CEpoch>,
    ) -> Result<Option<Connection<String, Epoch>>, RpcError> {
        let Some(latest_epoch) = Epoch::fetch(ctx, scope.clone(), None).await? else {
            return Ok(Some(Connection::new(false, false)));
        };

        page.paginate_indices(1 + latest_epoch.epoch_id as usize, |id| {
            Ok(Epoch::with_id(scope.clone(), id as u64))
        })
        .map(Some)
    }

    /// Get the epoch start information.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    async fn start(&self, ctx: &Context<'_>) -> Result<&Option<StoredEpochStart>, RpcError> {
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

    /// Attempt to fetch information about the end of an epoch from the store. May return an empty
    /// response if the epoch has not ended yet, as of the checkpoint being viewed, or when
    /// no checkpoint is set in scope (e.g. execution scope).
    async fn end(&self, ctx: &Context<'_>) -> Result<&Option<StoredEpochEnd>, RpcError> {
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

    async fn sequence_numbers(&self, ctx: &Context<'_>) -> Result<&SequenceNumbers, RpcError> {
        self.sequence_numbers
            .get_or_try_init(async || {
                let Some(start) = self.start(ctx).await? else {
                    return Ok(SequenceNumbers::default());
                };

                let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

                let start = pg_loader.load_one(CpSequenceNumberKey(start.cp_lo as u64));
                let next: OptionFuture<_> = self
                    .scope
                    .checkpoint_viewed_at()
                    .map(|cp| pg_loader.load_one(CpSequenceNumberKey(cp + 1)))
                    .into();

                let (start, next) = join!(start, next);
                let start = start.context("Failed to fetch epoch start sequence numbers")?;
                let next = next
                    .transpose()
                    .context("Failed to fetch latest checkpoint sequence numbers")?
                    .flatten();

                Ok(SequenceNumbers { start, next })
            })
            .await
    }

    /// The contents of the system state inner object at the start of this epoch.
    async fn system_state_impl(&self, ctx: &Context<'_>) -> Result<Option<MoveValue>, RpcError> {
        let Some(start) = self.start(ctx).await? else {
            return Ok(None);
        };

        let scope = self.scope.with_root_checkpoint(start.cp_lo as u64);
        let struct_name = match start.system_state.first() {
            Some(0) => SUI_SYSTEM_STATE_INNER_V1_STRUCT_NAME,
            Some(1) => SUI_SYSTEM_STATE_INNER_V2_STRUCT_NAME,
            _ => {
                return Ok(None);
            }
        };

        let tag = TypeTag::Struct(Box::new(StructTag {
            address: SUI_SYSTEM_ADDRESS,
            module: SUI_SYSTEM_STATE_INNER_MODULE_NAME.to_owned(),
            name: struct_name.to_owned(),
            type_params: vec![],
        }));

        let type_ = MoveType::from_native(tag, scope);
        let native = start.system_state[1..].to_owned();

        Ok(Some(MoveValue { type_, native }))
    }
}
