// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    address::Address, base64::Base64, digest::Digest, epoch::Epoch, gas::GasInput,
    sui_address::SuiAddress, transaction_block_effects::TransactionBlockEffects,
    transaction_block_kind::TransactionBlockKind, transaction_signature::TransactionSignature,
};
use crate::context_data::db_data_provider::PgManager;
use async_graphql::*;

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

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum TransactionBlockKindInput {
    SystemTx = 0,
    ProgrammableTx = 1,
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
