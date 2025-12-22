// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A mock implementation of Ethereum JSON-RPC client, using alloy.

use alloy::{
    providers::RootProvider,
    rpc::{
        client::RpcClient,
        json_rpc::{RequestPacket, Response, ResponsePacket, SerializedRequest},
    },
    transports::{TransportError, TransportErrorKind},
};
use serde::Serialize;
use serde_json::{Value, value::RawValue};
use std::fmt::Debug;
use std::{
    borrow::Borrow,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};
use std::{collections::HashMap, pin::Pin};
use tower::Service;

use crate::utils::EthProvider;

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
pub struct EthMockService {
    responses: Arc<Mutex<HashMap<(String, MockParams), Value>>>,
}

impl Default for EthMockService {
    fn default() -> Self {
        Self::new()
    }
}

impl Service<RequestPacket> for EthMockService {
    type Response = ResponsePacket;
    type Error = TransportError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: RequestPacket) -> Self::Future {
        let responses = self.responses.clone();

        Box::pin(async move {
            match req {
                RequestPacket::Single(request) => {
                    let response = process_request(&responses, &request)?;
                    Ok(ResponsePacket::Single(response))
                }
                RequestPacket::Batch(requests) => {
                    let responses: Result<Vec<_>, _> = requests
                        .iter()
                        .map(|req| process_request(&responses, req))
                        .collect();
                    Ok(ResponsePacket::Batch(responses?))
                }
            }
        })
    }
}

fn process_request(
    responses: &Arc<Mutex<HashMap<(String, MockParams), Value>>>,
    request: &SerializedRequest,
) -> Result<Response<Box<RawValue>>, TransportError> {
    let method = request.method();
    let params = request.params();

    let mock_params = match params {
        Some(value) if value.get() != "null" => MockParams::Value(value.to_string()),
        _ => MockParams::Zst,
    };

    let guard = responses.lock().unwrap();
    let value = guard
        .get(&(method.to_string(), mock_params))
        .ok_or_else(|| TransportErrorKind::custom(EthMockError::EmptyResponses))?
        .clone();
    let raw_value = RawValue::from_string(value.to_string()).map_err(TransportErrorKind::custom)?;

    Ok(Response {
        id: request.id().clone(),
        payload: alloy::rpc::json_rpc::ResponsePayload::Success(raw_value),
    })
}

#[derive(Debug, thiserror::Error)]
pub enum EthMockError {
    #[error("no response found for method and params")]
    EmptyResponses,
    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

impl EthMockService {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn as_provider(&self) -> EthProvider {
        let client = RpcClient::new(self.clone(), true);
        Arc::new(RootProvider::new(client))
    }

    pub fn add_response<P: Serialize + Send + Sync, T: Serialize + Send + Sync, K: Borrow<T>>(
        &self,
        method: &str,
        params: P,
        data: K,
    ) -> Result<(), EthMockError> {
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
    use alloy::{primitives::U64, providers::Provider};

    #[tokio::test]
    async fn test_with_provider() {
        let mock_service = EthMockService::new();
        let mock_provider = mock_service.as_provider();

        mock_service
            .add_response("eth_blockNumber", (), U64::from(12))
            .unwrap();
        let block: U64 = mock_provider
            .raw_request("eth_blockNumber".into(), ())
            .await
            .unwrap();

        assert_eq!(block, 12);
        let block: U64 = mock_provider
            .raw_request("eth_blockNumber".into(), ())
            .await
            .unwrap();
        assert_eq!(block, 12);

        mock_service
            .add_response("eth_blockNumber", (), U64::from(13))
            .unwrap();
        let block: U64 = mock_provider
            .raw_request("eth_blockNumber".into(), ())
            .await
            .unwrap();
        assert_eq!(block, 13);

        mock_service
            .add_response("eth_foo", (), U64::from(0))
            .unwrap();
        let block: U64 = mock_provider
            .raw_request("eth_blockNumber".into(), ())
            .await
            .unwrap();
        assert_eq!(block, 13);

        let err = mock_provider
            .raw_request::<_, U64>("eth_blockNumber".into(), "bar")
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("no response found for method and params")
        );

        mock_service
            .add_response("eth_blockNumber", "bar", U64::from(14))
            .unwrap();
        let block: U64 = mock_provider
            .raw_request("eth_blockNumber".into(), "bar")
            .await
            .unwrap();
        assert_eq!(block, 14);
    }
}
