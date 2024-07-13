// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::ListCheckpointsQueryParameters;
use crate::transactions::ExecuteTransactionQueryParameters;
use anyhow::{anyhow, Result};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::crypto::AuthorityStrongQuorumSignInfo;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSequenceNumber};
use sui_types::object::Object;
use sui_types::transaction::Transaction;
use sui_types::TypeTag;

#[derive(Clone)]
pub struct Client {
    inner: reqwest::Client,
    base_url: String,
}

impl Client {
    pub fn new<S: Into<String>>(base_url: S) -> Self {
        Self {
            inner: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }

    pub async fn get_latest_checkpoint(&self) -> Result<CertifiedCheckpointSummary> {
        let url = format!("{}/checkpoints", self.base_url);

        let query = ListCheckpointsQueryParameters {
            limit: Some(1),
            start: None,
            direction: None,
        };

        let response = self
            .inner
            .get(url)
            .query(&query)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        let mut page: Vec<CertifiedCheckpointSummary> = self.bcs(response).await?;

        page.pop()
            .ok_or_else(|| anyhow!("server returned empty checkpoint list"))
    }

    pub async fn get_full_checkpoint(
        &self,
        checkpoint_sequence_number: CheckpointSequenceNumber,
    ) -> Result<CheckpointData> {
        let url = format!(
            "{}/checkpoints/{checkpoint_sequence_number}/full",
            self.base_url
        );

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn get_checkpoint_summary(
        &self,
        checkpoint_sequence_number: CheckpointSequenceNumber,
    ) -> Result<CertifiedCheckpointSummary> {
        let url = format!("{}/checkpoints/{checkpoint_sequence_number}", self.base_url);

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn get_object(&self, object_id: ObjectID) -> Result<Object> {
        let url = format!("{}/objects/{object_id}", self.base_url);

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn get_object_with_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Object> {
        let url = format!("{}/objects/{object_id}/version/{version}", self.base_url);

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn execute_transaction(
        &self,
        parameters: &ExecuteTransactionQueryParameters,
        transaction: &Transaction,
    ) -> Result<TransactionExecutionResponse> {
        #[derive(serde::Serialize)]
        struct SignedTransaction<'a> {
            transaction: &'a sui_types::transaction::TransactionData,
            signatures: &'a [sui_types::signature::GenericSignature],
        }

        let url = format!("{}/transactions", self.base_url);
        let body = bcs::to_bytes(&SignedTransaction {
            transaction: &transaction.inner().intent_message.value,
            signatures: &transaction.inner().tx_signatures,
        })?;

        let response = self
            .inner
            .post(url)
            .query(parameters)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .header(reqwest::header::CONTENT_TYPE, crate::APPLICATION_BCS)
            .body(body)
            .send()
            .await?;

        self.bcs(response).await
    }

    fn check_response(&self, response: reqwest::Response) -> Result<reqwest::Response> {
        if !response.status().is_success() {
            let status = response.status();
            return Err(anyhow::anyhow!("request failed with status {status}"));
        }

        Ok(response)
    }

    #[allow(unused)]
    async fn json<T: serde::de::DeserializeOwned>(&self, response: reqwest::Response) -> Result<T> {
        let response = self.check_response(response)?;

        let json = response.json().await?;
        Ok(json)
    }

    async fn bcs<T: serde::de::DeserializeOwned>(&self, response: reqwest::Response) -> Result<T> {
        let response = self.check_response(response)?;

        let bytes = response.bytes().await?;
        let bcs = bcs::from_bytes(&bytes)?;
        Ok(bcs)
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
