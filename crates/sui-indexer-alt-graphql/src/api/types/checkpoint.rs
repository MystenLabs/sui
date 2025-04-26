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
    api::scalars::{date_time::DateTime, uint53::UInt53},
    error::RpcError,
};

pub(crate) struct Checkpoint {
    pub(crate) sequence_number: u64,
    contents: CheckpointContents,
}

#[derive(Clone)]
struct CheckpointContents(
    Option<
        Arc<(
            CheckpointSummary,
            NativeCheckpointContents,
            AuthorityStrongQuorumSignInfo,
        )>,
    >,
);

/// Checkpoints contain finalized transactions and are used for node synchronization and global transaction ordering.
#[Object]
impl Checkpoint {
    /// The checkpoint's position in the total order of finalized checkpoints, agreed upon by consensus.
    async fn sequence_number(&self) -> UInt53 {
        self.sequence_number.into()
    }

    #[graphql(flatten)]
    async fn contents(&self, ctx: &Context<'_>) -> Result<CheckpointContents, RpcError> {
        Ok(if self.contents.0.is_some() {
            self.contents.clone()
        } else {
            CheckpointContents::fetch(ctx, self.sequence_number).await?
        })
    }
}

#[Object]
impl CheckpointContents {
    /// The timestamp at which the checkpoint is agreed to have happened according to consensus. Transactions that access time in this checkpoint will observe this timestamp.
    async fn timestamp(&self) -> Result<Option<DateTime>, RpcError> {
        let Some(contents) = &self.0 else {
            return Ok(None);
        };

        Ok(Some(DateTime::from_ms(contents.0.timestamp_ms as i64)?))
    }
}

impl Checkpoint {
    /// Construct a checkpoint that is represented by just its identifier (its sequence number).
    /// This does not check whether the checkpoint exists, so should not be used to "fetch" a
    /// checkpoint based on user input.
    pub(crate) fn with_sequence_number(sequence_number: u64) -> Self {
        Self {
            sequence_number,
            contents: CheckpointContents(None),
        }
    }

    /// Load the checkpoint from the store, and return it fully inflated (with contents already
    /// fetched). Returns `None` if the checkpoint does not exist (either never existed or was
    /// pruned from the store).
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        sequence_number: UInt53,
    ) -> Result<Option<Self>, RpcError> {
        let contents = CheckpointContents::fetch(ctx, sequence_number.into()).await?;
        let Some(cp) = &contents.0 else {
            return Ok(None);
        };

        Ok(Some(Checkpoint {
            sequence_number: cp.0.sequence_number,
            contents: CheckpointContents(Some(cp.clone())),
        }))
    }
}

impl CheckpointContents {
    pub(crate) async fn fetch(ctx: &Context<'_>, sequence_number: u64) -> Result<Self, RpcError> {
        let kv_loader: &KvLoader = ctx.data()?;

        let checkpoint = kv_loader
            .load_one_checkpoint(sequence_number)
            .await
            .context("Failed to fetch checkpoint contents")?;

        Ok(Self(checkpoint.map(Arc::new)))
    }
}
