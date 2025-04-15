// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Object};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer_alt_reader::kv_loader::{
    KvLoader, TransactionContents as NativeTransactionContents,
};
use sui_types::digests::TransactionDigest;

use crate::{
    api::scalars::{base64::Base64, digest::Digest},
    error::RpcError,
};

use super::transaction_effects::TransactionEffects;

#[derive(Clone)]
pub(crate) struct Transaction {
    pub digest: TransactionDigest,
    pub contents: TransactionContents,
}

#[derive(Clone)]
pub(crate) struct TransactionContents(pub Option<Arc<NativeTransactionContents>>);

/// Description of a transaction, the unit of activity on Sui.
#[Object]
impl Transaction {
    /// A 32-byte hash that uniquely identifies the transaction contents, encoded in Base58.
    async fn digest(&self) -> String {
        Base58::encode(self.digest)
    }

    /// The results to the chain of executing this transaction.
    async fn effects(&self) -> Option<TransactionEffects> {
        Some(TransactionEffects::from(self.clone()))
    }

    #[graphql(flatten)]
    async fn contents(&self, ctx: &Context<'_>) -> Result<TransactionContents, RpcError> {
        Ok(if self.contents.0.is_some() {
            self.contents.clone()
        } else {
            TransactionContents::fetch(ctx, self.digest).await?
        })
    }
}

#[Object]
impl TransactionContents {
    /// The Base64-encoded BCS serialization of this transaction, as a `TransactionData`.
    async fn transaction_bcs(&self) -> Result<Option<Base64>, RpcError> {
        let Some(content) = &self.0 else {
            return Ok(None);
        };

        Ok(Some(Base64(content.raw_transaction()?)))
    }
}

impl Transaction {
    /// Construct a transaction that is represented by just its identifier (its transaction
    /// digest). This does not check whether the transaction exists, so should not be used to
    /// "fetch" a transaction based on a digest provided as user input.
    #[allow(dead_code)] // TODO: Remove once this is used in Object.previousTransaction
    pub(crate) fn with_id(digest: TransactionDigest) -> Self {
        Self {
            digest,
            contents: TransactionContents(None),
        }
    }

    /// Load the transaction from the store, and return it fully inflated (with contents already
    /// fetched). Returns `None` if the transaction does not exist (either never existed or was
    /// pruned from the store).
    pub(crate) async fn fetch(ctx: &Context<'_>, digest: Digest) -> Result<Option<Self>, RpcError> {
        let contents = TransactionContents::fetch(ctx, digest.into()).await?;
        let Some(tx) = &contents.0 else {
            return Ok(None);
        };

        Ok(Some(Transaction {
            digest: tx.digest()?,
            contents,
        }))
    }
}

impl TransactionContents {
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

impl From<TransactionEffects> for Transaction {
    fn from(fx: TransactionEffects) -> Self {
        Self {
            digest: fx.digest,
            contents: TransactionContents(fx.contents.0),
        }
    }
}
