// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Object, connection::Connection};

use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_types::{
    crypto::AuthorityStrongQuorumSignInfo,
    message_envelope::Message,
    messages_checkpoint::{
        CheckpointCommitment, CheckpointContents as NativeCheckpointContents, CheckpointSummary,
    },
};

use crate::{
    api::{
        query::Query,
        scalars::{base64::Base64, cursor::JsonCursor, date_time::DateTime, uint53::UInt53},
        types::available_range::AvailableRangeKey,
    },
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
    task::watermark::Watermarks,
};

use super::{
    checkpoint::filter::{CheckpointFilter, checkpoint_bounds, cp_by_epoch, cp_unfiltered},
    epoch::Epoch,
    gas::GasCostSummary,
    transaction::{
        CTransaction, Transaction,
        filter::{TransactionFilter, TransactionFilterValidator as TFValidator},
    },
    validator_aggregated_signature::ValidatorAggregatedSignature,
};

pub(crate) mod filter;

pub(crate) struct Checkpoint {
    pub(crate) sequence_number: u64,
    pub(crate) scope: Scope,
}

#[derive(Clone)]
struct CheckpointContents {
    // TODO: Remove when the scope is used in a nested field.
    #[allow(unused)]
    scope: Scope,
    contents: Option<(
        CheckpointSummary,
        NativeCheckpointContents,
        AuthorityStrongQuorumSignInfo,
    )>,
}

pub(crate) type CCheckpoint = JsonCursor<u64>;

/// Checkpoints contain finalized transactions and are used for node synchronization and global transaction ordering.
#[Object]
impl Checkpoint {
    /// The checkpoint's position in the total order of finalized checkpoints, agreed upon by consensus.
    async fn sequence_number(&self) -> UInt53 {
        self.sequence_number.into()
    }

    /// Query the RPC as if this checkpoint were the latest checkpoint.
    async fn query(&self) -> Result<Option<Query>, RpcError> {
        let scope = Some(
            self.scope
                .with_checkpoint_viewed_at(self.sequence_number)
                .context("Checkpoint in the future")?,
        );

        Ok(Some(Query { scope }))
    }

    #[graphql(flatten)]
    async fn contents(&self, ctx: &Context<'_>) -> Result<CheckpointContents, RpcError> {
        CheckpointContents::fetch(ctx, self.scope.clone(), self.sequence_number).await
    }
}

#[Object]
impl CheckpointContents {
    /// A commitment by the committee at each checkpoint on the artifacts of the checkpoint.
    /// e.g., object checkpoint states
    async fn artifacts_digest(&self) -> Result<Option<String>, RpcError> {
        let Some((summary, _, _)) = &self.contents else {
            return Ok(None);
        };

        for commitment in &summary.checkpoint_commitments {
            if let CheckpointCommitment::CheckpointArtifactsDigest(digest) = commitment {
                return Ok(Some(digest.base58_encode()));
            }
        }

        Ok(None)
    }

    /// A 32-byte hash that uniquely identifies the checkpoint, encoded in Base58. This is a hash of the checkpoint's summary.
    async fn digest(&self) -> Result<Option<String>, RpcError> {
        let Some((summary, _, _)) = &self.contents else {
            return Ok(None);
        };
        Ok(Some(summary.digest().base58_encode()))
    }

    /// A 32-byte hash that uniquely identifies the checkpoint's content, encoded in Base58.
    async fn content_digest(&self) -> Result<Option<String>, RpcError> {
        let Some((summary, _, _)) = &self.contents else {
            return Ok(None);
        };
        Ok(Some(summary.content_digest.base58_encode()))
    }

    /// The epoch that this checkpoint is part of.
    async fn epoch(&self) -> Option<Epoch> {
        let (summary, _, _) = self.contents.as_ref()?;
        Some(Epoch::with_id(self.scope.clone(), summary.epoch))
    }

    /// The total number of transactions in the network by the end of this checkpoint.
    async fn network_total_transactions(&self) -> Option<UInt53> {
        let (summary, _, _) = self.contents.as_ref()?;
        Some(summary.network_total_transactions.into())
    }

    /// The digest of the previous checkpoint's summary.
    async fn previous_checkpoint_digest(&self) -> Result<Option<String>, RpcError> {
        let Some((summary, _, _)) = &self.contents else {
            return Ok(None);
        };
        Ok(summary
            .previous_digest
            .as_ref()
            .map(|digest| digest.base58_encode()))
    }

    /// The computation cost, storage cost, storage rebate, and non-refundable storage fee accumulated during this epoch, up to and including this checkpoint. These values increase monotonically across checkpoints in the same epoch, and reset on epoch boundaries.
    async fn rolling_gas_summary(&self) -> Option<GasCostSummary> {
        let (summary, _, _) = self.contents.as_ref()?;
        Some(GasCostSummary::from(
            summary.epoch_rolling_gas_cost_summary.clone(),
        ))
    }

    /// The Base64 serialized BCS bytes of this checkpoint's summary.
    async fn summary_bcs(&self) -> Result<Option<Base64>, RpcError> {
        let Some((summary, _, _)) = &self.contents else {
            return Ok(None);
        };
        Ok(Some(Base64::from(
            bcs::to_bytes(summary).context("Failed to serialize checkpoint summary")?,
        )))
    }

    /// The Base64 serialized BCS bytes of this checkpoint's contents.
    async fn content_bcs(&self) -> Result<Option<Base64>, RpcError> {
        let Some((_, content, _)) = &self.contents else {
            return Ok(None);
        };
        Ok(Some(Base64::from(
            bcs::to_bytes(content).context("Failed to serialize checkpoint content")?,
        )))
    }

    /// The timestamp at which the checkpoint is agreed to have happened according to consensus. Transactions that access time in this checkpoint will observe this timestamp.
    async fn timestamp(&self) -> Result<Option<DateTime>, RpcError> {
        let Some((summary, _, _)) = &self.contents else {
            return Ok(None);
        };

        Ok(Some(DateTime::from_ms(summary.timestamp_ms as i64)?))
    }

    /// The aggregation of signatures from a quorum of validators for the checkpoint proposal.
    async fn validator_signatures(&self) -> Result<Option<ValidatorAggregatedSignature>, RpcError> {
        let Some((_, _, authority_info)) = &self.contents else {
            return Ok(None);
        };
        Ok(Some(ValidatorAggregatedSignature::with_authority_info(
            self.scope.clone(),
            authority_info.clone(),
        )))
    }

    // The transactions in this checkpoint.
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
        #[graphql(validator(custom = "TFValidator"))] filter: Option<TransactionFilter>,
    ) -> Result<Option<Connection<String, Transaction>>, RpcError> {
        let Some((summary, _, _)) = &self.contents else {
            return Ok(None);
        };
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Checkpoint", "transactions");
        let page = Page::from_params(limits, first, after, last, before)?;

        let Some(filter) = filter.unwrap_or_default().intersect(TransactionFilter {
            at_checkpoint: Some(UInt53::from(summary.sequence_number)),
            ..Default::default()
        }) else {
            return Ok(Some(Connection::new(false, false)));
        };

        Ok(Some(
            Transaction::paginate(ctx, self.scope.clone(), page, filter).await?,
        ))
    }
}

impl Checkpoint {
    /// Construct a checkpoint that is represented by just its identifier (its sequence number).
    ///
    /// If no sequence_number is provided, defaults to the scope's checkpoint.
    /// Returns `None` if the checkpoint is set in the future relative to the current scope's
    /// checkpoint, or when no checkpoint is set in scope (e.g. execution scope, where checkpoint
    /// queries return None to prevent temporal inconsistency).
    pub(crate) fn with_sequence_number(scope: Scope, sequence_number: Option<u64>) -> Option<Self> {
        let scope_checkpoint = scope.checkpoint_viewed_at()?;
        let sequence_number = sequence_number.unwrap_or(scope_checkpoint);

        (sequence_number <= scope_checkpoint).then_some(Self {
            scope,
            sequence_number,
        })
    }

    /// Paginate through checkpoints with filters applied.
    ///
    /// Returns empty results when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CCheckpoint>,
        filter: CheckpointFilter,
    ) -> Result<Connection<String, Checkpoint>, RpcError> {
        let watermarks: &Arc<Watermarks> = ctx.data()?;
        let available_range_key = AvailableRangeKey {
            type_: "Query".to_string(),
            field: Some("checkpoints".to_string()),
            filters: Some(filter.active_filters()),
        };
        let reader_lo = available_range_key.reader_lo(watermarks)?;

        let Some(cp_hi_inclusive) = scope.checkpoint_viewed_at() else {
            // In execution scope, checkpoint pagination returns empty results
            return Ok(Connection::new(false, false));
        };

        let Some(cp_bounds) = checkpoint_bounds(
            filter.after_checkpoint.map(u64::from),
            filter.at_checkpoint.map(u64::from),
            filter.before_checkpoint.map(u64::from),
            reader_lo,
            cp_hi_inclusive,
        ) else {
            return Ok(Connection::new(false, false));
        };

        let results = if let Some(epoch) = filter.at_epoch {
            cp_by_epoch(ctx, &page, &cp_bounds, epoch.into()).await?
        } else {
            cp_unfiltered(&cp_bounds, &page)
        };

        page.paginate_results(
            results,
            |c| JsonCursor::new(*c),
            |c| Ok(Self::with_sequence_number(scope.clone(), Some(c)).unwrap()),
        )
    }
}

impl CheckpointContents {
    /// Attempt to fill the contents. If the contents are already filled, returns a clone,
    /// otherwise attempts to fetch from the store. The resulting value may still have an empty
    /// contents field, because it could not be found in the store.
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        scope: Scope,
        sequence_number: u64,
    ) -> Result<Self, RpcError> {
        let kv_loader: &KvLoader = ctx.data()?;
        let contents = kv_loader
            .load_one_checkpoint(sequence_number)
            .await
            .context("Failed to fetch checkpoint contents")?;

        Ok(Self { scope, contents })
    }
}
