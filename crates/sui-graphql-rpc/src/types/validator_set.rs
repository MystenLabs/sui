// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use super::validator::Validator;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject, Default)]
pub(crate) struct ValidatorSet {
    pub total_stake: Option<BigInt>,
    pub active_validators: Option<Vec<Validator>>,
    pub pending_removals: Option<Vec<u64>>,
    // pending_active_validators: Option<MoveObject>,
    pub pending_active_validators_size: Option<u64>,
    // stake_pool_mappings: Option<MoveObject>,
    pub stake_pool_mappings_size: Option<u64>,
    // inactive_pools: Option<MoveObject>,
    pub inactive_pools_size: Option<u64>,
    // validator_candidates: Option<MoveObject>,
    pub validator_candidates_size: Option<u64>,
}
