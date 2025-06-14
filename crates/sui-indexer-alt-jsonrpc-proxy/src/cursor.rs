// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Adapted from sui-rpc-benchmark/src/json_rpc/runner.rs

use anyhow::{Context as _, Result};
use moka::policy::EvictionPolicy;
use moka::sync::Cache;
use phf::phf_map;
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, error};

/// static map of method names to the index of their cursor parameter
static METHOD_CURSOR_POSITIONS: phf::Map<&'static str, usize> = phf_map! {
    // based on function headers in crates/sui-json-rpc-api/src/indexer.rs
    "suix_getOwnedObjects" => 2,
    "suix_queryTransactionBlocks" => 1,
    // based on function headers in crates/sui-json-rpc-api/src/coin.rs
    "suix_getCoins" => 2,
    "suix_getAllCoins" => 1,
};

static METHOD_LENGTHS: phf::Map<&'static str, usize> = phf_map! {
    // based on function headers in crates/sui-json-rpc-api/src/indexer.rs
    "suix_getOwnedObjects" => 4,
    "suix_queryTransactionBlocks" => 4,
    // based on function headers in crates/sui-json-rpc-api/src/coin.rs
    "suix_getCoins" => 4,
    "suix_getAllCoins" => 3,
};

/// Tracks pagination state for active pagination requests
/// The key is a tuple of method name and the params `Vec<Value>`, where the cursor parameter is set to `null`.
/// The value is the cursor for the next page.
#[derive(Clone, Debug)]
pub struct PaginationCursorState {
    // TODO: potential optimization to condense the key so we can store more in the cache.
    requests: Cache<(String, Vec<Value>), Value>,
}

impl PaginationCursorState {
    pub fn new(cursor_cache_size: u64) -> Self {
        Self {
            requests: Cache::builder()
                .max_capacity(cursor_cache_size)
                .eviction_policy(EvictionPolicy::lru())
                .build(),
        }
    }

    /// Returns the index of the cursor parameter for a method, if it exists;
    /// Otherwise, it means no cursor transformation is needed for this method.
    pub fn get_method_cursor_index(method: &str) -> Option<usize> {
        METHOD_CURSOR_POSITIONS.get(method).copied()
    }

    /// Given a method and its paramters, returns the key to be used to store the cursor in the cache.
    /// The cursor parameter will be set to `null` in the key.
    fn get_method_key(
        method: &str,
        params: &[Value],
    ) -> Result<(String, Vec<Value>), anyhow::Error> {
        let cursor_idx = METHOD_CURSOR_POSITIONS
            .get(method)
            .with_context(|| format!("method {} not found in cursor positions", method))?;
        let mut key_params = params.to_vec();
        if let Some(param_to_modify) = key_params.get_mut(*cursor_idx) {
            *param_to_modify = Value::Null;
        } else {
            let method_length = METHOD_LENGTHS
                .get(method)
                .with_context(|| format!("method {} not found in method lengths", method))?;
            key_params.resize(*method_length, Value::Null);
        }
        Ok((method.to_string(), key_params))
    }

    /// In place updates the cursor parameter in the `params` array of a request to the `new_cursor`.
    fn update_params_cursor(
        params: &mut Value,
        cursor_idx: usize,
        new_cursor: Option<&Value>,
        method: &str,
    ) -> Result<(), anyhow::Error> {
        let params_array = params
            .get_mut("params")
            .and_then(|v| v.as_array_mut())
            .with_context(|| format!("params not found or not an array for method {}", method))?;
        // If the cursor parameter is not present, extend the array to include it.
        if params_array.len() <= cursor_idx {
            let method_length = METHOD_LENGTHS
                .get(method)
                .with_context(|| format!("method {} not found in method lengths", method))?;
            params_array.resize(*method_length, Value::Null);
        }
        let param_to_modify = params_array.get_mut(cursor_idx).with_context(|| {
            format!(
                "Failed to access cursor parameter at index {} for method {}",
                cursor_idx, method
            )
        })?;
        *param_to_modify = match new_cursor {
            Some(cursor) => cursor.clone(),
            None => Value::Null,
        };
        Ok(())
    }

    /// Updates the stored cursor for a given method and parameters.
    fn update(&self, key: (String, Vec<Value>), cursor: Option<Value>) {
        if let Some(cursor) = cursor {
            self.requests.insert(key, cursor);
        } else {
            self.requests.remove(&key);
        }
    }

    /// Returns a stored cursor for a given method and parameters.
    /// The cursor value is originally read from the response of a successful previous request.
    fn get(&self, key: &(String, Vec<Value>)) -> Option<Value> {
        self.requests.get(key)
    }
}

/// Transforms the `json_body` of a request to update the cursor parameter to the cached value.
/// Returns true if the cursor was updated, false otherwise.
pub fn transform_json_body(
    json_body: &mut Value,
    method: &str,
    params: &[Value],
    pagination_state: &PaginationCursorState,
) -> Result<bool, anyhow::Error> {
    if let Some(cursor_idx) = PaginationCursorState::get_method_cursor_index(method) {
        if !params.is_empty() {
            let method_key = PaginationCursorState::get_method_key(method, params)?;
            PaginationCursorState::update_params_cursor(
                json_body,
                cursor_idx,
                pagination_state.get(&method_key).as_ref(),
                method,
            )?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// Updates the pagination cursor cache based on the response of a successful request.
pub fn update_pagination_cursor_state(
    resp: &bytes::Bytes,
    method: &str,
    params: &[Value],
    pagination_state: &PaginationCursorState,
) -> Result<(), anyhow::Error> {
    if PaginationCursorState::get_method_cursor_index(method).is_some() {
        #[derive(Deserialize)]
        struct Body {
            result: Result,
        }
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Result {
            has_next_page: bool,
            next_cursor: Option<Value>,
        }

        let parse_result = serde_json::from_slice::<Body>(&resp);

        if let Ok(Body { result }) = parse_result {
            let method_key = match PaginationCursorState::get_method_key(&method, &params) {
                Ok(key) => key,
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to get method key for method {}: {}",
                        method,
                        e
                    ))
                }
            };
            if result.has_next_page {
                pagination_state.update(method_key, result.next_cursor.clone());
            } else {
                pagination_state.update(method_key, None);
            }
            debug!(
                "Updated pagination state for method: {method} with params: {params:?} to {next_cursor:?}",
                next_cursor = result.next_cursor
            );
        } else {
            error!("Failed to parse response: {:?}", parse_result.err());
        }
    }
    Ok(())
}
