// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ingestion::client::{FetchData, FetchError, FetchResult, IngestionClientTrait};
use anyhow::anyhow;
use sui_rpc_api::Client as RpcClient;
use tonic::Code;

#[async_trait::async_trait]
impl IngestionClientTrait for RpcClient {
    async fn fetch(&self, checkpoint: u64) -> FetchResult {
        let data = self
            .get_full_checkpoint(checkpoint)
            .await
            .map_err(|status| match status.code() {
                Code::NotFound => FetchError::NotFound,
                _ => FetchError::Transient {
                    reason: "get_full_checkpoint",
                    error: anyhow!(status),
                },
            })?;
        Ok(FetchData::CheckpointData(data))
    }
}
