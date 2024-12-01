// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod sdk;
use sdk::Result;

pub use reqwest;
use tap::Pipe;

use crate::types::ExecuteTransactionOptions;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSequenceNumber};
use sui_types::object::Object;
use sui_types::transaction::Transaction;

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
        parameters: &ExecuteTransactionOptions,
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

        let options = ExecuteTransactionOptions {
            effects_bcs: Some(true),
            events_bcs: Some(true),
            ..(parameters.to_owned())
        };

        let crate::types::ExecuteTransactionResponse {
            finality,
            effects: _,
            effects_bcs,
            events: _,
            events_bcs,
            balance_changes,
        } = self
            .inner
            .execute_transaction(&options, &signed_transaction)
            .await?
            .into_inner();

        TransactionExecutionResponse {
            finality,
            effects: bcs::from_bytes(
                effects_bcs
                    .as_deref()
                    .ok_or_else(|| sdk::Error::from_error("missing effects"))?,
            )?,
            events: events_bcs.as_deref().map(bcs::from_bytes).transpose()?,
            balance_changes,
        }
        .pipe(Ok)
    }
}

#[derive(Debug)]
pub struct TransactionExecutionResponse {
    pub finality: crate::types::EffectsFinality,

    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub balance_changes: Option<Vec<sui_sdk_types::types::BalanceChange>>,
}
