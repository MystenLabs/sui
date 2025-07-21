// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{
    connection::{Connection, Edge},
    Context, Enum, Object,
};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer_alt_reader::kv_loader::{
    KvLoader, TransactionContents as NativeTransactionContents,
};
use sui_types::{
    digests::TransactionDigest, effects::TransactionEffectsAPI,
    execution_status::ExecutionStatus as NativeExecutionStatus,
};

use crate::{
    api::scalars::{base64::Base64, cursor::JsonCursor, digest::Digest, uint53::UInt53},
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

use super::{
    checkpoint::Checkpoint,
    object_change::ObjectChange,
    transaction::{Transaction, TransactionContents},
};

/// The execution status of this transaction: success or failure.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum ExecutionStatus {
    /// The transaction was successfully executed.
    Success,
    /// The transaction could not be executed.
    Failure,
}

#[derive(Clone)]
pub(crate) struct TransactionEffects {
    pub(crate) digest: TransactionDigest,
    pub(crate) contents: EffectsContents,
}

#[derive(Clone)]
pub(crate) struct EffectsContents {
    pub(crate) scope: Scope,
    pub(crate) contents: Option<Arc<NativeTransactionContents>>,
}

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
        self.contents.fetch(ctx, self.digest).await
    }
}

#[Object]
impl EffectsContents {
    /// The checkpoint this transaction was finalized in.
    async fn checkpoint(&self) -> Option<Checkpoint> {
        let Some(content) = &self.contents else {
            return None;
        };

        Checkpoint::with_sequence_number(self.scope.clone(), content.cp_sequence_number())
    }

    /// Whether the transaction executed successfully or not.
    async fn status(&self) -> Result<Option<ExecutionStatus>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let effects = content.effects()?;
        let status = match effects.status() {
            NativeExecutionStatus::Success => ExecutionStatus::Success,
            NativeExecutionStatus::Failure { .. } => ExecutionStatus::Failure,
        };

        Ok(Some(status))
    }

    /// The latest version of all objects (apart from packages) that have been created or modified by this transaction, immediately following this transaction.
    async fn lamport_version(&self) -> Result<Option<UInt53>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let effects = content.effects()?;
        Ok(Some(UInt53::from(effects.lamport_version().value())))
    }

    /// The Base64-encoded BCS serialization of these effects, as `TransactionEffects`.
    async fn effects_bcs(&self) -> Result<Option<Base64>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        Ok(Some(Base64(content.raw_effects()?)))
    }

    /// A 32-byte hash that uniquely identifies the effects contents, encoded in Base58.
    async fn effects_digest(&self) -> Result<Option<String>, RpcError> {
        let Some(content) = &self.contents else {
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

        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let object_changes = content.effects()?.object_changes();
        let cursors = page.paginate_indices(object_changes.len());

        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);
        for edge in cursors.edges {
            let object_change = ObjectChange {
                scope: self.scope.clone(),
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
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        scope: Scope,
        digest: Digest,
    ) -> Result<Option<Self>, RpcError> {
        let contents = EffectsContents::empty(scope)
            .fetch(ctx, digest.into())
            .await?;

        let Some(tx) = &contents.contents else {
            return Ok(None);
        };

        Ok(Some(Self {
            digest: tx.digest()?,
            contents,
        }))
    }
}

impl EffectsContents {
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
        digest: TransactionDigest,
    ) -> Result<Self, RpcError> {
        if self.contents.is_some() {
            return Ok(self.clone());
        }

        let kv_loader: &KvLoader = ctx.data()?;
        let Some(transaction) = kv_loader
            .load_one_transaction(digest)
            .await
            .context("Failed to fetch transaction contents")?
        else {
            return Ok(self.clone());
        };

        // Discard the loaded result if we are viewing it at a checkpoint before it existed.
        if transaction.cp_sequence_number() > self.scope.checkpoint_viewed_at() {
            return Ok(self.clone());
        }

        Ok(Self {
            scope: self.scope.clone(),
            contents: Some(Arc::new(transaction)),
        })
    }
}

impl From<Transaction> for TransactionEffects {
    fn from(tx: Transaction) -> Self {
        let TransactionContents { scope, contents } = tx.contents;

        Self {
            digest: tx.digest,
            contents: EffectsContents { scope, contents },
        }
    }
}
