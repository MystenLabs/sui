// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{BigTableClient, KeyValueStoreReader, KeyValueStoreWriter};
use anyhow::Result;
use async_trait::async_trait;
use sui_data_ingestion_core::ProgressStore;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

pub struct BigTableProgressStore {
    client: BigTableClient,
}

impl BigTableProgressStore {
    pub fn new(client: BigTableClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ProgressStore for BigTableProgressStore {
    async fn load(&mut self, _: String) -> Result<CheckpointSequenceNumber> {
        self.client.get_latest_checkpoint().await
    }

    async fn save(&mut self, _: String, checkpoint_number: CheckpointSequenceNumber) -> Result<()> {
        self.client.save_watermark(checkpoint_number).await
    }
}
