// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use crate::types::owner::Owner;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct Balance {
    pub(crate) coin_type: Option<String>, // TODO: replace with MoveType
    pub(crate) coin_object_count: Option<u64>,
    pub(crate) total_balance: Option<BigInt>,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct BalanceChange {
    pub(crate) owner: Option<Owner>,
    pub(crate) amount: Option<BigInt>,
    // pub(crate) coin_type: MoveType,
}
