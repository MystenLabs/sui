// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;

pub struct Query;

#[Object]
impl Query {
    /// Test query
    async fn hello(&self) -> String {
        "Hello, GraphQL!".to_string()
    }
}
