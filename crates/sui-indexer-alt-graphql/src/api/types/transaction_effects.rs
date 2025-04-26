// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{
    connection::{Connection, Edge},
    Context, Object,
};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer_alt_reader::kv_loader::{KvLoader, TransactionContents};
use sui_types::{digests::TransactionDigest, effects::TransactionEffectsAPI};

use crate::{
    api::scalars::{base64::Base64, cursor::JsonCursor, digest::Digest},
    error::RpcError,
    pagination::{Page, PaginationConfig},
};

use super::{checkpoint::Checkpoint, object_change::ObjectChange, transaction::Transaction};

#[derive(Clone)]
pub(crate) struct TransactionEffects {
    pub digest: TransactionDigest,
    pub contents: EffectsContents,
}

#[derive(Clone)]
pub(crate) struct EffectsContents(pub Option<Arc<TransactionContents>>);

type CObjectChange = JsonCursor<usize>;

/// The results of executing a transaction.
#[Object]
impl TransactionEffects {
    /// A 32-byte hash that uniquely identifies the transaction contents, encoded in Base58.
    ///
    /// Note that this is different from the execution digest, which is the unique hash of the transaction effects.
    async fn digest(&self) -> String {
        Base58::encode(self.digest)
    }

    /// The transaction that ran to produce these effects.
    async fn transaction(&self) -> Option<Transaction> {
        Some(Transaction::from(self.clone()))
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
    /// The checkpoint this transaction was finalized in.
    async fn checkpoint(&self) -> Option<Checkpoint> {
        let Some(content) = &self.0 else {
            return None;
        };

        Some(Checkpoint::with_sequence_number(
            content.cp_sequence_number(),
        ))
    }

    /// The Base64-encoded BCS serialization of these effects, as `TransactionEffects`.
    async fn effects_bcs(&self) -> Result<Option<Base64>, RpcError> {
        let Some(content) = &self.0 else {
            return Ok(None);
        };

        Ok(Some(Base64(content.raw_effects()?)))
    }

    /// A 32-byte hash that uniquely identifies the effects contents, encoded in Base58.
    async fn effects_digest(&self) -> Result<Option<String>, RpcError> {
        let Some(content) = &self.0 else {
            return Ok(None);
        };

        Ok(Some(Base58::encode(content.effects_digest()?)))
    }

    /// The before and after state of objects that were modified by this transaction.
    async fn object_changes(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CObjectChange>,
        last: Option<u64>,
        before: Option<CObjectChange>,
    ) -> Result<Option<Connection<CObjectChange, ObjectChange>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("TransactionEffects", "objectChanges");
        let page = Page::from_params(limits, first, after, last, before)?;

        let Some(content) = &self.0 else {
            return Ok(None);
        };

        let object_changes = content.effects()?.object_changes();
        let cursors = page.paginate_indices(object_changes.len());

        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);
        for edge in cursors.edges {
            let object_change = ObjectChange {
                native: object_changes[*edge.cursor].clone(),
            };

            conn.edges.push(Edge::new(edge.cursor, object_change))
        }

        Ok(Some(conn))
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

impl From<Transaction> for TransactionEffects {
    fn from(tx: Transaction) -> Self {
        Self {
            digest: tx.digest,
            contents: EffectsContents(tx.contents.0),
        }
    }
}
