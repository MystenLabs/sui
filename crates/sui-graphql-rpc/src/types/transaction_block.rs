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
    sui_address::SuiAddress,
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
}

#[ComplexObject]
impl TransactionBlock {
    async fn expiration(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        if self.effects.is_none() {
            return Ok(None);
        }
        if let Some(gcs) = &self.effects.as_ref().unwrap().gas_effects {
            let data_provider = ctx.data_provider();
            let system_state = data_provider.get_latest_sui_system_state().await?;
            let protocol_configs = data_provider.fetch_protocol_config(None).await?;
            let epoch = convert_to_epoch(gcs.gcs, &system_state, &protocol_configs)?;
            Ok(Some(epoch))
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
    // pub epoch: Epoch
    // pub checkpoint: Checkpoint
}

#[ComplexObject]
impl TransactionBlockEffects {
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        if let Some(gcs) = &self.gas_effects {
            let data_provider = ctx.data_provider();
            let system_state = data_provider.get_latest_sui_system_state().await?;
            let protocol_configs = data_provider.fetch_protocol_config(None).await?;
            let epoch = convert_to_epoch(gcs.gcs, &system_state, &protocol_configs)?;
            Ok(Some(epoch))
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
