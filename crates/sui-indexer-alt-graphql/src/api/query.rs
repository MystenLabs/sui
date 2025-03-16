// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Error, Object, Result};

pub struct Query;

#[Object]
impl Query {
    /// Test query that always succeeds
    async fn hello(&self) -> String {
        "Hello, GraphQL!".to_string()
    }

    /// Test query that always fails
    async fn boom(&self) -> Result<String> {
        Err(Error::new("Boom!"))
    }
}
