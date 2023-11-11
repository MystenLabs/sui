// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use super::validator::Validator;
use async_graphql::*;

/// Representation of `0x3::validator_set::ValidatorSet`.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject, Default)]
pub(crate) struct ValidatorSet {
    /// Total amount of stake for all active validators at the beginning of the epoch.
    pub total_stake: Option<BigInt>,

    /// The current list of active validators.
    pub active_validators: Option<Vec<Validator>>,

    /// Validators that are pending removal from the active validator set, expressed as indices in
    /// to `activeValidators`.
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
