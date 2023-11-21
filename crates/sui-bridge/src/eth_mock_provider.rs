// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A mock implementation of Ethereum JSON-RPC client, based on `MockProvider` from `ethers-rs`.

use ethers::providers::JsonRpcClient;
use ethers::providers::MockError;
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use serde_json::Value;
use std::{
    borrow::Borrow,
    sync::{Arc, Mutex},
};
// use thiserror::Error;

/// Helper type that can be used to pass through the `params` value.
/// This is necessary because the wrapper provider is supposed to skip the `params` if it's of
/// size 0, see `crate::transports::common::Request`
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
enum MockParams {
    Value(String),
    Zst,
}

#[derive(Clone, Debug)]
/// Mock transport used in test environments.
pub struct EthMockProvider {
    // requests: Arc<Mutex<VecDeque<(String, MockParams)>>>,
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

    /// Pushes the `(method, params)` to the back of the `requests` queue,
    /// pops the responses from the back of the `responses` queue
    async fn request<P: Serialize + Send + Sync, R: DeserializeOwned>(
        &self,
        method: &str,
        params: P,
    ) -> Result<R, MockError> {
        let params = if std::mem::size_of::<P>() == 0 {
            MockParams::Zst
        } else {
            MockParams::Value(serde_json::to_value(params)?.to_string())
        };
        // self.requests.lock().unwrap().push_back((method.to_owned(), params));
        println!("request: {:?} {:?}", method.to_owned(), params);
        let element = self.responses.lock().unwrap().get(&(method.to_owned(), params)).ok_or(MockError::EmptyResponses)?.clone();
        // let element = data.pop_back().ok_or(MockError::EmptyResponses)?;
        let res: R = serde_json::from_value(element)?;

        Ok(res)
    }
}

impl EthMockProvider {
    /// Checks that the provided request was submitted by the client
    // pub fn assert_request<T: Serialize + Send + Sync>(
    //     &self,
    //     method: &str,
    //     data: T,
    // ) -> Result<(), MockError> {
    //     let (m, inp) = self.requests.lock().unwrap().pop_front().ok_or(MockError::EmptyRequests)?;
    //     assert_eq!(m, method);
    //     assert!(!matches!(inp, MockParams::Value(serde_json::Value::Null)));
    //     if std::mem::size_of::<T>() == 0 {
    //         assert!(matches!(inp, MockParams::Zst));
    //     } else if let MockParams::Value(inp) = inp {
    //         assert_eq!(serde_json::to_value(data).expect("could not serialize data"), inp);
    //     } else {
    //         unreachable!("Zero sized types must be denoted with MockParams::Zst")
    //     }

    //     Ok(())
    // }

    /// Instantiates a mock transport
    pub fn new() -> Self {
        Self {
            // requests: Arc::new(Mutex::new(VecDeque::new())),
            responses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Pushes the data to the responses
    pub fn push<P: Serialize + Send + Sync, T: Serialize + Send + Sync, K: Borrow<T>>(
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
        println!("pushing: {:?} {:?} {:?}", method.to_owned(), params, value);
        self.responses.lock().unwrap().insert((method.to_owned(), params), value);
        Ok(())
    }
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use ethers::{types::U64, providers::Middleware};

    #[tokio::test]
    async fn test_basic_responses_match() {
        let mock = EthMockProvider::new();
        
        mock.push("eth_blockNumber", (), U64::from(12)).unwrap();
        let block: U64 = mock.request("eth_blockNumber", ()).await.unwrap();

        assert_eq!(block.as_u64(), 12);
        let block: U64 = mock.request("eth_blockNumber", ()).await.unwrap();
        assert_eq!(block.as_u64(), 12);

        mock.push("eth_blockNumber", (), U64::from(13)).unwrap();
        let block: U64 = mock.request("eth_blockNumber", ()).await.unwrap();
        assert_eq!(block.as_u64(), 13);

        mock.push("eth_foo", (), U64::from(0)).unwrap();
        let block: U64 = mock.request("eth_blockNumber", ()).await.unwrap();
        assert_eq!(block.as_u64(), 13);

        let err = mock.request::<_, ()>("eth_blockNumber", "bar").await.unwrap_err();
        match err {
            MockError::EmptyResponses => {}
            _ => panic!("expected empty responses"),
        };

        mock.push("eth_blockNumber", "bar", U64::from(14)).unwrap();
        let block: U64 = mock.request("eth_blockNumber", "bar").await.unwrap();
        assert_eq!(block.as_u64(), 14);
    }

    // #[tokio::test]
    // async fn empty_responses() {
    //     let mock = MockProvider::new();
    //     // tries to get a response without pushing a response
    //     let err = mock.request::<_, ()>("eth_blockNumber", ()).await.unwrap_err();
    //     match err {
    //         MockError::EmptyResponses => {}
    //         _ => panic!("expected empty responses"),
    //     };

    // }

    // #[tokio::test]
    // async fn empty_requests() {
    //     let mock = MockProvider::new();
    //     // tries to assert a request without making one
    //     let err = mock.assert_request("eth_blockNumber", ()).unwrap_err();
    //     match err {
    //         MockError::EmptyRequests => {}
    //         _ => panic!("expected empty request"),
    //     };
    // }

    #[tokio::test]
    async fn composes_with_provider() {
        let mock = EthMockProvider::new();
        let provider = ethers::providers::Provider::new(mock.clone());

        mock.push("eth_blockNumber", (), U64::from(12)).unwrap();
        let block = provider.get_block_number().await.unwrap();
        assert_eq!(block.as_u64(), 12);
    }
}
