// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::ClientError;
use async_graphql::{Response, ServerError, Value};
use reqwest::header::{HeaderMap, HeaderName};
use reqwest::Response as ReqwestResponse;
use serde_json::json;
use std::{collections::BTreeMap, net::SocketAddr};
use sui_graphql_rpc_headers::VERSION_HEADER;

#[derive(Debug)]
pub struct GraphqlResponse {
    headers: HeaderMap,
    remote_address: Option<SocketAddr>,
    http_version: reqwest::Version,
    status: reqwest::StatusCode,
    full_response: Response,
}

impl GraphqlResponse {
    pub async fn from_resp(resp: ReqwestResponse) -> Result<Self, ClientError> {
        let headers = resp.headers().clone();
        let remote_address = resp.remote_addr();
        let http_version = resp.version();
        let status = resp.status();
        let full_response: Response = resp.json().await.map_err(ClientError::InnerClientError)?;

        Ok(Self {
            headers,
            remote_address,
            http_version,
            status,
            full_response,
        })
    }

    pub fn graphql_version(&self) -> Result<String, ClientError> {
        Ok(self
            .headers
            .get(VERSION_HEADER.as_str())
            .ok_or(ClientError::ServiceVersionHeaderNotFound)?
            .to_str()
            .map_err(|e| ClientError::ServiceVersionHeaderValueInvalidString { error: e })?
            .to_string())
    }

    pub fn response_body(&self) -> &Response {
        &self.full_response
    }

    pub fn response_body_json(&self) -> serde_json::Value {
        json!(self.full_response)
    }

    pub fn response_body_json_pretty(&self) -> String {
        serde_json::to_string_pretty(&self.full_response).unwrap()
    }

    pub fn http_status(&self) -> reqwest::StatusCode {
        self.status
    }

    pub fn http_version(&self) -> reqwest::Version {
        self.http_version
    }

    pub fn http_headers(&self) -> HeaderMap {
        self.headers.clone()
    }

    /// Returns the HTTP headers without the `Date` header.
    /// The `Date` header is removed because it is not deterministic.
    pub fn http_headers_without_date(&self) -> HeaderMap {
        let mut headers = self.http_headers().clone();
        headers.remove(HeaderName::from_static("date"));
        headers
    }

    pub fn remote_address(&self) -> Option<SocketAddr> {
        self.remote_address
    }

    pub fn errors(&self) -> Vec<ServerError> {
        self.full_response.errors.clone()
    }

    pub fn usage(&self) -> Result<Option<BTreeMap<String, u64>>, ClientError> {
        Ok(match self.full_response.extensions.get("usage").cloned() {
            Some(Value::Object(obj)) => Some(
                obj.into_iter()
                    .map(|(k, v)| match v {
                        Value::Number(n) => {
                            n.as_u64().ok_or(ClientError::InvalidUsageNumber {
                                usage_name: k.to_string(),
                                usage_number: n,
                            })
                        }
                        .map(|q| (k.to_string(), q)),
                        _ => Err(ClientError::InvalidUsageValue {
                            usage_name: k.to_string(),
                            usage_value: v,
                        }),
                    })
                    .collect::<Result<BTreeMap<String, u64>, ClientError>>()?,
            ),
            _ => None,
        })
    }
}
