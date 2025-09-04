// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    dataloader::DataLoader,
    Context, Enum, Object,
};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer_alt_reader::{
    kv_loader::{KvLoader, TransactionContents as NativeTransactionContents},
    pg_reader::PgReader,
    tx_balance_changes::TxBalanceChangeKey,
};
use sui_indexer_alt_schema::transactions::BalanceChange as NativeBalanceChange;
use sui_rpc_api::client::TransactionExecutionResponse;
use sui_types::{
    digests::TransactionDigest,
    effects::TransactionEffectsAPI,
    execution_status::ExecutionStatus as NativeExecutionStatus,
    signature::GenericSignature,
    transaction::{TransactionData, TransactionDataAPI},
};

use crate::{
    api::scalars::{
        base64::Base64, cursor::JsonCursor, date_time::DateTime, digest::Digest, uint53::UInt53,
    },
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

use super::{
    balance_change::BalanceChange,
    checkpoint::Checkpoint,
    epoch::Epoch,
    event::Event,
    execution_error::ExecutionError,
    gas_effects::GasEffects,
    object_change::ObjectChange,
    transaction::{Transaction, TransactionContents},
    unchanged_consensus_object::UnchangedConsensusObject,
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
type CEvent = JsonCursor<usize>;
type CBalanceChange = JsonCursor<usize>;
type CUnchangedConsensusObject = JsonCursor<usize>;
type CDependency = JsonCursor<usize>;

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

        content
            .cp_sequence_number()
            .and_then(|cp| Checkpoint::with_sequence_number(self.scope.clone(), Some(cp)))
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

    /// Rich execution error information for failed transactions.
    async fn execution_error(&self) -> Result<Option<ExecutionError>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let effects = content.effects()?;
        let status = effects.status();

        // Extract programmable transaction if available
        let programmable_tx = content
            .data()
            .ok()
            .and_then(|tx_data| match tx_data.into_kind() {
                sui_types::transaction::TransactionKind::ProgrammableTransaction(tx) => Some(tx),
                _ => None,
            });

        ExecutionError::from_execution_status(&self.scope, status, programmable_tx.as_ref()).await
    }

    /// Timestamp corresponding to the checkpoint this transaction was finalized in.
    async fn timestamp(&self) -> Result<Option<DateTime>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        Ok(Some(DateTime::from_ms(content.timestamp_ms() as i64)?))
    }

    /// The epoch this transaction was finalized in.
    async fn epoch(&self) -> Result<Option<Epoch>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let effects = content.effects()?;
        Ok(Some(Epoch::with_id(
            self.scope.clone(),
            effects.executed_epoch(),
        )))
    }

    /// Events emitted by this transaction.
    async fn events(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CEvent>,
        last: Option<u64>,
        before: Option<CEvent>,
    ) -> Result<Option<Connection<String, Event>>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(Some(Connection::new(false, false)));
        };

        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("TransactionEffects", "events");
        let page = Page::from_params(limits, first, after, last, before)?;

        let events = content.events()?;
        let cursors = page.paginate_indices(events.len());
        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);

        let transaction_digest = content.digest()?;
        let timestamp_ms = content.timestamp_ms();

        for edge in cursors.edges {
            let event = Event {
                scope: self.scope.clone(),
                native: events[*edge.cursor].clone(),
                transaction_digest,
                sequence_number: *edge.cursor as u64,
                timestamp_ms,
            };

            conn.edges
                .push(Edge::new(edge.cursor.encode_cursor(), event));
        }

        Ok(Some(conn))
    }

    /// The effect this transaction had on the balances (sum of coin values per coin type) of addresses and objects.
    async fn balance_changes(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CBalanceChange>,
        last: Option<u64>,
        before: Option<CBalanceChange>,
    ) -> Result<Option<Connection<String, BalanceChange>>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(Some(Connection::new(false, false)));
        };

        let transaction_digest = content.digest()?;

        // Load balance changes from database using DataLoader
        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;
        let key = TxBalanceChangeKey(transaction_digest);

        let Some(stored_balance_changes) = pg_loader
            .load_one(key)
            .await
            .context("Failed to load balance changes")?
        else {
            return Ok(Some(Connection::new(false, false)));
        };

        // Deserialize balance changes from BCS bytes
        let balance_changes: Vec<NativeBalanceChange> =
            bcs::from_bytes(&stored_balance_changes.balance_changes)
                .context("Failed to deserialize balance changes")?;

        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("TransactionEffects", "balanceChanges");
        let page = Page::from_params(limits, first, after, last, before)?;

        let cursors = page.paginate_indices(balance_changes.len());
        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);

        for edge in cursors.edges {
            let balance_change = BalanceChange {
                scope: self.scope.clone(),
                stored: balance_changes[*edge.cursor].clone(),
            };
            conn.edges
                .push(Edge::new(edge.cursor.encode_cursor(), balance_change));
        }

        Ok(Some(conn))
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
    ) -> Result<Option<Connection<String, ObjectChange>>, RpcError> {
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

            conn.edges
                .push(Edge::new(edge.cursor.encode_cursor(), object_change))
        }

        Ok(Some(conn))
    }

    /// Effects related to the gas object used for the transaction (costs incurred and the identity of the smashed gas object returned).
    async fn gas_effects(&self) -> Result<Option<GasEffects>, RpcError> {
        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let effects = content.effects()?;
        Ok(Some(GasEffects::from_effects(self.scope.clone(), &effects)))
    }

    /// The unchanged consensus-managed objects that were referenced by this transaction.
    async fn unchanged_consensus_objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CUnchangedConsensusObject>,
        last: Option<u64>,
        before: Option<CUnchangedConsensusObject>,
    ) -> Result<Option<Connection<CUnchangedConsensusObject, UnchangedConsensusObject>>, RpcError>
    {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("TransactionEffects", "unchangedConsensusObjects");
        let page = Page::from_params(limits, first, after, last, before)?;

        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let unchanged_consensus_objects = content.effects()?.unchanged_consensus_objects();
        let cursors = page.paginate_indices(unchanged_consensus_objects.len());

        let effects = content.effects()?;
        let epoch = effects.executed_epoch();

        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);
        for edge in cursors.edges {
            let unchanged_consensus_object = UnchangedConsensusObject::from_native(
                self.scope.clone(),
                unchanged_consensus_objects[*edge.cursor].clone(),
                epoch,
            );
            conn.edges
                .push(Edge::new(edge.cursor, unchanged_consensus_object));
        }

        Ok(Some(conn))
    }

    /// Transactions whose outputs this transaction depends upon.
    async fn dependencies(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CDependency>,
        last: Option<u64>,
        before: Option<CDependency>,
    ) -> Result<Option<Connection<String, Transaction>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("TransactionEffects", "dependencies");
        let page = Page::from_params(limits, first, after, last, before)?;

        let Some(content) = &self.contents else {
            return Ok(None);
        };

        let effects = content.effects()?;
        let dependencies = effects.dependencies();
        let cursors = page.paginate_indices(dependencies.len());

        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);
        for edge in cursors.edges {
            let dependency_digest = dependencies[*edge.cursor];
            let transaction = Transaction::with_id(self.scope.clone(), dependency_digest);

            conn.edges
                .push(Edge::new(edge.cursor.encode_cursor(), transaction));
        }

        Ok(Some(conn))
    }
}

impl TransactionEffects {
    /// Create a new TransactionEffects from a TransactionExecutionResponse.
    pub(crate) fn from_execution_response(
        scope: Scope,
        response: TransactionExecutionResponse,
        transaction_data: TransactionData,
        signatures: Vec<GenericSignature>,
    ) -> Self {
        let digest = *response.effects.transaction_digest();
        let contents = NativeTransactionContents::ExecutedTransaction {
            effects: Box::new(response.effects),
            events: response.events.map(|events| events.data),
            transaction_data: Box::new(transaction_data),
            signatures,
        };

        Self {
            digest,
            contents: EffectsContents {
                scope,
                contents: Some(Arc::new(contents)),
            },
        }
    }

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
        let Some(checkpoint_viewed_at) = self.scope.checkpoint_viewed_at() else {
            return Ok(self.clone());
        };

        let kv_loader: &KvLoader = ctx.data()?;
        let Some(transaction) = kv_loader
            .load_one_transaction(digest)
            .await
            .context("Failed to fetch transaction contents")?
        else {
            return Ok(self.clone());
        };

        let cp_num = transaction
            .cp_sequence_number()
            .context("Fetched transaction should have checkpoint sequence number")?;

        if cp_num > checkpoint_viewed_at {
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
