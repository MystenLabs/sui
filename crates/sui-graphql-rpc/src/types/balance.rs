// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct Balance {
    // pub(crate) coin_type: MoveType,
    pub(crate) coin_object_count: u64,
    pub(crate) total_balance: BigInt,
}
pub(crate) struct BalanceConnection;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl BalanceConnection {
    async fn unimplemented(&self) -> bool {
        unimplemented!()
    }
}
