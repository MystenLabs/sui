// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::big_int::BigInt;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct SystemParameters {
    pub duration_ms: Option<BigInt>,
    pub stake_subsidy_start_epoch: Option<u64>,
    pub min_validator_count: Option<u64>,
    pub max_validator_count: Option<u64>,
    pub min_validator_joining_stake: Option<BigInt>,
    pub validator_low_stake_threshold: Option<BigInt>,
    pub validator_very_low_stake_threshold: Option<BigInt>,
    pub validator_low_stake_grace_period: Option<BigInt>,
}
