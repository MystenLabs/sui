// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BlockAddresses {
    pub addresses: Vec<BlockAddress>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
pub struct BlockAddress {
    pub source_address: String,
    pub destination_port: u16,
    pub ttl: u64,
}

pub struct NodeFWClient {
    client: reqwest::Client,
    remote_fw_url: String,
}

impl NodeFWClient {
    pub fn new(remote_fw_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            remote_fw_url,
        }
    }

    pub async fn block_addresses(&self, addresses: BlockAddresses) -> Result<(), reqwest::Error> {
        let response = self
            .client
            .post(format!("{}/block_addresses", self.remote_fw_url))
            .json(&addresses)
            .send()
            .await?;
        match response.error_for_status() {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn list_addresses(&self) -> Result<BlockAddresses, reqwest::Error> {
        self.client
            .get(format!("{}/list_addresses", self.remote_fw_url))
            .send()
            .await?
            .error_for_status()?
            .json::<BlockAddresses>()
            .await
    }
}
