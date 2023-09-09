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

    async fn effects(&self) -> Result<Option<TransactionBlockEffects>> {
        let tx_effects = self.0.effects.as_ref();

        Ok(Some(TransactionBlockEffects {
            digest: self.0.digest,
            tx_effects,
        }))
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
        let epoch = convert_to_epoch(gcs, &system_state, &protocol_configs)?;
        Ok(Some(epoch))
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct TransactionBlockEffects<'a> {
    pub digest: NativeTransactionDigest,
    pub tx_effects: Option<&'a SuiTransactionBlockEffects>,
}

#[Object]
impl TransactionBlockEffects<'_> {
    async fn digest(&self) -> TransactionDigest {
        TransactionDigest::from_array(self.digest.into_inner())
    }

    async fn gas_effects(&self) -> Option<GasEffects> {
        let tx_effects = self.tx_effects.unwrap();
        let gas_effects = GasEffects::new(tx_effects.gas_cost_summary(), tx_effects.gas_object());
        Some(gas_effects)
    }

    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let tx_effects = self.tx_effects.unwrap();
        let gcs = tx_effects.gas_cost_summary();
        let data_provider = ctx.data_provider();
        let system_state = data_provider.get_latest_sui_system_state().await?;
        let protocol_configs = data_provider.fetch_protocol_config(None).await?;
        let epoch = convert_to_epoch(gcs, &system_state, &protocol_configs)?;
        Ok(Some(epoch))
    }

    async fn status(&self) -> Option<ExecutionStatus> {
        let tx_effects = self.tx_effects.unwrap();
        Some(match tx_effects.status() {
            SuiExecutionStatus::Success => ExecutionStatus::Success,
            SuiExecutionStatus::Failure { error: _ } => ExecutionStatus::Failure,
        })
    }

    async fn errors(&self) -> Option<String> {
        let tx_effects = self.tx_effects.unwrap();
        match tx_effects.status() {
            SuiExecutionStatus::Success => None,
            SuiExecutionStatus::Failure { error } => Some(error.clone()),
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
