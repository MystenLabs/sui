// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{connection::Connection, Context, Object};
use sui_types::sui_system_state::sui_system_state_inner_v1::ValidatorSetV1;

use crate::{
    api::{
        scalars::{big_int::BigInt, cursor::JsonCursor, sui_address::SuiAddress},
        types::validator::Validator,
    },
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

pub(crate) type CValidator = JsonCursor<usize>;

/// Representation of `0x3::validator_set::ValidatorSet`.
#[derive(Clone, Debug)]
pub(crate) struct ValidatorSet {
    scope: Scope,
    native: ValidatorSetV1,
}

/// Representation of `0x3::validator_set::ValidatorSet`.
#[Object]
impl ValidatorSet {
    /// The current list of active validators.
    async fn active_validators(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CValidator>,
        last: Option<u64>,
        before: Option<CValidator>,
    ) -> Result<Option<Connection<String, Validator>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("ValidatorSet", "activeValidators");
        let page = Page::from_params(limits, first, after, last, before)?;

        page.paginate_indices(self.native.active_validators.len(), |i| {
            let validator = &self.native.active_validators[i];

            let at_risk = self
                .native
                .at_risk_validators
                .get(&validator.metadata.sui_address)
                .map_or(0, |at_risk| *at_risk);

            Ok(Validator::from_validator_v1(
                self.scope.clone(),
                validator.clone(),
                at_risk,
            ))
        })
        .map(Some)
    }

    /// Object ID of the `Table` storing the inactive staking pools.
    async fn inactive_pools_id(&self) -> Option<SuiAddress> {
        Some(self.native.inactive_validators.id.into())
    }

    /// Size of the inactive pools `Table`.
    async fn inactive_pools_size(&self) -> Option<u64> {
        Some(self.native.inactive_validators.size)
    }

    // TODO: instead of returning the id and size of the table, potentially return the table itself, paginated.
    /// Object ID of the wrapped object `TableVec` storing the pending active validators.
    async fn pending_active_validators_id(&self) -> Option<SuiAddress> {
        Some(self.native.pending_active_validators.contents.id.into())
    }

    /// Size of the pending active validators table.
    async fn pending_active_validators_size(&self) -> Option<u64> {
        Some(self.native.pending_active_validators.contents.size)
    }

    /// Validators that are pending removal from the active validator set, expressed as indices in to `activeValidators`.
    async fn pending_removals(&self) -> Option<Vec<u64>> {
        Some(self.native.pending_removals.clone())
    }

    /// Object ID of the `Table` storing the mapping from staking pool ids to the addresses of the corresponding validators.
    /// This is needed because a validator's address can potentially change but the object ID of its pool will not.
    async fn staking_pool_mappings_id(&self) -> Option<SuiAddress> {
        Some(self.native.staking_pool_mappings.id.into())
    }

    /// Size of the stake pool mappings `Table`.
    async fn staking_pool_mappings_size(&self) -> Option<u64> {
        Some(self.native.staking_pool_mappings.size)
    }

    /// Total amount of stake for all active validators at the beginning of the epoch.
    async fn total_stake(&self) -> Option<BigInt> {
        Some(self.native.total_stake.into())
    }

    /// Object ID of the `Table` storing the validator candidates.
    async fn validator_candidates_id(&self) -> Option<SuiAddress> {
        Some(self.native.validator_candidates.id.into())
    }

    /// Size of the validator candidates `Table`.
    async fn validator_candidates_size(&self) -> Option<u64> {
        Some(self.native.validator_candidates.size)
    }
}

impl ValidatorSet {
    pub(crate) fn from_validator_set_v1(scope: Scope, native: ValidatorSetV1) -> Self {
        Self { scope, native }
    }
}
