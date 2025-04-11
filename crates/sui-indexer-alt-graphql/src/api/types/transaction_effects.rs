// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Object};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer_alt_reader::kv_loader::{KvLoader, TransactionContents};
use sui_types::digests::TransactionDigest;

use crate::{
    api::scalars::{base64::Base64, digest::Digest},
    error::RpcError,
};

pub(crate) struct TransactionEffects {
    digest: TransactionDigest,
    contents: EffectsContents,
}

#[derive(Clone)]
pub(crate) struct EffectsContents(Option<Arc<TransactionContents>>);

/// The results of executing a transaction.
#[Object]
impl TransactionEffects {
    /// A 32-byte hash that uniquely identifies the transaction contents, encoded in Base58.
    ///
    /// Note that this is different from the execution digest, which is the unique hash of the transaction effects.
    async fn digest(&self) -> String {
        Base58::encode(self.digest)
    }

    #[graphql(flatten)]
    async fn contents(&self, ctx: &Context<'_>) -> Result<EffectsContents, RpcError> {
        Ok(if self.contents.0.is_some() {
            self.contents.clone()
        } else {
            EffectsContents::fetch(ctx, self.digest).await?
        })
    }
}

#[Object]
impl EffectsContents {
    /// The Base64-encoded BCS serialization of these effects, as `TransactionEffects`.
    async fn effects_bcs(&self) -> Result<Option<Base64>, RpcError> {
        let Some(content) = &self.0 else {
            return Ok(None);
        };

        Ok(Some(Base64(content.raw_effects()?)))
    }
}

impl TransactionEffects {
    /// Load the effects from the store, and return it fully inflated (with contents already
    /// fetched). Returns `None` if the effects do not exist (either never existed or were pruned
    /// from the store).
    pub(crate) async fn fetch(ctx: &Context<'_>, digest: Digest) -> Result<Option<Self>, RpcError> {
        let contents = EffectsContents::fetch(ctx, digest.into()).await?;
        let Some(tx) = &contents.0 else {
            return Ok(None);
        };

        Ok(Some(TransactionEffects {
            digest: tx.digest()?,
            contents,
        }))
    }
}

impl EffectsContents {
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        digest: TransactionDigest,
    ) -> Result<Self, RpcError> {
        let kv_loader: &KvLoader = ctx.data()?;

        let transaction = kv_loader
            .load_one_transaction(digest)
            .await
            .context("Failed to fetch transaction contents")?;

        Ok(Self(transaction.map(Arc::new)))
    }
}
