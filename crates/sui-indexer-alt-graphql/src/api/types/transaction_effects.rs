// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Enum, Object, connection::Connection, dataloader::DataLoader};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer_alt_reader::{
    kv_loader::{KvLoader, TransactionContents as NativeTransactionContents},
    pg_reader::PgReader,
    tx_balance_changes::TxBalanceChangeKey,
};
use sui_indexer_alt_schema::transactions::BalanceChange as StoredBalanceChange;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
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
    async fn status(&self) -> Option<Result<ExecutionStatus, RpcError>> {
        let content = self.contents.as_ref()?;

        Some(
            content
                .effects()
                .map(|effects| match effects.status() {
                    NativeExecutionStatus::Success => ExecutionStatus::Success,
                    NativeExecutionStatus::Failure { .. } => ExecutionStatus::Failure,
                })
                .map_err(RpcError::from),
        )
    }

    /// The latest version of all objects (apart from packages) that have been created or modified by this transaction, immediately following this transaction.
    async fn lamport_version(&self) -> Option<Result<UInt53, RpcError>> {
        let content = self.contents.as_ref()?;

        Some(
            content
                .effects()
                .map(|effects| UInt53::from(effects.lamport_version().value()))
                .map_err(RpcError::from),
        )
    }

    /// Rich execution error information for failed transactions.
    async fn execution_error(&self) -> Option<Result<ExecutionError, RpcError>> {
        let content = self.contents.as_ref()?;

        let result = async {
            let effects = content.effects()?;
            let status = effects.status();

            // Extract programmable transaction if available
            let programmable_tx =
                content
                    .data()
                    .ok()
                    .and_then(|tx_data| match tx_data.into_kind() {
                        sui_types::transaction::TransactionKind::ProgrammableTransaction(tx) => {
                            Some(tx)
                        }
                        _ => None,
                    });

            ExecutionError::from_execution_status(&self.scope, status, programmable_tx.as_ref())
                .await
        }
        .await;

        result.transpose()
    }

    /// Timestamp corresponding to the checkpoint this transaction was finalized in.
    ///
    /// `null` for executed/simulated transactions that have not been included in a checkpoint.
    async fn timestamp(&self) -> Option<Result<DateTime, RpcError>> {
        let content = self.contents.as_ref()?;
        let timestamp_ms = content.timestamp_ms()?;
        Some(DateTime::from_ms(timestamp_ms as i64))
    }

    /// The epoch this transaction was finalized in.
    async fn epoch(&self) -> Option<Result<Epoch, RpcError>> {
        let content = self.contents.as_ref()?;

        Some(
            content
                .effects()
                .map(|effects| Epoch::with_id(self.scope.clone(), effects.executed_epoch()))
                .map_err(RpcError::from),
        )
    }

    /// Events emitted by this transaction.
    async fn events(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CEvent>,
        last: Option<u64>,
        before: Option<CEvent>,
    ) -> Option<Result<Connection<String, Event>, RpcError>> {
        let content = self.contents.as_ref()?;

        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("TransactionEffects", "events");
                let page = Page::from_params(limits, first, after, last, before)?;

                let events = content.events()?;
                page.paginate_indices(events.len(), |i| {
                    let transaction_digest = content.digest()?;
                    let timestamp_ms = content.timestamp_ms();

                    Ok(Event {
                        scope: self.scope.clone(),
                        native: events[i].clone(),
                        transaction_digest,
                        sequence_number: i as u64,
                        timestamp_ms,
                    })
                })
            }
            .await,
        )
    }

    /// The effect this transaction had on the balances (sum of coin values per coin type) of addresses and objects.
    async fn balance_changes(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CBalanceChange>,
        last: Option<u64>,
        before: Option<CBalanceChange>,
    ) -> Option<Result<Connection<String, BalanceChange>, RpcError>> {
        let content = self.contents.as_ref()?;

        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("TransactionEffects", "balanceChanges");
                let page = Page::from_params(limits, first, after, last, before)?;

                // First try to get balance changes from execution context (content)
                if let Some(grpc_balance_changes) = content.balance_changes() {
                    return page.paginate_indices(grpc_balance_changes.len(), |i| {
                        BalanceChange::from_grpc(self.scope.clone(), &grpc_balance_changes[i])
                    });
                }

                // Fall back to loading from database
                let transaction_digest = content.digest()?;
                let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;
                let key = TxBalanceChangeKey(transaction_digest);

                let Some(stored_balance_changes) = pg_loader
                    .load_one(key)
                    .await
                    .context("Failed to load balance changes")?
                else {
                    return Ok(Connection::new(false, false));
                };

                // Deserialize balance changes from BCS bytes
                let balance_changes: Vec<StoredBalanceChange> =
                    bcs::from_bytes(&stored_balance_changes.balance_changes)
                        .context("Failed to deserialize balance changes")?;

                page.paginate_indices(balance_changes.len(), |i| {
                    BalanceChange::from_stored(self.scope.clone(), balance_changes[i].clone())
                })
            }
            .await,
        )
    }

    /// The Base64-encoded BCS serialization of these effects, as `TransactionEffects`.
    async fn effects_bcs(&self) -> Option<Result<Base64, RpcError>> {
        let content = self.contents.as_ref()?;
        Some(content.raw_effects().map(Base64).map_err(RpcError::from))
    }

    /// A 32-byte hash that uniquely identifies the effects contents, encoded in Base58.
    async fn effects_digest(&self) -> Option<Result<String, RpcError>> {
        let content = self.contents.as_ref()?;
        Some(
            content
                .effects_digest()
                .map(Base58::encode)
                .map_err(RpcError::from),
        )
    }

    /// The before and after state of objects that were modified by this transaction.
    async fn object_changes(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CObjectChange>,
        last: Option<u64>,
        before: Option<CObjectChange>,
    ) -> Option<Result<Connection<String, ObjectChange>, RpcError>> {
        let content = self.contents.as_ref()?;

        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("TransactionEffects", "objectChanges");
                let page = Page::from_params(limits, first, after, last, before)?;

                let object_changes = content.effects()?.object_changes();
                page.paginate_indices(object_changes.len(), |i| {
                    Ok(ObjectChange {
                        scope: self.scope.clone(),
                        native: object_changes[i].clone(),
                    })
                })
            }
            .await,
        )
    }

    /// Effects related to the gas object used for the transaction (costs incurred and the identity of the smashed gas object returned).
    async fn gas_effects(&self) -> Option<Result<GasEffects, RpcError>> {
        let content = self.contents.as_ref()?;

        Some(
            content
                .effects()
                .map(|effects| GasEffects::from_effects(self.scope.clone(), &effects))
                .map_err(RpcError::from),
        )
    }

    /// The unchanged consensus-managed objects that were referenced by this transaction.
    async fn unchanged_consensus_objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CUnchangedConsensusObject>,
        last: Option<u64>,
        before: Option<CUnchangedConsensusObject>,
    ) -> Option<Result<Connection<String, UnchangedConsensusObject>, RpcError>> {
        let content = self.contents.as_ref()?;

        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("TransactionEffects", "unchangedConsensusObjects");
                let page = Page::from_params(limits, first, after, last, before)?;

                let epoch = content.effects()?.executed_epoch();
                let unchanged_consensus_objects = content.effects()?.unchanged_consensus_objects();
                page.paginate_indices(unchanged_consensus_objects.len(), |i| {
                    Ok(UnchangedConsensusObject::from_native(
                        self.scope.clone(),
                        unchanged_consensus_objects[i].clone(),
                        epoch,
                    ))
                })
            }
            .await,
        )
    }

    /// Transactions whose outputs this transaction depends upon.
    async fn dependencies(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CDependency>,
        last: Option<u64>,
        before: Option<CDependency>,
    ) -> Option<Result<Connection<String, Transaction>, RpcError>> {
        let content = self.contents.as_ref()?;

        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("TransactionEffects", "dependencies");
                let page = Page::from_params(limits, first, after, last, before)?;

                let effects = content.effects()?;
                let dependencies = effects.dependencies();
                page.paginate_indices(dependencies.len(), |i| {
                    Ok(Transaction::with_id(self.scope.clone(), dependencies[i]))
                })
            }
            .await,
        )
    }
}

impl TransactionEffects {
    /// Create a new TransactionEffects from an ExecutedTransaction.
    pub(crate) fn from_executed_transaction(
        scope: Scope,
        executed_transaction: &ExecutedTransaction,
        transaction_data: TransactionData,
        signatures: Vec<GenericSignature>,
    ) -> Result<Self, RpcError> {
        let contents = NativeTransactionContents::from_executed_transaction(
            executed_transaction,
            transaction_data,
            signatures,
        )
        .context("Failed to create TransactionContents from ExecutedTransaction")?;

        let digest = contents
            .digest()
            .context("Failed to get digest from transaction contents")?;

        Ok(Self {
            digest,
            contents: EffectsContents {
                scope,
                contents: Some(Arc::new(contents)),
            },
        })
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
