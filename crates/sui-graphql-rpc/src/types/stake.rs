// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

pub(crate) struct Stake;
pub(crate) struct StakeConnection;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Stake {
    async fn id(&self) -> ID {
        unimplemented!()
    }
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl StakeConnection {
    async fn id(&self) -> ID {
        unimplemented!()
    }
}
