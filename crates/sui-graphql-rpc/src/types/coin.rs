// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

pub(crate) struct Coin;
pub(crate) struct CoinConnection;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Coin {
    async fn id(&self) -> ID {
        unimplemented!()
    }
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl CoinConnection {
    async fn unimplemented(&self) -> bool {
        unimplemented!()
    }
}
