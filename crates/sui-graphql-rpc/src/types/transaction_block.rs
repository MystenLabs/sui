// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::{
    context_ext::DataProviderContextExt, sui_sdk_data_provider::convert_to_epoch,
};

use super::{
    address::Address,
    base64::Base64,
    digest::Digest,
    epoch::Epoch,
    gas::{GasEffects, GasInput},
    sui_address::SuiAddress,
};
use crate::error::Error;
use async_graphql::*;
use sui_indexer::models_v2::transactions::StoredTransaction;
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionBlockDataAPI, SuiTransactionBlockEffects,
    SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse,
};
use sui_sdk::types::{
    effects::TransactionEffects,
    transaction::{SenderSignedData, TransactionDataAPI},
};

#[derive(SimpleObject, Clone, Eq, PartialEq)]
#[graphql(complex)]
pub(crate) struct TransactionBlock {
    #[graphql(skip)]
    pub digest: Digest,
    pub effects: Option<TransactionBlockEffects>,
    pub sender: Option<Address>,
    pub bcs: Option<Base64>,
    pub gas_input: Option<GasInput>,
}

impl From<SuiTransactionBlockResponse> for TransactionBlock {
    fn from(tx_block: SuiTransactionBlockResponse) -> Self {
        let transaction = tx_block.transaction.as_ref();
        let sender = transaction.map(|tx| Address {
            address: SuiAddress::from_array(tx.data.sender().to_inner()),
        });
        let gas_input = transaction.map(|tx| GasInput::from(tx.data.gas_data()));

        Self {
            digest: Digest::from_array(tx_block.digest.into_inner()),
            effects: tx_block.effects.as_ref().map(TransactionBlockEffects::from),
            sender,
            bcs: Some(Base64::from(&tx_block.raw_transaction)),
            gas_input,
        }
    }
}

impl TryFrom<StoredTransaction> for TransactionBlock {
    type Error = Error;

    fn try_from(tx: StoredTransaction) -> Result<Self, Self::Error> {
        // TODO (wlmyng): Split the below into resolver methods
        let digest = Digest::try_from(tx.transaction_digest.as_slice())?;

        let sender_signed_data: SenderSignedData =
            bcs::from_bytes(&tx.raw_transaction).map_err(|e| {
                Error::Internal(format!(
                    "Can't convert raw_transaction into SenderSignedData. Error: {e}",
                ))
            })?;

        let sender = Address {
            address: SuiAddress::from_array(
                sender_signed_data
                    .intent_message()
                    .value
                    .sender()
                    .to_inner(),
            ),
        };

        let gas_input = GasInput::from(sender_signed_data.intent_message().value.gas_data());
        let effects: TransactionEffects = bcs::from_bytes(&tx.raw_effects).map_err(|e| {
            Error::Internal(format!(
                "Can't convert raw_effects into TransactionEffects. Error: {e}",
            ))
        })?;
        let effects = match SuiTransactionBlockEffects::try_from(effects) {
            Ok(effects) => Ok(Some(TransactionBlockEffects::from(&effects))),
            Err(e) => Err(Error::Internal(format!(
                "Can't convert TransactionEffects into SuiTransactionBlockEffects. Error: {e}",
            ))),
        }?;

        Ok(Self {
            digest,
            effects,
            sender: Some(sender),
            bcs: Some(Base64::from(&tx.raw_transaction)),
            gas_input: Some(gas_input),
        })
    }
}

#[ComplexObject]
impl TransactionBlock {
    async fn digest(&self) -> String {
        self.digest.to_string()
    }

    async fn expiration(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        if self.effects.is_none() {
            return Ok(None);
        }
        let gcs = self.effects.as_ref().unwrap().gas_effects.gcs;
        let data_provider = ctx.data_provider();
        let system_state = data_provider.get_latest_sui_system_state().await?;
        let protocol_configs = data_provider.fetch_protocol_config(None).await?;
        let epoch = convert_to_epoch(gcs, &system_state, &protocol_configs)?;
        Ok(Some(epoch))
    }
}

#[derive(Clone, Eq, PartialEq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct TransactionBlockEffects {
    #[graphql(skip)]
    pub digest: Digest,
    #[graphql(skip)]
    pub gas_effects: GasEffects,
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

impl From<&SuiTransactionBlockEffects> for TransactionBlockEffects {
    fn from(tx_effects: &SuiTransactionBlockEffects) -> Self {
        let (status, errors) = match tx_effects.status() {
            SuiExecutionStatus::Success => (ExecutionStatus::Success, None),
            SuiExecutionStatus::Failure { error } => {
                (ExecutionStatus::Failure, Some(error.clone()))
            }
        };

        Self {
            // TODO (wlmyng): To remove as this is the wrong digest, effects digest is not a field on SuiTransactionBlockEffects
            digest: Digest::from_array(tx_effects.transaction_digest().into_inner()),
            gas_effects: GasEffects::from((tx_effects.gas_cost_summary(), tx_effects.gas_object())),
            status,
            errors,
        }
    }
}

#[ComplexObject]
impl TransactionBlockEffects {
    async fn digest(&self) -> String {
        self.digest.to_string()
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
    SystemTx = 0,
    ProgrammableTx = 1,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum ExecutionStatus {
    Success,
    Failure,
}

#[derive(InputObject)]
pub(crate) struct TransactionBlockFilter {
    pub package: Option<SuiAddress>,
    pub module: Option<String>,
    pub function: Option<String>,

    pub kind: Option<TransactionBlockKindInput>,
    pub checkpoint: Option<u64>,

    pub sign_address: Option<SuiAddress>,
    pub sent_address: Option<SuiAddress>,
    pub recv_address: Option<SuiAddress>,
    pub paid_address: Option<SuiAddress>,

    pub input_object: Option<SuiAddress>,
    pub changed_object: Option<SuiAddress>,
}
