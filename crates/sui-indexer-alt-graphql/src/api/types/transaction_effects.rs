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
use sui_types::{
    digests::TransactionDigest, effects::TransactionEffectsAPI,
    execution_status::ExecutionStatus as NativeExecutionStatus, message_envelope::Message,
    transaction::TransactionDataAPI,
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
    pub(crate) data_source: EffectsDataSource,
}

#[derive(Clone)]
pub(crate) enum EffectsDataSource {
    /// Transaction indexed and stored in KV store
    Stored {
        native: Option<Box<sui_types::effects::TransactionEffects>>,
        contents: Option<Arc<NativeTransactionContents>>,
    },

    /// Transaction just executed via gRPC, not yet indexed
    ExecutedTransaction {
        native: Box<sui_types::effects::TransactionEffects>,
        events: Option<Vec<sui_types::event::Event>>,
        transaction_data: Box<sui_types::transaction::TransactionData>,
    },
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
        let content = self.stored_contents()?;

        Checkpoint::with_sequence_number(self.scope.clone(), content.cp_sequence_number())
    }

    /// Whether the transaction executed successfully or not.
    async fn status(&self) -> Result<Option<ExecutionStatus>, RpcError> {
        let Some(effects) = self.native() else {
            return Ok(None);
        };

        let status = match effects.status() {
            NativeExecutionStatus::Success => ExecutionStatus::Success,
            NativeExecutionStatus::Failure { .. } => ExecutionStatus::Failure,
        };

        Ok(Some(status))
    }

    /// The latest version of all objects (apart from packages) that have been created or modified by this transaction, immediately following this transaction.
    async fn lamport_version(&self) -> Result<Option<UInt53>, RpcError> {
        let Some(effects) = self.native() else {
            return Ok(None);
        };

        Ok(Some(UInt53::from(effects.lamport_version().value())))
    }

    /// Rich execution error information for failed transactions.
    async fn execution_error(&self) -> Result<Option<ExecutionError>, RpcError> {
        let Some(effects) = self.native() else {
            return Ok(None);
        };

        let status = effects.status();
        let programmable_tx = self.programmable_transaction();

        ExecutionError::from_execution_status(&self.scope, status, programmable_tx.as_ref()).await
    }

    /// Timestamp corresponding to the checkpoint this transaction was finalized in.
    async fn timestamp(&self) -> Result<Option<DateTime>, RpcError> {
        let Some(content) = self.stored_contents() else {
            return Ok(None);
        };

        Ok(Some(DateTime::from_ms(content.timestamp_ms() as i64)?))
    }

    /// The epoch this transaction was finalized in.
    async fn epoch(&self) -> Result<Option<Epoch>, RpcError> {
        let Some(effects) = self.native() else {
            return Ok(None);
        };
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
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("TransactionEffects", "events");
        let page = Page::from_params(limits, first, after, last, before)?;

        // Get events length - return empty connection if data not loaded yet
        let Some(events_len) = self.events_len()? else {
            return Ok(Some(Connection::new(false, false)));
        };

        let cursors = page.paginate_indices(events_len);
        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);

        // Process events from either data source
        for edge in cursors.edges {
            if let Some(event) = Event::from_effects_data_source(
                self.scope.clone(),
                &self.data_source,
                *edge.cursor,
            )? {
                conn.edges
                    .push(Edge::new(edge.cursor.encode_cursor(), event));
            }
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
        let Some(content) = self.stored_contents() else {
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
        let bytes = match &self.data_source {
            EffectsDataSource::Stored {
                contents: Some(content),
                ..
            } => content.raw_effects()?,
            EffectsDataSource::ExecutedTransaction { native, .. } => {
                bcs::to_bytes(native.as_ref()).context("Error serializing transaction effects")?
            }
            _ => return Ok(None),
        };

        Ok(Some(Base64(bytes)))
    }

    /// A 32-byte hash that uniquely identifies the effects contents, encoded in Base58.
    async fn effects_digest(&self) -> Result<Option<String>, RpcError> {
        let Some(effects) = self.native() else {
            return Ok(None);
        };

        Ok(Some(Base58::encode(effects.digest())))
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

        let Some(effects) = self.native() else {
            return Ok(None);
        };

        let object_changes = effects.object_changes();
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

    /// Effects related to the gas object used for the transaction (costs incurred and the identity of the smashed gas object returned).
    async fn gas_effects(&self) -> Result<Option<GasEffects>, RpcError> {
        let Some(effects) = self.native() else {
            return Ok(None);
        };
        Ok(Some(GasEffects::from_effects(self.scope.clone(), effects)))
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

        let Some(effects) = self.native() else {
            return Ok(None);
        };
        let Some(content) = self.stored_contents() else {
            return Ok(None);
        };

        let unchanged_consensus_objects = effects.unchanged_consensus_objects();
        let cursors = page.paginate_indices(unchanged_consensus_objects.len());

        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);
        for edge in cursors.edges {
            let execution_checkpoint = content.cp_sequence_number();
            let unchanged_consensus_object = UnchangedConsensusObject::from_native(
                self.scope.clone(),
                unchanged_consensus_objects[*edge.cursor].clone(),
                execution_checkpoint,
            );

            conn.edges
                .push(Edge::new(edge.cursor, unchanged_consensus_object))
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

        let Some(effects) = self.native() else {
            return Ok(None);
        };
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

        let EffectsDataSource::Stored {
            contents: Some(tx), ..
        } = &contents.data_source
        else {
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
            data_source: EffectsDataSource::Stored {
                native: None, // No dummy data - explicitly None until loaded
                contents: None,
            },
        }
    }

    /// Helper method like old GraphQL's native() - returns effects from any source
    fn native(&self) -> Option<&sui_types::effects::TransactionEffects> {
        match &self.data_source {
            EffectsDataSource::Stored {
                native: Some(n), ..
            } => Some(n.as_ref()),
            EffectsDataSource::Stored { native: None, .. } => None,
            EffectsDataSource::ExecutedTransaction { native, .. } => Some(native.as_ref()),
        }
    }

    /// Helper method to get stored contents only (returns None for fresh execution)
    fn stored_contents(&self) -> Option<&Arc<NativeTransactionContents>> {
        match &self.data_source {
            EffectsDataSource::Stored { contents, .. } => contents.as_ref(),
            EffectsDataSource::ExecutedTransaction { .. } => None,
        }
    }

    /// Helper method to get events length from any data source
    fn events_len(&self) -> Result<Option<usize>, RpcError> {
        match &self.data_source {
            EffectsDataSource::Stored {
                contents: Some(content),
                ..
            } => Ok(Some(content.events()?.len())),
            EffectsDataSource::Stored { contents: None, .. } => Ok(None), // Data not loaded yet
            EffectsDataSource::ExecutedTransaction {
                events: Some(events),
                ..
            } => Ok(Some(events.len())),
            EffectsDataSource::ExecutedTransaction { events: None, .. } => Ok(Some(0)), // No events emitted
        }
    }

    /// Helper method to extract programmable transaction from any data source
    fn programmable_transaction(&self) -> Option<sui_types::transaction::ProgrammableTransaction> {
        match &self.data_source {
            EffectsDataSource::Stored {
                contents: Some(content),
                ..
            } => content
                .data()
                .ok()
                .and_then(|tx_data| match tx_data.into_kind() {
                    sui_types::transaction::TransactionKind::ProgrammableTransaction(tx) => {
                        Some(tx)
                    }
                    _ => None,
                }),
            EffectsDataSource::ExecutedTransaction {
                transaction_data, ..
            } => match transaction_data.as_ref().kind() {
                sui_types::transaction::TransactionKind::ProgrammableTransaction(tx) => {
                    Some(tx.clone())
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Attempt to fill the contents. If the contents are already filled, returns a clone,
    /// otherwise attempts to fetch from the store. The resulting value may still have an empty
    /// contents field, because it could not be found in the store.
    ///
    /// Returns self unchanged for all data source types except `Stored` without contents.
    pub(crate) async fn fetch(
        &self,
        ctx: &Context<'_>,
        digest: TransactionDigest,
    ) -> Result<Self, RpcError> {
        // Only fetch if we have Stored data without contents
        if !matches!(
            &self.data_source,
            EffectsDataSource::Stored { contents: None, .. }
        ) {
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

        let effects = transaction.effects()?;
        Ok(Self {
            scope: self.scope.clone(),
            data_source: EffectsDataSource::Stored {
                native: Some(Box::new(effects)),
                contents: Some(Arc::new(transaction)),
            },
        })
    }
}

impl From<Transaction> for TransactionEffects {
    fn from(tx: Transaction) -> Self {
        let TransactionContents { scope, contents } = tx.contents;

        // TODO: Handle ExecutedTransaction for Transaction type.
        let native = match contents {
            Some(ref content) => content.effects().ok().map(Box::new),
            None => None,
        };

        Self {
            digest: tx.digest,
            contents: EffectsContents {
                scope,
                data_source: EffectsDataSource::Stored { native, contents },
            },
        }
    }
}
