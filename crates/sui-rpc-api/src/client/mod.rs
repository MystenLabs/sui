// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod sdk;
use sdk::Result;

pub use reqwest;

use crate::rest::transactions::ExecuteTransactionQueryParameters;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::crypto::AuthorityStrongQuorumSignInfo;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSequenceNumber};
use sui_types::object::Object;
use sui_types::transaction::Transaction;
use sui_types::TypeTag;

use self::sdk::Response;

#[derive(Clone)]
pub struct Client {
    inner: sdk::Client,
}

impl Client {
    pub fn new<S: AsRef<str>>(base_url: S) -> Self {
        Self {
            inner: sdk::Client::new(base_url.as_ref()).unwrap(),
        }
    }

    pub fn inner(&self) -> &sdk::Client {
        &self.inner
    }

    pub async fn get_latest_checkpoint(&self) -> Result<CertifiedCheckpointSummary> {
        self.inner
            .get_latest_checkpoint()
            .await
            .map(Response::into_inner)
            .and_then(|checkpoint| checkpoint.try_into().map_err(Into::into))
    }

    pub async fn get_full_checkpoint(
        &self,
        checkpoint_sequence_number: CheckpointSequenceNumber,
    ) -> Result<CheckpointData> {
        let url = self
            .inner
            .url()
            .join(&format!("checkpoints/{checkpoint_sequence_number}/full"))?;

        let request = self.inner.client().get(url);

        self.inner.bcs(request).await.map(Response::into_inner)
    }

    pub async fn get_checkpoint_summary(
        &self,
        checkpoint_sequence_number: CheckpointSequenceNumber,
    ) -> Result<CertifiedCheckpointSummary> {
        self.inner
            .get_checkpoint(checkpoint_sequence_number)
            .await
            .map(Response::into_inner)
            .and_then(|checkpoint| {
                sui_sdk_types::types::SignedCheckpointSummary {
                    checkpoint: checkpoint.summary.unwrap(),
                    signature: checkpoint.signature.unwrap(),
                }
                .try_into()
                .map_err(Into::into)
            })
    }

    pub async fn get_object(&self, object_id: ObjectID) -> Result<Object> {
        self.inner
            .get_object(object_id.into())
            .await
            .map(Response::into_inner)
            .and_then(|object| object.try_into().map_err(Into::into))
    }

    pub async fn get_object_with_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Object> {
        self.inner
            .get_object_with_version(object_id.into(), version.into())
            .await
            .map(Response::into_inner)
            .and_then(|object| object.try_into().map_err(Into::into))
    }

    pub async fn execute_transaction(
        &self,
        parameters: &ExecuteTransactionQueryParameters,
        transaction: &Transaction,
    ) -> Result<TransactionExecutionResponse> {
        let signed_transaction = sui_sdk_types::types::SignedTransaction {
            transaction: transaction
                .inner()
                .intent_message
                .value
                .clone()
                .try_into()?,
            signatures: transaction
                .inner()
                .tx_signatures
                .clone()
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        };

        let response = self
            .inner
            .execute_transaction(parameters, &signed_transaction)
            .await?
            .into_inner();
        bcs::from_bytes(&bcs::to_bytes(&response)?).map_err(Into::into)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TransactionExecutionResponse {
    pub effects: TransactionEffects,

    pub finality: EffectsFinality,
    pub events: Option<TransactionEvents>,
    pub balance_changes: Option<Vec<BalanceChange>>,
    pub input_objects: Option<Vec<Object>>,
    pub output_objects: Option<Vec<Object>>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum EffectsFinality {
    Certified {
        signature: AuthorityStrongQuorumSignInfo,
    },
    Checkpointed {
        checkpoint: CheckpointSequenceNumber,
    },
}

#[derive(PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct BalanceChange {
    /// Owner of the balance change
    pub address: SuiAddress,
    /// Type of the Coin
    pub coin_type: TypeTag,
    /// The amount indicate the balance value changes,
    /// negative amount means spending coin value and positive means receiving coin value.
    pub amount: i128,
}
