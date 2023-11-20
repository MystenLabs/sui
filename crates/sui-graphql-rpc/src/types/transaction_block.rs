// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use super::{
    address::Address,
    balance::BalanceChange,
    base64::Base64,
    big_int::BigInt,
    checkpoint::Checkpoint,
    date_time::DateTime,
    digest::Digest,
    epoch::Epoch,
    gas::{GasEffects, GasInput},
    move_type::MoveType,
    object_change::ObjectChange,
    owner::Owner,
    sui_address::SuiAddress,
    transaction_block_kind::TransactionBlockKind,
    transaction_signature::TransactionSignature,
};
use crate::{context_data::db_data_provider::PgManager, error::Error};
use async_graphql::*;

use sui_indexer::types_v2::IndexedObjectChange;
use sui_json_rpc_types::{
    BalanceChange as NativeBalanceChange, SuiExecutionStatus, SuiTransactionBlockEffects,
    SuiTransactionBlockEffectsAPI,
};
use sui_types::digests::TransactionDigest;

#[derive(SimpleObject, Clone)]
#[graphql(complex)]
pub(crate) struct TransactionBlock {
    #[graphql(skip)]
    pub digest: Digest,
    /// The effects field captures the results to the chain of executing this transaction
    pub effects: Option<TransactionBlockEffects>,
    /// The address of the user sending this transaction block
    pub sender: Option<Address>,
    /// The transaction block data in BCS format.
    /// This includes data on the sender, inputs, sponsor, gas inputs, individual transactions, and user signatures.
    pub bcs: Option<Base64>,
    /// The gas input field provides information on what objects were used as gas
    /// As well as the owner of the gas object(s) and information on the gas price and budget
    /// If the owner of the gas object(s) is not the same as the sender,
    /// the transaction block is a sponsored transaction block.
    pub gas_input: Option<GasInput>,
    #[graphql(skip)]
    pub epoch_id: Option<u64>,
    pub kind: Option<TransactionBlockKind>,
    /// A list of signatures of all signers, senders, and potentially the gas owner if this is a sponsored transaction.
    pub signatures: Option<Vec<Option<TransactionSignature>>>,
}

#[ComplexObject]
impl TransactionBlock {
    /// A 32-byte hash that uniquely identifies the transaction block contents, encoded in Base58.
    /// This serves as a unique id for the block on chain
    async fn digest(&self) -> String {
        self.digest.to_string()
    }

    /// This field is set by senders of a transaction block
    /// It is an epoch reference that sets a deadline after which validators will no longer consider the transaction valid
    /// By default, there is no deadline for when a transaction must execute
    async fn expiration(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        match self.epoch_id {
            None => Ok(None),
            Some(epoch_id) => {
                let epoch = ctx
                    .data_unchecked::<PgManager>()
                    .fetch_epoch_strict(epoch_id)
                    .await
                    .extend()?;
                Ok(Some(epoch))
            }
        }
    }
}

#[derive(Clone, SimpleObject)]
#[graphql(complex)]
pub(crate) struct TransactionBlockEffects {
    #[graphql(skip)]
    pub gas_effects: GasEffects,
    pub status: ExecutionStatus,
    pub errors: Option<String>,

    #[graphql(skip)]
    pub tx_block_digest: Digest,
    // pub transaction_block: Option<Box<TransactionBlock>>,
    #[graphql(skip)]
    pub dependencies: Vec<TransactionDigest>,
    pub lamport_version: Option<u64>,
    // unclear what object reads is about, TODO @ashok
    // pub object_reads: Vec<Object>,
    #[graphql(skip)]
    pub object_changes_as_bcs: Vec<Option<Vec<u8>>>,
    pub balance_changes: Option<Vec<Option<BalanceChange>>>,
    // have their own resolvers in the impl block
    #[graphql(skip)]
    pub epoch_id: u64,
    // pub epoch: Option<Epoch>,
    // pub checkpoint: Option<Checkpoint>,
    #[graphql(skip)]
    checkpoint_seq_number: u64,
    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    /// representing the time when the checkpoint that contains
    /// this transaction was created
    pub timestamp: Option<DateTime>,
}

impl TransactionBlockEffects {
    pub fn from_stored_transaction(
        balance_changes: Vec<Option<Vec<u8>>>,
        checkpoint_seq_number: u64,
        object_changes: Vec<Option<Vec<u8>>>,
        tx_effects: &SuiTransactionBlockEffects,
        tx_block_digest: Digest,
        timestamp: Option<DateTime>,
    ) -> Result<Option<Self>, Error> {
        let (status, errors) = match tx_effects.status() {
            SuiExecutionStatus::Success => (ExecutionStatus::Success, None),
            SuiExecutionStatus::Failure { error } => {
                (ExecutionStatus::Failure, Some(error.clone()))
            }
        };
        let lamport_version = tx_effects
            .created()
            .first()
            .map(|x| x.reference.version.value());
        let balance_changes = BalanceChange::from(balance_changes)?;

        Ok(Some(Self {
            gas_effects: GasEffects::from((tx_effects.gas_cost_summary(), tx_effects.gas_object())),
            status,
            errors,
            lamport_version,
            dependencies: tx_effects.dependencies().to_vec(),
            balance_changes: Some(balance_changes),
            epoch_id: tx_effects.executed_epoch(),
            tx_block_digest,
            object_changes_as_bcs: object_changes,
            checkpoint_seq_number,
            timestamp,
        }))
    }
}

#[ComplexObject]
impl TransactionBlockEffects {
    // the lamport version is the sequence number?
    async fn checkpoint(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        let checkpoint = ctx
            .data_unchecked::<PgManager>()
            .fetch_checkpoint(None, Some(self.checkpoint_seq_number))
            .await
            .extend()?;
        Ok(checkpoint)
    }

    // resolve the dependencies based on the transaction digests
    async fn dependencies(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Vec<Option<TransactionBlock>>>> {
        let digests = &self.dependencies;

        ctx.data_unchecked::<PgManager>()
            .fetch_txs_by_digests(digests)
            .await
            .extend()
    }

    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let epoch = ctx
            .data_unchecked::<PgManager>()
            .fetch_epoch_strict(self.epoch_id)
            .await
            .extend()?;
        Ok(Some(epoch))
    }

    async fn gas_effects(&self) -> Option<GasEffects> {
        Some(self.gas_effects)
    }

    async fn object_changes(&self, ctx: &Context<'_>) -> Result<Option<Vec<Option<ObjectChange>>>> {
        let mut changes = vec![];
        for bcs in self.object_changes_as_bcs.iter().flatten() {
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

    async fn transaction_block(&self, ctx: &Context<'_>) -> Result<Option<TransactionBlock>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_tx(self.tx_block_digest.to_string().as_str())
            .await
            .extend()
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum TransactionBlockKindInput {
    SystemTx = 0,
    ProgrammableTx = 1,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum ExecutionStatus {
    Success,
    Failure,
}

#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct TransactionBlockFilter {
    pub package: Option<SuiAddress>,
    pub module: Option<String>,
    pub function: Option<String>,

    pub kind: Option<TransactionBlockKindInput>,
    pub after_checkpoint: Option<u64>,
    pub at_checkpoint: Option<u64>,
    pub before_checkpoint: Option<u64>,

    pub sign_address: Option<SuiAddress>,
    pub sent_address: Option<SuiAddress>,
    pub recv_address: Option<SuiAddress>,
    pub paid_address: Option<SuiAddress>,

    pub input_object: Option<SuiAddress>,
    pub changed_object: Option<SuiAddress>,

    pub transaction_ids: Option<Vec<String>>,
}

impl BalanceChange {
    fn from(balance_changes: Vec<Option<Vec<u8>>>) -> Result<Vec<Option<BalanceChange>>, Error> {
        let mut output = vec![];
        for balance_change_bcs in balance_changes.into_iter().flatten() {
            let balance_change: NativeBalanceChange = bcs::from_bytes(&balance_change_bcs)
                .map_err(|_| {
                    Error::Internal("Cannot convert bcs bytes to BalanceChange".to_string())
                })?;
            let balance_change_owner_address =
                balance_change.owner.get_owner_address().map_err(|_| {
                    Error::Internal("Cannot get the balance change owner's address".to_string())
                })?;

            let address =
                SuiAddress::from_bytes(balance_change_owner_address.to_vec()).map_err(|_| {
                    Error::Internal(
                        "Cannot get a SuiAddress from the balance change owner address".to_string(),
                    )
                })?;
            let owner = Owner { address };
            let amount =
                BigInt::from_str(balance_change.amount.to_string().as_str()).map_err(|_| {
                    Error::Internal(
                        "Cannot convert balance change amount to BigInt amount".to_string(),
                    )
                })?;
            output.push(Some(BalanceChange {
                owner: Some(owner),
                amount: Some(amount),
                coin_type: Some(MoveType::new(
                    balance_change.coin_type.to_canonical_string(true),
                )),
            }))
        }
        Ok(output)
    }
}

// TODO this should be replaced together with the whole TXBLOCKEFFECTS once the indexer has this stuff implemented
// see effects_v2.rs in indexer
impl ObjectChange {
    async fn from(
        object_change: IndexedObjectChange,
        ctx: &Context<'_>,
    ) -> Result<Option<Self>, Error> {
        match object_change {
            IndexedObjectChange::Created {
                sender: _,
                owner: _,
                object_type: _,
                object_id,
                version,
                digest: _,
            } => {
                let sui_address = SuiAddress::from_bytes(object_id.into_bytes()).map_err(|_| {
                    Error::Internal("Cannot decode a SuiAddress from object_id".to_string())
                })?;
                let output_state = ctx
                    .data_unchecked::<PgManager>()
                    .fetch_obj(sui_address, Some(version.value()))
                    .await?;
                Ok(Some(Self {
                    input_state: None,
                    output_state,
                    id_created: Some(true),
                    id_deleted: None,
                }))
            }

            IndexedObjectChange::Published {
                package_id,
                version,
                digest: _,
                modules: _,
            } => {
                let sui_address =
                    SuiAddress::from_bytes(package_id.into_bytes()).map_err(|_| {
                        Error::Internal("Cannot decode a SuiAddress from package_id".to_string())
                    })?;
                let output_state = ctx
                    .data_unchecked::<PgManager>()
                    .fetch_obj(sui_address, Some(version.value()))
                    .await?;
                Ok(Some(Self {
                    input_state: None,
                    output_state,
                    id_created: Some(true),
                    id_deleted: None,
                }))
            }
            IndexedObjectChange::Transferred {
                sender: _,
                recipient: _,
                object_type: _,
                object_id,
                version,
                digest: _,
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

                Ok(Some(Self {
                    input_state: output_state.clone(),
                    output_state,
                    id_created: None,
                    id_deleted: None,
                }))
            }
            IndexedObjectChange::Mutated {
                sender: _,
                owner: _,
                object_type: _,
                object_id,
                version,
                previous_version,
                digest: _,
            } => {
                let sui_address = SuiAddress::from_bytes(object_id.into_bytes()).map_err(|_| {
                    Error::Internal("Cannot decode a SuiAddress from object_id".to_string())
                })?;
                let input_state = ctx
                    .data_unchecked::<PgManager>()
                    .fetch_obj(sui_address, Some(previous_version.value()))
                    .await?;
                let output_state = ctx
                    .data_unchecked::<PgManager>()
                    .fetch_obj(sui_address, Some(version.value()))
                    .await?;
                Ok(Some(Self {
                    input_state,
                    output_state,
                    id_created: None,
                    id_deleted: None,
                }))
            }
            IndexedObjectChange::Deleted {
                sender: _,
                object_type: _,
                object_id,
                version,
            } => {
                let sui_address = SuiAddress::from_bytes(object_id.into_bytes()).map_err(|_| {
                    Error::Internal("Cannot decode a SuiAddress from object_id".to_string())
                })?;
                let input_state = ctx
                    .data_unchecked::<PgManager>()
                    .fetch_obj(sui_address, Some(version.value()))
                    .await?;
                Ok(Some(Self {
                    input_state,
                    output_state: None,
                    id_created: None,
                    id_deleted: Some(true),
                }))
            }
            IndexedObjectChange::Wrapped {
                sender: _,
                object_type: _,
                object_id,
                version,
            } => {
                let sui_address = SuiAddress::from_bytes(object_id.into_bytes()).map_err(|_| {
                    Error::Internal("Cannot decode a SuiAddress from object_id".to_string())
                })?;
                let output_state = ctx
                    .data_unchecked::<PgManager>()
                    .fetch_obj(sui_address, Some(version.value()))
                    .await?;
                Ok(Some(Self {
                    input_state: None,
                    output_state,
                    id_created: None,
                    id_deleted: None,
                }))
            }
        }
    }
}
