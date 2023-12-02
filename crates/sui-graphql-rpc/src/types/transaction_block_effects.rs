// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_indexer::models_v2::transactions::StoredTransaction;
use sui_types::{
    effects::{TransactionEffects as NativeTransactionEffects, TransactionEffectsAPI},
    execution_status::ExecutionStatus as NativeExecutionStatus,
};

use crate::{context_data::db_data_provider::PgManager, error::Error};

use super::{
    balance_change::BalanceChange, base64::Base64, checkpoint::Checkpoint, date_time::DateTime,
    epoch::Epoch, gas::GasEffects, object_change::ObjectChange,
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

    /// The latest version of all objects (apart from packages) that have been created or modified
    /// by this transaction, immediately following this transaction.
    async fn lamport_version(&self) -> u64 {
        self.native.lamport_version().value()
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
    async fn object_changes(&self) -> Result<Option<Vec<ObjectChange>>> {
        let mut changes = Vec::with_capacity(self.stored.object_changes.len());
        for change in self.stored.object_changes.iter().flatten() {
            changes.push(ObjectChange::read(change).extend()?);
        }

        Ok(Some(changes))
    }

    /// The effect this transaction had on the balances (sum of coin values per coin type) of
    /// addresses and objects.
    async fn balance_changes(&self) -> Result<Option<Vec<BalanceChange>>> {
        let mut changes = Vec::with_capacity(self.stored.balance_changes.len());
        for change in self.stored.balance_changes.iter().flatten() {
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

impl TryFrom<StoredTransaction> for TransactionBlockEffects {
    type Error = Error;

    fn try_from(stored: StoredTransaction) -> Result<Self, Error> {
        let native = bcs::from_bytes(&stored.raw_effects).map_err(|e| {
            Error::Internal(format!("Error deserializing transaction effects: {e}"))
        })?;

        Ok(TransactionBlockEffects { stored, native })
    }
}
