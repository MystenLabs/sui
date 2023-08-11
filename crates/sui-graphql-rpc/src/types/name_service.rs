// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

pub(crate) struct NameServiceConnection;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl NameServiceConnection {
    async fn unimplemented(&self) -> bool {
        unimplemented!()
    }
}
