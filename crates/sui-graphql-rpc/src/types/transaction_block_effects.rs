// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{context_data::db_data_provider::PgManager, error::Error};
use async_graphql::*;
use either::Either;
use sui_indexer::models_v2::transactions::StoredTransaction;
use sui_types::{
    effects::{
        InputSharedObject, TransactionEffects as NativeTransactionEffects, TransactionEffectsAPI,
    },
    transaction::TransactionData as NativeTransactionData,
};

use super::{
    balance_change::BalanceChange, base64::Base64, checkpoint::Checkpoint, date_time::DateTime,
    epoch::Epoch, gas::GasEffects, object_change::ObjectChange,
    transaction_block::TransactionBlock, unchanged_shared_object::UnchangedSharedObject,
};
use std::collections::HashSet;
use sui_json_rpc_types::SuiExecutionStatus;
use sui_json_rpc_types::{
    SuiTransactionBlockEffects as PreCommitTransactionBlockEffects, SuiTransactionBlockEffectsAPI,
};

#[derive(Clone)]
pub(crate) struct TransactionBlockEffects {
    /// Representation of transaction effects in the Indexer's Store or
    /// the native representation of transaction effects.
    pub tx_data: Either<StoredTransaction, NativeTransactionData>,

    /// Deserialized representation of `stored.raw_effects`.
    /// Or representation returned from the JSON-RPC server in case of tx execution.
    /// TODO: remove this once we rework execution
    pub effects: Either<NativeTransactionEffects, PreCommitTransactionBlockEffects>,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum ExecutionStatus {
    Success,
    Failure,
}

#[Object]
impl TransactionBlockEffects {
    /// The transaction that ran to produce these effects.
    async fn transaction_block(&self) -> Result<Option<TransactionBlock>> {
        self.tx_data
            .as_ref()
            .left()
            .map(|stored_tx| TransactionBlock::try_from(stored_tx.clone()).extend())
            .transpose()
    }

    /// Whether the transaction executed successfully or not.
    async fn status(&self) -> Option<ExecutionStatus> {
        let sucess = match self.effects {
            Either::Left(ref native) => native.status().is_ok(),
            Either::Right(ref pre_commit_effects) => pre_commit_effects.status().is_ok(),
        };

        Some(match sucess {
            true => ExecutionStatus::Success,
            false => ExecutionStatus::Failure,
        })
    }

    /// The latest version of all objects (apart from packages) that have been created or modified
    /// by this transaction, immediately following this transaction.
    async fn lamport_version(&self) -> u64 {
        match self.effects {
            Either::Left(ref native) => native.lamport_version(),
            Either::Right(ref pre_commit_effects) => pre_commit_effects.lamport_version(),
        }
        .value()
    }

    /// The reason for a transaction failure, if it did fail.
    async fn errors(&self) -> Option<String> {
        let status = match self.effects {
            Either::Left(ref native) => {
                // Convert to SuiExecutionStatus for consistency with response from FN
                SuiExecutionStatus::from(native.status().clone())
            }
            Either::Right(ref pre_commit_effects) => pre_commit_effects.status().clone(),
        };

        match status {
            SuiExecutionStatus::Success => None,

            SuiExecutionStatus::Failure { error } => Some(error),
        }
    }

    /// Transactions whose outputs this transaction depends upon.
    async fn dependencies(&self, ctx: &Context<'_>) -> Result<Option<Vec<TransactionBlock>>> {
        let dependencies = match self.effects {
            Either::Left(ref native) => native.dependencies(),
            Either::Right(ref pre_commit_effects) => pre_commit_effects.dependencies(),
        };
        ctx.data_unchecked::<PgManager>()
            .fetch_txs_by_digests(dependencies)
            .await
            .extend()
    }

    /// Effects to the gas object.
    async fn gas_effects(&self) -> Option<GasEffects> {
        let gas_eff = match self.effects {
            Either::Left(ref native) => GasEffects::from(native),
            Either::Right(ref pre_commit_effects) => {
                GasEffects::from_json_rpc_effects(pre_commit_effects)
            }
        };
        Some(gas_eff)
    }

    /// Shared objects that are referenced by but not changed by this transaction.
    async fn unchanged_shared_objects(&self) -> Option<Vec<UnchangedSharedObject>> {
        Some(match self.effects {
            Either::Left(ref native) => native
                .input_shared_objects()
                .iter()
                .filter_map(|input| UnchangedSharedObject::try_from(input.clone()).ok())
                .collect(),
            Either::Right(ref pre_commit_effects) => {
                let modified: HashSet<_> = pre_commit_effects
                    .modified_at_versions()
                    .iter()
                    .map(|(r, _)| *r)
                    .collect();
                pre_commit_effects
                    .shared_objects()
                    .iter()
                    .map(|r| {
                        if modified.contains(&r.object_id) {
                            InputSharedObject::Mutate(r.to_object_ref())
                        } else {
                            InputSharedObject::ReadOnly(r.to_object_ref())
                        }
                    })
                    .filter_map(|input| UnchangedSharedObject::try_from(input).ok())
                    .collect()
            }
        })
    }

    /// The effect this transaction had on objects on-chain.
    async fn object_changes(&self) -> Option<Vec<ObjectChange>> {
        match self.effects {
            Either::Left(ref native) => Some(
                native
                    .object_changes()
                    .into_iter()
                    .map(|native| ObjectChange { native })
                    .collect(),
            ),

            // TODO: implement this for pre-commit effects
            Either::Right(ref _pre_commit_effects) => None,
        }
    }

    /// The effect this transaction had on the balances (sum of coin values per coin type) of
    /// addresses and objects.
    async fn balance_changes(&self) -> Result<Option<Vec<BalanceChange>>> {
        let Some(stored_tx) = self.tx_data.as_ref().left() else {
            return Ok(None);
        };

        let mut changes = Vec::with_capacity(stored_tx.balance_changes.len());
        for change in stored_tx.balance_changes.iter().flatten() {
            changes.push(BalanceChange::read(change).extend()?);
        }

        Ok(Some(changes))
    }

    /// Timestamp corresponding to the checkpoint this transaction was finalized in.
    async fn timestamp(&self) -> Option<DateTime> {
        DateTime::from_ms(self.tx_data.as_ref().left()?.timestamp_ms)
    }

    /// The epoch this transaction was finalized in.
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let executed_epoch = match self.effects {
            Either::Left(ref native) => native.executed_epoch(),
            Either::Right(ref pre_commit_effects) => pre_commit_effects.executed_epoch(),
        };
        Ok(Some(
            ctx.data_unchecked::<PgManager>()
                .fetch_epoch_strict(executed_epoch)
                .await
                .extend()?,
        ))
    }

    /// The checkpoint this transaction was finalized in.
    async fn checkpoint(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        let Some(stored_tx) = self.tx_data.as_ref().left() else {
            return Ok(None);
        };
        ctx.data_unchecked::<PgManager>()
            .fetch_checkpoint(None, Some(stored_tx.checkpoint_sequence_number as u64))
            .await
            .extend()
    }

    // TODO: event_connection: EventConnection

    /// Base64 encoded bcs serialization of the on-chain transaction effects.
    async fn bcs(&self) -> Result<Option<Base64>> {
        let bytes = if let Some(stored) = self.tx_data.as_ref().left() {
            Some(stored.raw_effects.clone())
        } else {
            match self.effects {
                Either::Left(ref native) => Some(
                    bcs::to_bytes(native)
                        .map_err(|e| {
                            Error::Internal(format!("Error serializing transaction effects: {e}"))
                        })
                        .extend()?,
                ),
                Either::Right(_) => None,
            }
        };

        Ok(bytes.map(Base64::from))
    }
}

impl TryFrom<StoredTransaction> for TransactionBlockEffects {
    type Error = Error;

    fn try_from(stored: StoredTransaction) -> Result<Self, Error> {
        let native = bcs::from_bytes(&stored.raw_effects).map_err(|e| {
            Error::Internal(format!("Error deserializing transaction effects: {e}"))
        })?;

        Ok(TransactionBlockEffects {
            tx_data: Either::Left(stored),
            effects: Either::Left(native),
        })
    }
}
