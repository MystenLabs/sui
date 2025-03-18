// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_graphql::{Object, Result};

use crate::error::{bad_user_input, RpcError};

pub struct Query;

#[derive(thiserror::Error, Debug)]
#[error("Boom!")]
struct Error;

#[Object]
impl Query {
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
}
