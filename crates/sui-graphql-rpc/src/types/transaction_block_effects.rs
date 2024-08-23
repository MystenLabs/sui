// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    consistency::ConsistentIndexCursor, data::package_resolver::PackageResolver, error::Error,
};
use async_graphql::{
    connection::{Connection, ConnectionNameType, CursorType, Edge, EdgeNameType, EmptyFields},
    *,
};
use fastcrypto::encoding::{Base64 as FBase64, Encoding};
use sui_indexer::models::transactions::StoredTransaction;
use sui_package_resolver::{CleverError, ErrorConstants};
use sui_types::{
    effects::{TransactionEffects as NativeTransactionEffects, TransactionEffectsAPI},
    event::Event as NativeEvent,
    execution_status::{
        ExecutionFailureStatus, ExecutionStatus as NativeExecutionStatus, MoveLocation,
        MoveLocationOpt,
    },
    transaction::{
        Command, ProgrammableTransaction, SenderSignedData as NativeSenderSignedData,
        TransactionData as NativeTransactionData, TransactionDataAPI,
        TransactionKind as NativeTransactionKind,
    },
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
    transaction_block::{TransactionBlock, TransactionBlockInner},
    uint53::UInt53,
    unchanged_shared_object::UnchangedSharedObject,
};

/// Wraps the actual transaction block effects data with the checkpoint sequence number at which the
/// data was viewed, for consistent results on paginating through and resolving nested types.
#[derive(Clone, Debug)]
pub(crate) struct TransactionBlockEffects {
    pub kind: TransactionBlockEffectsKind,
    /// The checkpoint sequence number this was viewed at.
    pub checkpoint_viewed_at: u64,
}

#[derive(Clone, Debug)]
pub(crate) enum TransactionBlockEffectsKind {
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

type CDependencies = JsonCursor<ConsistentIndexCursor>;
type CUnchangedSharedObject = JsonCursor<ConsistentIndexCursor>;
type CObjectChange = JsonCursor<ConsistentIndexCursor>;
type CBalanceChange = JsonCursor<ConsistentIndexCursor>;
type CEvent = JsonCursor<ConsistentIndexCursor>;

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
    async fn lamport_version(&self) -> UInt53 {
        self.native().lamport_version().value().into()
    }

    /// The reason for a transaction failure, if it did fail.
    /// If the error is a Move abort, the error message will be resolved to a human-readable form if
    /// possible, otherwise it will fall back to displaying the abort code and location.
    async fn errors(&self, ctx: &Context<'_>) -> Result<Option<String>> {
        let resolver: &PackageResolver = ctx.data_unchecked();
        let status = self.resolve_native_status_impl(resolver).await?;

        match status {
            NativeExecutionStatus::Success => Ok(None),

            NativeExecutionStatus::Failure {
                error,
                command: None,
            } => Ok(Some(error.to_string())),

            NativeExecutionStatus::Failure {
                error,
                command: Some(command),
            } => {
                let error = 'error: {
                    let ExecutionFailureStatus::MoveAbort(loc, code) = &error else {
                        break 'error error.to_string();
                    };
                    let fname_string = if let Some(fname) = &loc.function_name {
                        format!("::{}'", fname)
                    } else {
                        "'".to_string()
                    };

                    let Some(CleverError {
                        module_id,
                        source_line_number,
                        error_info,
                    }) = resolver
                        .resolve_clever_error(loc.module.clone(), *code)
                        .await
                    else {
                        break 'error format!(
                            "from '{}{fname_string} (instruction {}), abort code: {code}",
                            loc.module.to_canonical_display(true),
                            loc.instruction,
                        );
                    };

                    match error_info {
                        ErrorConstants::Rendered {
                            identifier,
                            constant,
                        } => {
                            format!(
                                "from '{}{fname_string} (line {source_line_number}), abort '{identifier}': {constant}",
                                module_id.to_canonical_display(true)
                            )
                        }
                        ErrorConstants::Raw { identifier, bytes } => {
                            let const_str = FBase64::encode(bytes);
                            format!(
                                "from '{}{fname_string} (line {source_line_number}), abort '{identifier}': {const_str}",
                                module_id.to_canonical_display(true)
                            )
                        }
                        ErrorConstants::None => {
                            format!(
                                "from '{}{fname_string} (line {source_line_number})",
                                module_id.to_canonical_display(true)
                            )
                        }
                    }
                };
                // Convert the command index into an ordinal.
                let command = command + 1;
                let suffix = match command % 10 {
                    1 => "st",
                    2 => "nd",
                    3 => "rd",
                    _ => "th",
                };
                Ok(Some(format!("Error in {command}{suffix} command, {error}")))
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

        let Some((prev, next, _, cs)) =
            page.paginate_consistent_indices(dependencies.len(), self.checkpoint_viewed_at)?
        else {
            return Ok(connection);
        };

        let indices: Vec<CDependencies> = cs.collect();

        let (Some(fst), Some(lst)) = (indices.first(), indices.last()) else {
            return Ok(connection);
        };

        let transactions = TransactionBlock::multi_query(
            ctx,
            dependencies[fst.ix..=lst.ix]
                .iter()
                .map(|d| Digest::from(*d))
                .collect(),
            fst.c, // Each element's cursor has the same checkpoint sequence number set
        )
        .await
        .extend()?;

        if transactions.is_empty() {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in indices {
            let digest: Digest = dependencies[c.ix].into();
            connection.edges.push(Edge::new(
                c.encode_cursor(),
                transactions.get(&digest).cloned(),
            ));
        }

        Ok(connection)
    }

    /// Effects to the gas object.
    async fn gas_effects(&self) -> Option<GasEffects> {
        Some(GasEffects::from(self.native(), self.checkpoint_viewed_at))
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

        let Some((prev, next, _, cs)) = page
            .paginate_consistent_indices(input_shared_objects.len(), self.checkpoint_viewed_at)?
        else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let result = UnchangedSharedObject::try_from(input_shared_objects[c.ix].clone(), c.c);
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

        let Some((prev, next, _, cs)) =
            page.paginate_consistent_indices(object_changes.len(), self.checkpoint_viewed_at)?
        else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let object_change = ObjectChange {
                native: object_changes[c.ix].clone(),
                checkpoint_viewed_at: c.c,
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

        let TransactionBlockEffectsKind::Stored { stored_tx, .. } = &self.kind else {
            return Ok(connection);
        };

        let Some((prev, next, _, cs)) = page
            .paginate_consistent_indices(stored_tx.get_balance_len(), self.checkpoint_viewed_at)?
        else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let Some(serialized) = &stored_tx.get_balance_at_idx(c.ix) else {
                continue;
            };

            let balance_change = BalanceChange::read(serialized, c.c).extend()?;
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
        let len = match &self.kind {
            TransactionBlockEffectsKind::Stored { stored_tx, .. } => stored_tx.get_event_len(),
            TransactionBlockEffectsKind::Executed { events, .. }
            | TransactionBlockEffectsKind::DryRun { events, .. } => events.len(),
        };
        let Some((prev, next, _, cs)) =
            page.paginate_consistent_indices(len, self.checkpoint_viewed_at)?
        else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let event = match &self.kind {
                TransactionBlockEffectsKind::Stored { stored_tx, .. } => {
                    Event::try_from_stored_transaction(stored_tx, c.ix, c.c).extend()?
                }
                TransactionBlockEffectsKind::Executed { events, .. }
                | TransactionBlockEffectsKind::DryRun { events, .. } => Event {
                    stored: None,
                    native: events[c.ix].clone(),
                    checkpoint_viewed_at: c.c,
                },
            };
            connection.edges.push(Edge::new(c.encode_cursor(), event));
        }

        Ok(connection)
    }

    /// Timestamp corresponding to the checkpoint this transaction was finalized in.
    async fn timestamp(&self) -> Result<Option<DateTime>, Error> {
        let TransactionBlockEffectsKind::Stored { stored_tx, .. } = &self.kind else {
            return Ok(None);
        };
        Ok(Some(DateTime::from_ms(stored_tx.timestamp_ms)?))
    }

    /// The epoch this transaction was finalized in.
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        Epoch::query(
            ctx,
            Some(self.native().executed_epoch()),
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    /// The checkpoint this transaction was finalized in.
    async fn checkpoint(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        // If the transaction data is not a stored transaction, it's not in the checkpoint yet so we return None.
        let TransactionBlockEffectsKind::Stored { stored_tx, .. } = &self.kind else {
            return Ok(None);
        };

        Checkpoint::query(
            ctx,
            CheckpointId::by_seq_num(stored_tx.checkpoint_sequence_number as u64),
            self.checkpoint_viewed_at,
        )
        .await
        .extend()
    }

    /// Base64 encoded bcs serialization of the on-chain transaction effects.
    async fn bcs(&self) -> Result<Base64> {
        let bytes = if let TransactionBlockEffectsKind::Stored { stored_tx, .. } = &self.kind {
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
        match &self.kind {
            TransactionBlockEffectsKind::Stored { native, .. } => native,
            TransactionBlockEffectsKind::Executed { native, .. } => native,
            TransactionBlockEffectsKind::DryRun { native, .. } => native,
        }
    }

    /// Get the transaction data from the transaction block effects.
    /// Will error if the transaction data is not available/invalid, but this should not occur.
    fn transaction_data(&self) -> Result<NativeTransactionData> {
        Ok(match &self.kind {
            TransactionBlockEffectsKind::Stored { stored_tx, .. } => {
                let s: NativeSenderSignedData = bcs::from_bytes(&stored_tx.raw_transaction)
                    .map_err(|e| {
                        Error::Internal(format!("Error deserializing transaction data: {e}"))
                    })?;
                s.transaction_data().clone()
            }
            TransactionBlockEffectsKind::Executed { tx_data, .. } => {
                tx_data.transaction_data().clone()
            }
            TransactionBlockEffectsKind::DryRun { tx_data, .. } => tx_data.clone(),
        })
    }

    /// Get the programmable transaction from the transaction block effects.
    /// * If the transaction was unable to be retrieved, this will return an Err.
    /// * If the transaction was able to be retrieved but was not a programmable transaction, this
    ///   will return Ok(None).
    /// * If the transaction was a programmable transaction, this will return Ok(Some(tx)).
    fn programmable_transaction(&self) -> Result<Option<ProgrammableTransaction>> {
        let tx_data = self.transaction_data()?;
        match tx_data.into_kind() {
            NativeTransactionKind::ProgrammableTransaction(tx) => Ok(Some(tx)),
            _ => Ok(None),
        }
    }

    /// Resolves the module ID within a Move abort to the storage ID of the package that the
    /// abort occured in.
    /// * If the error is not a Move abort, or the Move call in the programmable transaction cannot
    ///   be found, this function will do nothing.
    /// * If the error is a Move abort and the storage ID is unable to be resolved an error is
    ///   returned.
    async fn resolve_native_status_impl(
        &self,
        resolver: &PackageResolver,
    ) -> Result<NativeExecutionStatus> {
        let mut status = self.native().status().clone();
        if let NativeExecutionStatus::Failure {
            error:
                ExecutionFailureStatus::MoveAbort(MoveLocation { module, .. }, _)
                | ExecutionFailureStatus::MovePrimitiveRuntimeError(MoveLocationOpt(Some(MoveLocation {
                    module,
                    ..
                }))),
            command: Some(command_idx),
        } = &mut status
        {
            // Get the Move call that this error is associated with.
            if let Some(Command::MoveCall(ptb_call)) = self
                .programmable_transaction()?
                .and_then(|ptb| ptb.commands.into_iter().nth(*command_idx))
            {
                let module_new = module.clone();
                // Resolve the runtime module ID in the Move abort to the storage ID of the package
                // that the abort occured in. This is important to make sure that we look at the
                // correct version of the module when resolving the error.
                *module = resolver
                    .resolve_module_id(module_new, ptb_call.package.into())
                    .await
                    .map_err(|e| Error::Internal(format!("Error resolving Move location: {e}")))?;
            }
        }
        Ok(status)
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
        let checkpoint_viewed_at = block.checkpoint_viewed_at;
        let kind = match block.inner {
            TransactionBlockInner::Stored { stored_tx, .. } => {
                bcs::from_bytes(&stored_tx.raw_effects)
                    .map(|native| TransactionBlockEffectsKind::Stored {
                        stored_tx: stored_tx.clone(),
                        native,
                    })
                    .map_err(|e| {
                        Error::Internal(format!("Error deserializing transaction effects: {e}"))
                    })
            }
            TransactionBlockInner::Executed {
                tx_data,
                effects,
                events,
            } => Ok(TransactionBlockEffectsKind::Executed {
                tx_data,
                native: effects,
                events,
            }),
            TransactionBlockInner::DryRun {
                tx_data,
                effects,
                events,
            } => Ok(TransactionBlockEffectsKind::DryRun {
                tx_data,
                native: effects,
                events,
            }),
        }?;

        Ok(Self {
            kind,
            checkpoint_viewed_at,
        })
    }
}
