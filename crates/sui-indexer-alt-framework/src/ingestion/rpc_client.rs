// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ingestion::client::{FetchError, FetchResult, IngestionClientTrait};
use anyhow::anyhow;
use bytes::Bytes;
use sui_rpc_api::Client;
use sui_storage::blob::{Blob, BlobEncoding};
use url::Url;

pub(crate) struct RpcIngestionClient {
    client: Client,
}

impl RpcIngestionClient {
    pub(crate) fn new(
        url: Url,
        basic_auth: Option<(String, String)>,
    ) -> Result<Self, anyhow::Error> {
        let mut client = Client::new(url.to_string())?;
        if let Some(basic_auth) = basic_auth {
            client = client.with_basic_auth(basic_auth)?;
        }
        Ok(Self { client })
    }
}

#[async_trait::async_trait]
impl IngestionClientTrait for RpcIngestionClient {
    async fn fetch(&self, checkpoint: u64) -> FetchResult {
        let data = self
            .client
            .get_full_checkpoint(checkpoint)
            .await
            .map_err(|e| {
                if e.message().contains("not found") {
                    FetchError::NotFound
                } else {
                    FetchError::Transient {
                        reason: "io_error",
                        error: anyhow!(e),
                    }
                }
            })?;
        Ok(Bytes::from(
            Blob::encode(&data, BlobEncoding::Bcs)?.to_bytes(),
        ))
    }
}
