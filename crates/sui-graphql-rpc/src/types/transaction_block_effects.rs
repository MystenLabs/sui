// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_indexer::{models_v2::transactions::StoredTransaction, types_v2::IndexedObjectChange};
use sui_types::{
    effects::{TransactionEffects as NativeTransactionEffects, TransactionEffectsAPI},
    execution_status::ExecutionStatus as NativeExecutionStatus,
};

use crate::{context_data::db_data_provider::PgManager, error::Error};

use super::{
    balance_change::BalanceChange, base64::Base64, checkpoint::Checkpoint, date_time::DateTime,
    epoch::Epoch, gas::GasEffects, object::Object, sui_address::SuiAddress,
    transaction_block::TransactionBlock,
};

#[derive(Clone)]
pub(crate) struct TransactionBlockEffects {
    /// Representation of transaction effects in the Indexer's Store.  The indexer stores the
    /// transaction data and its effects together, in one table.
    pub stored: StoredTransaction,

    /// Deserialized representation of `stored.raw_effects`.
    pub native: NativeTransactionEffects,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum ExecutionStatus {
    Success,
    Failure,
}

#[derive(Clone, SimpleObject)]
pub(crate) struct ObjectChange {
    // TODO: input_state (waiting for object history)
    pub output_state: Option<Object>,
    pub id_created: Option<bool>,
    pub id_deleted: Option<bool>,
}

#[Object]
impl TransactionBlockEffects {
    /// The transaction that ran to produce these effects.
    async fn transaction_block(&self, ctx: &Context<'_>) -> Result<TransactionBlock> {
        let digest = self.native.transaction_digest().to_string();
        ctx.data_unchecked::<PgManager>()
            .fetch_tx(digest.as_str())
            .await
            .extend()?
            .ok_or_else(|| {
                Error::Internal(format!(
                    "Failed to get transaction {digest} from its effects"
                ))
            })
            .extend()
    }

    /// Whether the transaction executed successfully or not.
    async fn status(&self) -> Option<ExecutionStatus> {
        Some(match self.native.status() {
            NativeExecutionStatus::Success => ExecutionStatus::Success,
            NativeExecutionStatus::Failure { .. } => ExecutionStatus::Failure,
        })
    }

    /// The latest version of all objects that have been created or modified by this transaction,
    /// immediately following this transaction.  A system transaction that does not modify or create
    /// objects will not have a lamport version.
    async fn lamport_version(&self) -> Option<u64> {
        if let Some(((_id, version, _digest), _owner)) = self.native.created().first() {
            Some(version.value())
        } else if let Some(((_id, version, _digest), _owner)) = self.native.mutated().first() {
            Some(version.value())
        } else if let Some(((_id, version, _digest), _owner)) = self.native.unwrapped().first() {
            Some(version.value())
        } else {
            None
        }
    }

    /// The reason for a transaction failure, if it did fail.
    async fn errors(&self) -> Option<String> {
        match self.native.status() {
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
    async fn dependencies(&self, ctx: &Context<'_>) -> Result<Option<Vec<TransactionBlock>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_txs_by_digests(self.native.dependencies())
            .await
            .extend()
    }

    /// Effects to the gas object.
    async fn gas_effects(&self) -> Option<GasEffects> {
        Some(GasEffects::from(&self.native))
    }

    // TODO object_reads

    /// The effect this transaction had on objects on-chain.
    async fn object_changes(&self, ctx: &Context<'_>) -> Result<Option<Vec<ObjectChange>>> {
        let mut changes = vec![];

        for bcs in self.stored.object_changes.iter().flatten() {
            let object_change: IndexedObjectChange = bcs::from_bytes(bcs)
                .map_err(|_| {
                    Error::Internal(
                        "Cannot convert bcs bytes into IndexedObjectChange object".to_string(),
                    )
                })
                .extend()?;
            changes.push(ObjectChange::from(object_change, ctx).await.extend()?);
        }

        Ok(Some(changes))
    }

    /// The effect this transaction had on the balances (sum of coin values per coin type) of
    /// addresses and objects.
    async fn balance_changes(&self) -> Result<Option<Vec<BalanceChange>>> {
        let mut changes = Vec::with_capacity(self.stored.balance_changes.len());
        for change in &self.stored.balance_changes {
            let Some(change) = change else { continue };
            changes.push(BalanceChange::read(change).extend()?);
        }

        Ok(Some(changes))
    }

    /// Timestamp corresponding to the checkpoint this transaction was finalized in.
    async fn timestamp(&self) -> Option<DateTime> {
        DateTime::from_ms(self.stored.timestamp_ms)
    }

    /// The epoch this transaction was finalized in.
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        Ok(Some(
            ctx.data_unchecked::<PgManager>()
                .fetch_epoch_strict(self.native.executed_epoch())
                .await
                .extend()?,
        ))
    }

    /// The checkpoint this transaction was finalized in.
    async fn checkpoint(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        let checkpoint = self.stored.checkpoint_sequence_number as u64;
        ctx.data_unchecked::<PgManager>()
            .fetch_checkpoint(None, Some(checkpoint))
            .await
            .extend()
    }

    // TODO: event_connection: EventConnection

    /// Base64 encoded bcs serialization of the on-chain transaction effects.
    async fn bcs(&self) -> Option<Base64> {
        Some(Base64::from(&self.stored.raw_effects))
    }
}

// TODO this should be replaced together with the whole TXBLOCKEFFECTS once the indexer has this stuff implemented
// see effects_v2.rs in indexer
impl ObjectChange {
    async fn from(object_change: IndexedObjectChange, ctx: &Context<'_>) -> Result<Self, Error> {
        match object_change {
            IndexedObjectChange::Created {
                object_id, version, ..
            } => {
                let sui_address = SuiAddress::from_bytes(object_id.into_bytes()).map_err(|_| {
                    Error::Internal("Cannot decode a SuiAddress from object_id".to_string())
                })?;
                let output_state = ctx
                    .data_unchecked::<PgManager>()
                    .fetch_obj(sui_address, Some(version.value()))
                    .await?;
                Ok(Self {
                    output_state,
                    id_created: Some(true),
                    id_deleted: None,
                })
            }

            IndexedObjectChange::Published {
                package_id,
                version,
                ..
            } => {
                let sui_address =
                    SuiAddress::from_bytes(package_id.into_bytes()).map_err(|_| {
                        Error::Internal("Cannot decode a SuiAddress from package_id".to_string())
                    })?;
                let output_state = ctx
                    .data_unchecked::<PgManager>()
                    .fetch_obj(sui_address, Some(version.value()))
                    .await?;
                Ok(Self {
                    output_state,
                    id_created: Some(true),
                    id_deleted: None,
                })
            }
            IndexedObjectChange::Transferred {
                object_id, version, ..
            } => {
                let sui_address = SuiAddress::from_bytes(object_id.into_bytes()).map_err(|_| {
                    Error::Internal("Cannot decode a SuiAddress from object_id".to_string())
                })?;
                // TODO
                // I assume the output is a different object as it probably has a different
                // owner (the recipient) + the version + digest are different
                let output_state = ctx
                    .data_unchecked::<PgManager>()
                    .fetch_obj(sui_address, Some(version.value()))
                    .await?;

                Ok(Self {
                    output_state,
                    id_created: None,
                    id_deleted: None,
                })
            }
            IndexedObjectChange::Mutated {
                object_id, version, ..
            } => {
                let sui_address = SuiAddress::from_bytes(object_id.into_bytes()).map_err(|_| {
                    Error::Internal("Cannot decode a SuiAddress from object_id".to_string())
                })?;
                let output_state = ctx
                    .data_unchecked::<PgManager>()
                    .fetch_obj(sui_address, Some(version.value()))
                    .await?;
                Ok(Self {
                    output_state,
                    id_created: None,
                    id_deleted: None,
                })
            }
            IndexedObjectChange::Deleted { .. } => Ok(Self {
                output_state: None,
                id_created: None,
                id_deleted: Some(true),
            }),
            IndexedObjectChange::Wrapped { .. } => Ok(Self {
                output_state: None,
                id_created: None,
                id_deleted: None,
            }),
        }
    }
}

impl TryFrom<StoredTransaction> for TransactionBlockEffects {
    type Error = Error;

    fn try_from(stored: StoredTransaction) -> Result<Self, Error> {
        let native = bcs::from_bytes(&stored.raw_effects).map_err(|e| {
            Error::Internal(format!("Error deserializing transaction effects: {e}"))
        })?;

        Ok(TransactionBlockEffects { stored, native })
    }
}
