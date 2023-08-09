// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

pub struct Query;

#[allow(unreachable_code)]
#[Object]
impl Query {
    async fn chain_identifier(&self) -> String {
        unimplemented!()
    }
}
