// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSequenceNumber};
use sui_types::object::Object;

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

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_JSON)
            .send()
            .await?;

        self.json(response).await
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

    fn check_response(&self, response: reqwest::Response) -> Result<reqwest::Response> {
        if !response.status().is_success() {
            let status = response.status();
            return Err(anyhow::anyhow!("request failed with status {status}"));
        }

        Ok(response)
    }

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
