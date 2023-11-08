// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Response, ServerError, Value};
use hyper::HeaderMap;
use reqwest::Response as ReqwestResponse;
use std::{collections::BTreeMap, net::SocketAddr};

use crate::server::version::VERSION_HEADER;

#[derive(Debug)]
pub struct GraphqlResponse {
    headers: HeaderMap,
    remote_address: Option<SocketAddr>,
    http_version: hyper::Version,
    status: hyper::StatusCode,
    full_response: Response,
}

impl GraphqlResponse {
    pub async fn from_resp(resp: ReqwestResponse) -> Self {
        let headers = resp.headers().clone();
        let remote_address = resp.remote_addr();
        let http_version = resp.version();
        let status = resp.status();
        let full_response: Response = resp.json().await.expect("Failed to parse response");

        Self {
            headers,
            remote_address,
            http_version,
            status,
            full_response,
        }
    }

    pub fn graphql_version(&self) -> String {
        self.headers
            .get(&VERSION_HEADER)
            .expect("Missing version header")
            .to_str()
            .expect("Failed to parse version header to string")
            .to_string()
    }

    pub fn response_body(&self) -> &Response {
        &self.full_response
    }

    pub fn http_status(&self) -> hyper::StatusCode {
        self.status
    }

    pub fn http_version(&self) -> hyper::Version {
        self.http_version
    }

    pub fn http_headers(&self) -> HeaderMap {
        self.headers.clone()
    }

    pub fn remote_address(&self) -> Option<SocketAddr> {
        self.remote_address
    }

    pub fn errors(&self) -> Vec<ServerError> {
        self.full_response.errors.clone()
    }

    pub fn usage(&self) -> Option<BTreeMap<String, u64>> {
        match self.full_response.extensions.get("usage").cloned() {
            Some(Value::Object(obj)) => Some(
                obj.into_iter()
                    .map(|(k, v)| {
                        (
                            k.to_string(),
                            match v {
                                Value::Number(n) => {
                                    n.as_u64().expect("Usage value should be a number")
                                }
                                _ => panic!("Usage value should be a number"),
                            },
                        )
                    })
                    .collect(),
            ),
            _ => None,
        }
    }
}
