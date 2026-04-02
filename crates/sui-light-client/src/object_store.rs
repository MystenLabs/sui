// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::Config;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use sui_storage::object_store::util::fetch_checkpoint;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CertifiedCheckpointSummary;
use tracing::info;
use url::Url;

pub struct SuiObjectStore {
    store: Arc<dyn object_store::ObjectStore>,
}

impl SuiObjectStore {
    pub fn new(config: &Config) -> Result<Self> {
        let url = Url::parse(&config.object_store_url)?;
        let (store, _) = object_store::parse_url(&url)?;
        Ok(Self {
            store: Arc::new(store),
        })
    }

    pub async fn download_checkpoint_summary(
        &self,
        checkpoint_number: u64,
    ) -> Result<CertifiedCheckpointSummary> {
        let checkpoint = fetch_checkpoint(&self.store, checkpoint_number).await?;
        info!("Downloaded checkpoint summary: {}", checkpoint_number);
        Ok(checkpoint.summary)
    }

    pub async fn get_full_checkpoint(&self, checkpoint_number: u64) -> Result<CheckpointData> {
        let checkpoint = fetch_checkpoint(&self.store, checkpoint_number).await?;
        info!("Request full checkpoint: {}", checkpoint_number);
        Ok(CheckpointData::from(checkpoint))
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
