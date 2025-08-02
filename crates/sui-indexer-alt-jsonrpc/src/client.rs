// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sui_json_rpc_types::{Balance, DelegatedStake, ValidatorApys, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions, DryRunTransactionBlockResponse};
use sui_types::{base_types::{ObjectID, SuiAddress}, quorum_driver_types::ExecuteTransactionRequestType};
use fastcrypto::encoding::Base64;

#[derive(Debug)]
pub struct HttpClient {
    client: reqwest::Client,
    url: String,
    request_id_counter: AtomicU64,
}

impl Clone for HttpClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            url: self.url.clone(),
            request_id_counter: AtomicU64::new(1),
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    params: Value,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: String,
    id: u64,
    #[serde(flatten)]
    result_or_error: JsonRpcResultOrError<T>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum JsonRpcResultOrError<T> {
    Result { result: T },
    Error { error: JsonRpcError },
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[allow(dead_code)]
    data: Option<Value>,
}

impl HttpClient {
    pub fn new(
        url: String,
        headers: HeaderMap,
        _max_request_size: u32,
        connection_timeout: Duration,
        connection_idle_timeout: Duration,
    ) -> Result<Self> {
        let client = reqwest::ClientBuilder::new()
            .default_headers(headers)
            .timeout(connection_timeout)
            .pool_idle_timeout(Some(connection_idle_timeout))
            .build()
            .context("Failed to create reqwest client")?;

        Ok(Self {
            client,
            url,
            request_id_counter: AtomicU64::new(1),
        })
    }

    pub async fn call<T, R>(&self, method: &str, params: T) -> Result<R>
    where
        T: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        let id = self.request_id_counter.fetch_add(1, Ordering::SeqCst);
        
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params: serde_json::to_value(params).context("Failed to serialize params")?,
        };

        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP error: {}", response.status());
        }

        let response_text = response
            .text()
            .await
            .context("Failed to read response text")?;

        let rpc_response: JsonRpcResponse<R> = serde_json::from_str(&response_text)
            .with_context(|| format!("Failed to parse JSON-RPC response: {}", response_text))?;

        if rpc_response.id != id {
            anyhow::bail!("Response ID mismatch: expected {}, got {}", id, rpc_response.id);
        }

        match rpc_response.result_or_error {
            JsonRpcResultOrError::Result { result } => Ok(result),
            JsonRpcResultOrError::Error { error } => {
                anyhow::bail!("JSON-RPC error {}: {}", error.code, error.message)
            }
        }
    }
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for JsonRpcError {}

// Direct method implementations that match the interface expected by the server implementations
impl HttpClient {
    pub async fn get_all_balances(&self, owner: SuiAddress) -> Result<Vec<Balance>> {
        self.call("suix_getAllBalances", (owner,)).await
    }

    pub async fn get_balance(&self, owner: SuiAddress, coin_type: Option<String>) -> Result<Balance> {
        self.call("suix_getBalance", (owner, coin_type)).await
    }

    pub async fn get_stakes_by_ids(&self, staked_sui_ids: Vec<ObjectID>) -> Result<Vec<DelegatedStake>> {
        self.call("suix_getStakesByIds", (staked_sui_ids,)).await
    }

    pub async fn get_stakes(&self, owner: SuiAddress) -> Result<Vec<DelegatedStake>> {
        self.call("suix_getStakes", (owner,)).await
    }

    pub async fn get_validators_apy(&self) -> Result<ValidatorApys> {
        self.call("suix_getValidatorsApy", ()).await
    }

    pub async fn execute_transaction_block(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        options: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> Result<SuiTransactionBlockResponse> {
        self.call("sui_executeTransactionBlock", (tx_bytes, signatures, options, request_type)).await
    }

    pub async fn dry_run_transaction_block(&self, tx_bytes: Base64) -> Result<DryRunTransactionBlockResponse> {
        self.call("sui_dryRunTransactionBlock", (tx_bytes,)).await
    }
}