// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::server::{context_ext::DataProviderContextExt, sui_sdk_data_provider::convert_to_epoch};

use super::{
    address::Address,
    base64::Base64,
    epoch::Epoch,
    gas::{GasEffects, GasInput},
    sui_address::SuiAddress,
    tx_digest::TransactionDigest,
};
use async_graphql::*;
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionBlockDataAPI, SuiTransactionBlockEffects,
    SuiTransactionBlockEffectsAPI,
};
use sui_sdk::types::digests::TransactionDigest as NativeTransactionDigest;

#[derive(Clone)]
pub(crate) struct TransactionBlock(pub sui_json_rpc_types::SuiTransactionBlockResponse);

#[Object]
impl TransactionBlock {
    async fn digest(&self) -> TransactionDigest {
        TransactionDigest::from_array(self.0.digest.into_inner())
    }

    async fn effects(&self) -> Option<TransactionBlockEffects> {
        self.0.effects.as_ref().map(|tx_effects| tx_effects.into())
    }

    async fn sender(&self) -> Option<Address> {
        Some(Address {
            address: SuiAddress::from_array(
                self.0
                    .transaction
                    .as_ref()
                    .unwrap()
                    .data
                    .sender()
                    .to_inner(),
            ),
        })
    }

    async fn bcs(&self) -> Option<Base64> {
        Some(Base64::from(&self.0.raw_transaction))
    }

    async fn gas_input(&self) -> Option<GasInput> {
        Some(self.0.transaction.as_ref().unwrap().data.gas_data().into())
    }

    async fn expiration(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let tx_effects = self.0.effects.as_ref().unwrap();
        let gcs = tx_effects.gas_cost_summary();
        let data_provider = ctx.data_provider();
        let system_state = data_provider.get_latest_sui_system_state().await?;
        let protocol_configs = data_provider.fetch_protocol_config(None).await?;
        let epoch = convert_to_epoch(gcs.into(), &system_state, &protocol_configs)?;
        Ok(Some(epoch))
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct TransactionBlockEffects {
    pub digest: NativeTransactionDigest,
    pub gas_effects: GasEffects,
    pub status: SuiExecutionStatus,
    // pub transaction_block: TransactionBlock,
    // pub dependencies: Vec<TransactionBlock>,
    // pub lamport_version: Option<u64>,
    // pub object_reads: Vec<Object>,
    // pub object_changes: Vec<ObjectChange>,
    // pub balance_changes: Vec<BalanceChange>,
    // pub epoch: Epoch
    // pub checkpoint: Checkpoint
}

impl From<&SuiTransactionBlockEffects> for TransactionBlockEffects {
    fn from(tx_effects: &SuiTransactionBlockEffects) -> Self {
        Self {
            digest: *tx_effects.transaction_digest(),
            gas_effects: GasEffects::from((tx_effects.gas_cost_summary(), tx_effects.gas_object())),
            status: tx_effects.status().clone(),
        }
    }
}

#[Object]
impl TransactionBlockEffects {
    async fn digest(&self) -> TransactionDigest {
        TransactionDigest::from_array(self.digest.into_inner())
    }

    async fn status(&self) -> Option<ExecutionStatus> {
        Some(match self.status {
            SuiExecutionStatus::Success => ExecutionStatus::Success,
            SuiExecutionStatus::Failure { error: _ } => ExecutionStatus::Failure,
        })
    }

    async fn errors(&self) -> Option<String> {
        match &self.status {
            SuiExecutionStatus::Success => None,
            SuiExecutionStatus::Failure { error } => Some(error.clone()),
        }
    }

    async fn gas_effects(&self) -> Option<GasEffects> {
        Some(self.gas_effects)
    }

    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let data_provider = ctx.data_provider();
        let system_state = data_provider.get_latest_sui_system_state().await?;
        let protocol_configs = data_provider.fetch_protocol_config(None).await?;
        let epoch = convert_to_epoch(self.gas_effects.gcs, &system_state, &protocol_configs)?;
        Ok(Some(epoch))
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
