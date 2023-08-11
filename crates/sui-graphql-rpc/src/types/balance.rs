// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

pub(crate) struct Balance;
pub(crate) struct BalanceConnection;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Balance {
    async fn id(&self) -> ID {
        unimplemented!()
    }
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl BalanceConnection {
    async fn unimplemented(&self) -> bool {
        unimplemented!()
    }
}
