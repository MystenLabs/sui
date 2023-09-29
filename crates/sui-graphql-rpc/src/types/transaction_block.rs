// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::{
    context_ext::DataProviderContextExt, sui_sdk_data_provider::convert_to_epoch,
};

use super::{
    address::Address,
    base64::Base64,
    epoch::Epoch,
    gas::{GasEffects, GasInput},
    sui_address::SuiAddress, checkpoint::Checkpoint,
};
use async_graphql::*;

#[derive(SimpleObject, Clone, Eq, PartialEq)]
#[graphql(complex)]
pub(crate) struct TransactionBlock {
    pub digest: String,
    pub effects: Option<TransactionBlockEffects>,
    pub sender: Option<Address>,
    pub bcs: Option<Base64>,
    pub gas_input: Option<GasInput>,

    #[graphql(skip)]
    pub checkpoint_sequence_number: Option<u64>,
}

#[ComplexObject]
impl TransactionBlock {
    async fn expiration(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let checkpoint = ctx.data_provider().fetch_checkpoint(None, self.checkpoint_sequence_number).await?;
        let epoch_id = checkpoint.map(|c| c.epoch.epoch_id);
        if let Some(epoch_id) = epoch_id {
            let epoch = ctx.data_provider().fetch_epoch(epoch_id).await?;
            Ok(epoch)
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone, Eq, PartialEq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct TransactionBlockEffects {
    pub gas_effects: Option<GasEffects>,
    pub status: ExecutionStatus,
    pub errors: Option<String>,
    // pub transaction_block: TransactionBlock,
    // pub dependencies: Vec<TransactionBlock>,
    // pub lamport_version: Option<u64>,
    // pub object_reads: Vec<Object>,
    // pub object_changes: Vec<ObjectChange>,
    // pub balance_changes: Vec<BalanceChange>,

    #[graphql(skip)]
    pub checkpoint_sequence_number: Option<u64>,
}

#[ComplexObject]
impl TransactionBlockEffects {
    async fn checkpoint(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        ctx.data_provider().fetch_checkpoint(None, self.checkpoint_sequence_number).await
    }

    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let checkpoint = ctx.data_provider().fetch_checkpoint(None, self.checkpoint_sequence_number).await?;
        let epoch_id = checkpoint.map(|c| c.epoch.epoch_id);
        if let Some(epoch_id) = epoch_id {
            let epoch = ctx.data_provider().fetch_epoch(epoch_id).await?;
            Ok(epoch)
        } else {
            Ok(None)
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum TransactionBlockKindInput {
    ProgrammableTx,
    SystemTx,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum ExecutionStatus {
    Success,
    Failure,
}

#[derive(InputObject)]
pub(crate) struct TransactionBlockFilter {
    package: Option<SuiAddress>,
    module: Option<String>,
    function: Option<String>,

    kind: Option<TransactionBlockKindInput>,
    checkpoint: Option<u64>,

    sign_address: Option<SuiAddress>,
    sent_address: Option<SuiAddress>,
    recv_address: Option<SuiAddress>,
    paid_address: Option<SuiAddress>,

    input_object: Option<SuiAddress>,
    changed_object: Option<SuiAddress>,
}
