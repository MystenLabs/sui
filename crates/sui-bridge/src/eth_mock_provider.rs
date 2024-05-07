// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A mock implementation of Ethereum JSON-RPC client, based on `MockProvider` from `ethers-rs`.

use async_trait::async_trait;
use ethers::providers::JsonRpcClient;
use ethers::providers::MockError;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Debug;
use std::{
    borrow::Borrow,
    sync::{Arc, Mutex},
};

/// Helper type that can be used to pass through the `params` value.
/// This is necessary because the wrapper provider is supposed to skip the `params` if it's of
/// size 0, see `crate::transports::common::Request`
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
enum MockParams {
    Value(String),
    Zst,
}

/// Mock transport used in test environments.
#[derive(Clone, Debug)]
pub struct EthMockProvider {
    responses: Arc<Mutex<HashMap<(String, MockParams), Value>>>,
}

impl Default for EthMockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl JsonRpcClient for EthMockProvider {
    type Error = MockError;

    /// If `method` and `params` match previously set response by
    /// `add_response`, return the response. Otherwise return
    /// MockError::EmptyResponses.
    async fn request<P: Serialize + Send + Sync + Debug, R: DeserializeOwned>(
        &self,
        method: &str,
        params: P,
    ) -> Result<R, MockError> {
        let params = if std::mem::size_of::<P>() == 0 {
            MockParams::Zst
        } else {
            MockParams::Value(serde_json::to_value(params)?.to_string())
        };
        let element = self
            .responses
            .lock()
            .unwrap()
            .get(&(method.to_owned(), params))
            .ok_or(MockError::EmptyResponses)?
            .clone();
        let res: R = serde_json::from_value(element)?;

        Ok(res)
    }
}

impl EthMockProvider {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn add_response<P: Serialize + Send + Sync, T: Serialize + Send + Sync, K: Borrow<T>>(
        &self,
        method: &str,
        params: P,
        data: K,
    ) -> Result<(), MockError> {
        let params = if std::mem::size_of::<P>() == 0 {
            MockParams::Zst
        } else {
            MockParams::Value(serde_json::to_value(params)?.to_string())
        };
        let value = serde_json::to_value(data.borrow())?;
        self.responses
            .lock()
            .unwrap()
            .insert((method.to_owned(), params), value);
        Ok(())
    }
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use ethers::{providers::Middleware, types::U64};

    #[tokio::test]
    async fn test_basic_responses_match() {
        let mock = EthMockProvider::new();

        mock.add_response("eth_blockNumber", (), U64::from(12))
            .unwrap();
        let block: U64 = mock.request("eth_blockNumber", ()).await.unwrap();

        assert_eq!(block.as_u64(), 12);
        let block: U64 = mock.request("eth_blockNumber", ()).await.unwrap();
        assert_eq!(block.as_u64(), 12);

        mock.add_response("eth_blockNumber", (), U64::from(13))
            .unwrap();
        let block: U64 = mock.request("eth_blockNumber", ()).await.unwrap();
        assert_eq!(block.as_u64(), 13);

        mock.add_response("eth_foo", (), U64::from(0)).unwrap();
        let block: U64 = mock.request("eth_blockNumber", ()).await.unwrap();
        assert_eq!(block.as_u64(), 13);

        let err = mock
            .request::<_, ()>("eth_blockNumber", "bar")
            .await
            .unwrap_err();
        match err {
            MockError::EmptyResponses => {}
            _ => panic!("expected empty responses"),
        };

        mock.add_response("eth_blockNumber", "bar", U64::from(14))
            .unwrap();
        let block: U64 = mock.request("eth_blockNumber", "bar").await.unwrap();
        assert_eq!(block.as_u64(), 14);
    }

    #[tokio::test]
    async fn test_with_provider() {
        let mock = EthMockProvider::new();
        let provider = ethers::providers::Provider::new(mock.clone());

        mock.add_response("eth_blockNumber", (), U64::from(12))
            .unwrap();
        let block = provider.get_block_number().await.unwrap();
        assert_eq!(block.as_u64(), 12);
    }
}
