// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_graphql::Client;

pub struct GraphQLQueryClient {
    client: Client,
}

impl GraphQLQueryClient {
    pub fn new(endpoint: &str) -> Self {
        let client = Client::new(endpoint);
        Self { client }
    }
}
fn fetch_checkpoint() {}
