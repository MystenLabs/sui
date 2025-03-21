// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_graphql::{Object, Result};

use crate::error::{bad_user_input, RpcError};

use super::service_config::ServiceConfig;

pub struct Query;

#[derive(thiserror::Error, Debug)]
#[error("Boom!")]
struct Error;

#[Object]
impl Query {
    /// Configuration for this RPC service.
    async fn service_config(&self) -> ServiceConfig {
        ServiceConfig
    }

    /// Test query that always succeeds
    async fn hello(&self) -> String {
        "Hello, GraphQL!".to_string()
    }

    /// Test query that always fails
    async fn boom(&self) -> Result<String, RpcError<Error>> {
        Err(bad_user_input(Error))
    }

    /// Test query that fails with an internal error
    async fn uh_oh(&self) -> Result<String, RpcError<Error>> {
        Err(anyhow!("Underlying reason")
            .context("Reason for making that call")
            .context("Main problem")
            .into())
    }

    /// Test query that times out
    async fn tick_tock(&self) -> String {
        tokio::time::sleep(std::time::Duration::from_secs(100)).await;
        "Done!".to_string()
    }

    /// Test query that can be used to test simulate nesting.
    async fn dot(&self) -> Query {
        Query
    }
}
