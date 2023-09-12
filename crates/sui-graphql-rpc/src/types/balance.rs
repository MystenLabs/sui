// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use super::big_int::BigInt;
use crate::types::owner::Owner;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct Balance {
    // pub(crate) coin_type: MoveType,
    pub(crate) coin_object_count: u64,
    pub(crate) total_balance: BigInt,
}

impl From<sui_json_rpc_types::Balance> for Balance {
    fn from(balance: sui_json_rpc_types::Balance) -> Self {
        Self {
            coin_object_count: balance.coin_object_count as u64,
            total_balance: BigInt::from_str(&format!("{}", balance.total_balance)).unwrap(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct BalanceChange {
    pub(crate) owner: Owner,
    pub(crate) amount: BigInt,
    // pub(crate) coin_type: MoveType,
}
