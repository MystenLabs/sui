// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct StakeSubsidy {
    pub balance: Option<BigInt>,
    pub distribution_counter: Option<u64>,
    pub current_distribution_amount: Option<BigInt>,
    pub period_length: Option<u64>,
    pub decrease_rate: Option<u64>,
}
