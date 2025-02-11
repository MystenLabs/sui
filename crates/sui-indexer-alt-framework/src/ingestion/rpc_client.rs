// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ingestion::client::{FetchData, FetchError, FetchResult, IngestionClientTrait};
use anyhow::anyhow;
use sui_rpc_api::Client as RpcClient;

#[async_trait::async_trait]
impl IngestionClientTrait for RpcClient {
    async fn fetch(&self, checkpoint: u64) -> FetchResult {
        let data = self.get_full_checkpoint(checkpoint).await.map_err(|e| {
            if e.message().contains("not found") {
                FetchError::NotFound
            } else {
                FetchError::Transient {
                    reason: "get_full_checkpoint",
                    error: anyhow!(e),
                }
            }
        })?;
        Ok(FetchData::CheckPointData(data))
    }
}
