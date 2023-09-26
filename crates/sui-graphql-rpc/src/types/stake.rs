// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

pub(crate) struct Stake;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Stake {
    async fn id(&self) -> ID {
        unimplemented!()
    }
}
