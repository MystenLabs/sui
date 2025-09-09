// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::Config;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use object_store::{path::Path, ObjectStore};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CertifiedCheckpointSummary;
use tracing::info;
use url::Url;

pub struct SuiObjectStore {
    store: Box<dyn ObjectStore>,
}

impl SuiObjectStore {
    pub fn new(config: &Config) -> Result<Self> {
        let url = Url::parse(&config.object_store_url)?;
        let (store, _) = object_store::parse_url(&url)?;
        Ok(Self { store })
    }

    pub async fn download_checkpoint_summary(
        &self,
        checkpoint_number: u64,
    ) -> Result<CertifiedCheckpointSummary> {
        let path = Path::from(format!("{}.chk", checkpoint_number));
        let response = self.store.get(&path).await?;
        let bytes = response.bytes().await?;

        let (_, blob) = bcs::from_bytes::<(u8, CheckpointData)>(&bytes)?;

        info!("Downloaded checkpoint summary: {}", checkpoint_number);
        Ok(blob.checkpoint_summary)
    }

    pub async fn get_full_checkpoint(&self, checkpoint_number: u64) -> Result<CheckpointData> {
        let path = Path::from(format!("{}.chk", checkpoint_number));
        info!("Request full checkpoint: {}", path);
        let response = self
            .store
            .get(&path)
            .await
            .map_err(|_| anyhow!("Cannot get full checkpoint from object store"))?;
        let bytes = response.bytes().await?;
        let (_, full_checkpoint) = bcs::from_bytes::<(u8, CheckpointData)>(&bytes)?;
        Ok(full_checkpoint)
    }
}

#[async_trait]
pub trait ObjectStoreExt {
    async fn get_checkpoint_summary(
        &self,
        checkpoint_number: u64,
    ) -> Result<CertifiedCheckpointSummary>;
}

#[async_trait]
impl ObjectStoreExt for SuiObjectStore {
    async fn get_checkpoint_summary(
        &self,
        checkpoint_number: u64,
    ) -> Result<CertifiedCheckpointSummary> {
        self.download_checkpoint_summary(checkpoint_number).await
    }
}

pub async fn download_checkpoint_summary(
    config: &Config,
    checkpoint_number: u64,
) -> Result<CertifiedCheckpointSummary> {
    let store = SuiObjectStore::new(config)?;
    store.get_checkpoint_summary(checkpoint_number).await
}
