// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Object};
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_types::{
    crypto::AuthorityStrongQuorumSignInfo,
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

pub(crate) struct Checkpoint {
    pub(crate) sequence_number: u64,
    contents: CheckpointContents,
}

#[derive(Clone)]
struct CheckpointContents {
    scope: Scope,
    contents: Option<
        Arc<(
            CheckpointSummary,
            NativeCheckpointContents,
            AuthorityStrongQuorumSignInfo,
        )>,
    >,
}

/// Checkpoints contain finalized transactions and are used for node synchronization and global transaction ordering.
#[Object]
impl Checkpoint {
    /// The checkpoint's position in the total order of finalized checkpoints, agreed upon by consensus.
    async fn sequence_number(&self) -> UInt53 {
        self.sequence_number.into()
    }

    /// Query the RPC as if this checkpoint were the latest checkpoint.
    async fn query(&self) -> Option<Query> {
        let scope = Some(
            self.contents
                .scope
                .with_checkpoint_viewed_at(self.sequence_number),
        );

        Some(Query { scope })
    }

    #[graphql(flatten)]
    async fn contents(&self, ctx: &Context<'_>) -> Result<CheckpointContents, RpcError> {
        self.contents.fetch(ctx, self.sequence_number).await
    }
}

#[Object]
impl CheckpointContents {
    /// The timestamp at which the checkpoint is agreed to have happened according to consensus. Transactions that access time in this checkpoint will observe this timestamp.
    async fn timestamp(&self) -> Result<Option<DateTime>, RpcError> {
        let Some(contents) = &self.contents else {
            return Ok(None);
        };

        Ok(Some(DateTime::from_ms(contents.0.timestamp_ms as i64)?))
    }
}

impl Checkpoint {
    /// Construct a checkpoint that is represented by just its identifier (its sequence number).
    /// This does not check whether the checkpoint exists, so should not be used to "fetch" a
    /// checkpoint based on user input.
    pub(crate) fn with_sequence_number(scope: Scope, sequence_number: u64) -> Self {
        Self {
            sequence_number,
            contents: CheckpointContents::empty(scope),
        }
    }

    /// Return the checkpoint with the given sequence number, returns `None` if this checkpoint has
    /// not happened yet, according to the scope.
    pub(crate) async fn fetch(
        scope: Scope,
        sequence_number: UInt53,
    ) -> Result<Option<Self>, RpcError> {
        if u64::from(sequence_number) > scope.checkpoint_viewed_at() {
            return Ok(None);
        }

        Ok(Some(Self::with_sequence_number(
            scope,
            sequence_number.into(),
        )))
    }
}

impl CheckpointContents {
    fn empty(scope: Scope) -> Self {
        Self {
            scope,
            contents: None,
        }
    }

    /// Attempt to fill the contents. If the contents are already filled, returns a clone,
    /// otherwise attempts to fetch from the store. The resulting value may still have an empty
    /// contents field, because it could not be found in the store.
    pub(crate) async fn fetch(
        &self,
        ctx: &Context<'_>,
        sequence_number: u64,
    ) -> Result<Self, RpcError> {
        if self.contents.is_some() {
            return Ok(self.clone());
        }

        let kv_loader: &KvLoader = ctx.data()?;
        let Some(checkpoint) = kv_loader
            .load_one_checkpoint(sequence_number)
            .await
            .context("Failed to fetch checkpoint contents")?
        else {
            return Ok(self.clone());
        };

        if checkpoint.0.sequence_number > self.scope.checkpoint_viewed_at() {
            return Ok(self.clone());
        }

        Ok(Self {
            scope: self.scope.clone(),
            contents: Some(Arc::new(checkpoint)),
        })
    }
}
