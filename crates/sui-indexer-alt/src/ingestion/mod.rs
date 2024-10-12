// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{bail, Context, Result};
use reqwest::Client;
use sui_storage::blob::Blob;
use sui_types::full_checkpoint_content::CheckpointData;
use url::Url;

pub struct IngestionClient {
    url: Url,
    client: Client,
}

impl IngestionClient {
    pub fn new(url: Url) -> Result<Self> {
        Ok(Self {
            url,
            client: Client::builder().build()?,
        })
    }

    pub async fn fetch(&self, checkpoint: u64) -> Result<Arc<CheckpointData>> {
        // SAFETY: The path being joined is statically known to be valid.
        let url = self
            .url
            .join(&format!("/{checkpoint}.chk"))
            .expect("Unexpected invalid URL");

        let response = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("Failed to fetch checkpoint {checkpoint}"))?;

        if !response.status().is_success() {
            if let Some(reason) = response.status().canonical_reason() {
                bail!("Failed to fetch checkpoint {checkpoint}: {reason}");
            } else {
                bail!(
                    "Failed to fetch checkpoint {checkpoint}: {}",
                    response.status()
                );
            }
        }

        let bytes = response
            .bytes()
            .await
            .with_context(|| format!("Failed to read checkpoint {checkpoint}"))?;

        let data: CheckpointData = Blob::from_bytes(&bytes)
            .with_context(|| format!("Failed to decode checkpoint {checkpoint}"))?;

        Ok(Arc::new(data))
    }
}
