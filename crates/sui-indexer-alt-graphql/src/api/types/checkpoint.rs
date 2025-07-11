// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::{Context, Object};
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_types::{
    crypto::AuthorityStrongQuorumSignInfo,
    message_envelope::Message,
    messages_checkpoint::{CheckpointContents as NativeCheckpointContents, CheckpointSummary},
};

use crate::{
    api::{
        query::Query,
        scalars::{date_time::DateTime, uint53::UInt53},
    },
    error::RpcError,
    scope::Scope,
};

use super::epoch::Epoch;

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
    /// A 32-byte hash that uniquely identifies the checkpoint, encoded in Base58. This is a hash of the checkpoint's summary.
    async fn digest(&self) -> Result<Option<String>, RpcError> {
        let Some((summary, _, _)) = &self.contents else {
            return Ok(None);
        };
        Ok(Some(summary.digest().base58_encode()))
    }

    /// The epoch that this checkpoint is part of.
    async fn epoch(&self) -> Option<Epoch> {
        let (summary, _, _) = self.contents.as_ref()?;
        Some(Epoch::with_id(self.scope.clone(), summary.epoch))
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

    /// The timestamp at which the checkpoint is agreed to have happened according to consensus. Transactions that access time in this checkpoint will observe this timestamp.
    async fn timestamp(&self) -> Result<Option<DateTime>, RpcError> {
        let Some((summary, _, _)) = &self.contents else {
            return Ok(None);
        };

        Ok(Some(DateTime::from_ms(summary.timestamp_ms as i64)?))
    }
}

impl Checkpoint {
    /// Construct a checkpoint that is represented by just its identifier (its sequence number).
    /// Returns `None` if the checkpoint is set in the future relative to the current scope's
    /// checkpoint.
    pub(crate) fn with_sequence_number(scope: Scope, sequence_number: u64) -> Option<Self> {
        (sequence_number <= scope.checkpoint_viewed_at()).then_some(Self {
            scope,
            sequence_number,
        })
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
