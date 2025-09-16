// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;
use sui_types::sui_system_state::sui_system_state_inner_v1::ValidatorSetV1;

use crate::api::{
    scalars::{big_int::BigInt, sui_address::SuiAddress},
    types::validator::Validator,
};

/// Representation of `0x3::validator_set::ValidatorSet`.
#[derive(Clone, Debug, SimpleObject, Default)]
pub(crate) struct ValidatorSet {
    /// Total amount of stake for all active validators at the beginning of the epoch.
    pub total_stake: Option<BigInt>,

    /// The current list of active validators.
    pub active_validators: Option<Vec<Validator>>,

    /// Validators that are pending removal from the active validator set, expressed as indices in
    /// to `activeValidators`.
    pub pending_removals: Option<Vec<u64>>,

    // TODO: instead of returning the id and size of the table, potentially return the table itself, paginated.
    /// Object ID of the wrapped object `TableVec` storing the pending active validators.
    pub pending_active_validators_id: Option<SuiAddress>,

    /// Size of the pending active validators table.
    pub pending_active_validators_size: Option<u64>,

    /// Object ID of the `Table` storing the mapping from staking pool ids to the addresses
    /// of the corresponding validators. This is needed because a validator's address
    /// can potentially change but the object ID of its pool will not.
    pub staking_pool_mappings_id: Option<SuiAddress>,

    /// Size of the stake pool mappings `Table`.
    pub staking_pool_mappings_size: Option<u64>,

    /// Object ID of the `Table` storing the inactive staking pools.
    pub inactive_pools_id: Option<SuiAddress>,

    /// Size of the inactive pools `Table`.
    pub inactive_pools_size: Option<u64>,

    /// Object ID of the `Table` storing the validator candidates.
    pub validator_candidates_id: Option<SuiAddress>,

    /// Size of the validator candidates `Table`.
    pub validator_candidates_size: Option<u64>,
}

impl From<ValidatorSetV1> for ValidatorSet {
    fn from(value: ValidatorSetV1) -> Self {
        ValidatorSet {
            total_stake: Some(BigInt::from(value.total_stake)),
            active_validators: Some(
                value
                    .active_validators
                    .iter()
                    .map(|v| v.clone().into())
                    // todo (ewall)
                    // remove this and add pagination
                    .take(1)
                    .collect(),
            ),
            pending_removals: Some(value.pending_removals),
            pending_active_validators_id: Some(value.pending_active_validators.contents.id.into()),
            pending_active_validators_size: Some(value.pending_active_validators.contents.size),
            staking_pool_mappings_id: Some(value.staking_pool_mappings.id.into()),
            staking_pool_mappings_size: Some(value.staking_pool_mappings.size),
            inactive_pools_id: Some(value.inactive_validators.id.into()),
            inactive_pools_size: Some(value.inactive_validators.size),
            validator_candidates_id: Some(value.validator_candidates.id.into()),
            validator_candidates_size: Some(value.validator_candidates.size),
        }
    }
}
