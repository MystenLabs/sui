// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use async_graphql::{
    connection::{Connection, ConnectionNameType, CursorType, Edge, EdgeNameType, EmptyFields},
    *,
};
use sui_indexer::models_v2::transactions::StoredTransaction;
use sui_types::{
    effects::{TransactionEffects as NativeTransactionEffects, TransactionEffectsAPI},
    event::Event as NativeEvent,
    execution_status::ExecutionStatus as NativeExecutionStatus,
    transaction::SenderSignedData as NativeSenderSignedData,
    transaction::TransactionData as NativeTransactionData,
};

use super::{
    balance_change::BalanceChange,
    base64::Base64,
    checkpoint::{Checkpoint, CheckpointId},
    cursor::{JsonCursor, Page},
    date_time::DateTime,
    digest::Digest,
    epoch::Epoch,
    event::Event,
    gas::GasEffects,
    object_change::ObjectChange,
    transaction_block::TransactionBlock,
    unchanged_shared_object::UnchangedSharedObject,
};

#[derive(Clone)]
pub(crate) enum TransactionBlockEffects {
    /// A transaction that has been indexed and stored in the database,
    /// containing all information that the other two variants have, and more.
    Stored {
        stored_tx: StoredTransaction,
        native: NativeTransactionEffects,
    },
    /// A transaction block that has been executed via executeTransactionBlock
    /// but not yet indexed. So it does not contain checkpoint, timestamp or balanceChanges.
    Executed {
        tx_data: NativeSenderSignedData,
        native: NativeTransactionEffects,
        events: Vec<NativeEvent>,
    },
    /// A transaction block that has been executed via dryRunTransactionBlock. Similar to
    /// Executed, it does not contain checkpoint, timestamp or balanceChanges.
    DryRun {
        tx_data: NativeTransactionData,
        native: NativeTransactionEffects,
        events: Vec<NativeEvent>,
    },
}

/// The execution status of this transaction block: success or failure.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum ExecutionStatus {
    /// The transaction block was successfully executed
    Success,
    /// The transaction block could not be executed
    Failure,
}

/// Type to override names of the Dependencies Connection (which has nullable transactions and
/// therefore must be a different types to the default `TransactionBlockConnection`).
struct DependencyConnectionNames;

type CDependencies = JsonCursor<usize>;
type CUnchangedSharedObject = JsonCursor<usize>;
type CObjectChange = JsonCursor<usize>;
type CBalanceChange = JsonCursor<usize>;
type CEvent = JsonCursor<usize>;

/// The effects representing the result of executing a transaction block.
#[Object]
impl TransactionBlockEffects {
    /// The transaction that ran to produce these effects.
    async fn transaction_block(&self) -> Result<Option<TransactionBlock>> {
        Ok(Some(self.clone().try_into().extend()?))
    }

    /// Whether the transaction executed successfully or not.
    async fn status(&self) -> Option<ExecutionStatus> {
        Some(match self.native().status() {
            NativeExecutionStatus::Success => ExecutionStatus::Success,
            NativeExecutionStatus::Failure { .. } => ExecutionStatus::Failure,
        })
    }

    /// The latest version of all objects (apart from packages) that have been created or modified
    /// by this transaction, immediately following this transaction.
    async fn lamport_version(&self) -> u64 {
        self.native().lamport_version().value()
    }

    /// The reason for a transaction failure, if it did fail.
    async fn errors(&self) -> Option<String> {
        match self.native().status() {
            NativeExecutionStatus::Success => None,

            NativeExecutionStatus::Failure {
                error,
                command: None,
            } => Some(error.to_string()),

            NativeExecutionStatus::Failure {
                error,
                command: Some(command),
            } => {
                // Convert the command index into an ordinal.
                let command = command + 1;
                let suffix = match command % 10 {
                    1 => "st",
                    2 => "nd",
                    3 => "rd",
                    _ => "th",
                };

                Some(format!("{error} in {command}{suffix} command."))
            }
        }
    }

    /// Transactions whose outputs this transaction depends upon.
    async fn dependencies(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CDependencies>,
        last: Option<u64>,
        before: Option<CDependencies>,
    ) -> Result<
        Connection<
            String,
            Option<TransactionBlock>,
            EmptyFields,
            EmptyFields,
            DependencyConnectionNames,
            DependencyConnectionNames,
        >,
    > {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let mut connection = Connection::new(false, false);

        let dependencies = self.native().dependencies();

        let Some((prev, next, cs)) = page.paginate_indices(dependencies.len()) else {
            return Ok(connection);
        };

        let indices: Vec<CDependencies> = cs.collect();

        let (Some(fst), Some(lst)) = (indices.first(), indices.last()) else {
            return Ok(connection);
        };

        let transactions = TransactionBlock::multi_query(
            ctx.data_unchecked(),
            dependencies[**fst..=**lst]
                .iter()
                .map(|d| Digest::from(*d))
                .collect(),
        )
        .await
        .extend()?;

        if transactions.is_empty() {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for idx in indices {
            let digest: Digest = dependencies[*idx].into();
            connection.edges.push(Edge::new(
                idx.encode_cursor(),
                transactions.get(&digest).cloned(),
            ));
        }

        Ok(connection)
    }

    /// Effects to the gas object.
    async fn gas_effects(&self) -> Option<GasEffects> {
        Some(GasEffects::from(self.native()))
    }

    /// Shared objects that are referenced by but not changed by this transaction.
    async fn unchanged_shared_objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CUnchangedSharedObject>,
        last: Option<u64>,
        before: Option<CUnchangedSharedObject>,
    ) -> Result<Connection<String, UnchangedSharedObject>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let mut connection = Connection::new(false, false);

        let input_shared_objects = self.native().input_shared_objects();

        let Some((prev, next, cs)) = page.paginate_indices(input_shared_objects.len()) else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let result = UnchangedSharedObject::try_from(input_shared_objects[*c].clone());
            match result {
                Ok(unchanged_shared_object) => {
                    connection
                        .edges
                        .push(Edge::new(c.encode_cursor(), unchanged_shared_object));
                }
                Err(_shared_object_changed) => continue, // Only add unchanged shared objects to the connection.
            }
        }

        Ok(connection)
    }

    /// The effect this transaction had on objects on-chain.
    async fn object_changes(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CObjectChange>,
        last: Option<u64>,
        before: Option<CObjectChange>,
    ) -> Result<Connection<String, ObjectChange>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let mut connection = Connection::new(false, false);

        let object_changes = self.native().object_changes();

        let Some((prev, next, cs)) = page.paginate_indices(object_changes.len()) else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let object_change = ObjectChange {
                native: object_changes[*c].clone(),
            };

            connection
                .edges
                .push(Edge::new(c.encode_cursor(), object_change));
        }

        Ok(connection)
    }

    /// The effect this transaction had on the balances (sum of coin values per coin type) of
    /// addresses and objects.
    async fn balance_changes(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CBalanceChange>,
        last: Option<u64>,
        before: Option<CBalanceChange>,
    ) -> Result<Connection<String, BalanceChange>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let mut connection = Connection::new(false, false);

        let Self::Stored { stored_tx, .. } = self else {
            return Ok(connection);
        };

        let Some((prev, next, cs)) = page.paginate_indices(stored_tx.balance_changes.len()) else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let Some(serialized) = &stored_tx.balance_changes[*c] else {
                continue;
            };

            let balance_change = BalanceChange::read(serialized).extend()?;
            connection
                .edges
                .push(Edge::new(c.encode_cursor(), balance_change));
        }

        Ok(connection)
    }

    /// Events emitted by this transaction block.
    async fn events(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CEvent>,
        last: Option<u64>,
        before: Option<CEvent>,
    ) -> Result<Connection<String, Event>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let mut connection = Connection::new(false, false);
        let len = match self {
            Self::Stored { stored_tx, .. } => stored_tx.events.len(),
            Self::Executed { events, .. } | Self::DryRun { events, .. } => events.len(),
        };
        let Some((prev, next, cs)) = page.paginate_indices(len) else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let event = match self {
                Self::Stored { stored_tx, .. } => {
                    Event::try_from_stored_transaction(stored_tx, *c).extend()?
                }
                Self::Executed { events, .. } | Self::DryRun { events, .. } => Event {
                    stored: None,
                    native: events[*c].clone(),
                },
            };
            connection.edges.push(Edge::new(c.encode_cursor(), event));
        }

        Ok(connection)
    }

    /// Timestamp corresponding to the checkpoint this transaction was finalized in.
    async fn timestamp(&self) -> Result<Option<DateTime>, Error> {
        let Self::Stored { stored_tx, .. } = self else {
            return Ok(None);
        };
        Ok(Some(DateTime::from_ms(stored_tx.timestamp_ms)?))
    }

    /// The epoch this transaction was finalized in.
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        Epoch::query(ctx.data_unchecked(), Some(self.native().executed_epoch()))
            .await
            .extend()
    }

    /// The checkpoint this transaction was finalized in.
    async fn checkpoint(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        // If the transaction data is not a stored transaction, it's not in the checkpoint yet so we return None.
        let Self::Stored { stored_tx, .. } = self else {
            return Ok(None);
        };

        Checkpoint::query(
            ctx.data_unchecked(),
            CheckpointId::by_seq_num(stored_tx.checkpoint_sequence_number as u64),
        )
        .await
        .extend()
    }

    // TODO: event_connection: EventConnection

    /// Base64 encoded bcs serialization of the on-chain transaction effects.
    async fn bcs(&self) -> Result<Base64> {
        let bytes = if let Self::Stored { stored_tx, .. } = self {
            stored_tx.raw_effects.clone()
        } else {
            bcs::to_bytes(&self.native())
                .map_err(|e| Error::Internal(format!("Error serializing transaction effects: {e}")))
                .extend()?
        };

        Ok(Base64::from(bytes))
    }
}

impl TransactionBlockEffects {
    fn native(&self) -> &NativeTransactionEffects {
        match self {
            TransactionBlockEffects::Stored { native, .. } => native,
            TransactionBlockEffects::Executed { native, .. } => native,
            TransactionBlockEffects::DryRun { native, .. } => native,
        }
    }
}

impl ConnectionNameType for DependencyConnectionNames {
    fn type_name<T: OutputType>() -> String {
        "DependencyConnection".to_string()
    }
}

impl EdgeNameType for DependencyConnectionNames {
    fn type_name<T: OutputType>() -> String {
        "DependencyEdge".to_string()
    }
}

impl TryFrom<TransactionBlock> for TransactionBlockEffects {
    type Error = Error;

    fn try_from(block: TransactionBlock) -> Result<Self, Error> {
        match block {
            TransactionBlock::Stored { stored_tx, .. } => {
                let native = bcs::from_bytes(&stored_tx.raw_effects).map_err(|e| {
                    Error::Internal(format!("Error deserializing transaction effects: {e}"))
                })?;

                Ok(TransactionBlockEffects::Stored {
                    stored_tx: stored_tx.clone(),
                    native,
                })
            }
            TransactionBlock::Executed {
                tx_data,
                effects,
                events,
            } => Ok(TransactionBlockEffects::Executed {
                tx_data,
                native: effects,
                events,
            }),
            TransactionBlock::DryRun {
                tx_data,
                effects,
                events,
            } => Ok(TransactionBlockEffects::DryRun {
                tx_data,
                native: effects,
                events,
            }),
        }
    }
}
