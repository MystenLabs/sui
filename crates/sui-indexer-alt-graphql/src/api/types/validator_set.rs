// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::{ComplexObject, Context, SimpleObject};
use sui_types::sui_system_state::sui_system_state_inner_v1::{ValidatorSetV1, ValidatorV1};

use crate::{
    api::scalars::cursor::JsonCursor,
    api::{
        scalars::{big_int::BigInt, sui_address::SuiAddress},
        types::validator::Validator,
    },
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

pub(crate) type CValidator = JsonCursor<u64>;

/// Representation of `0x3::validator_set::ValidatorSet`.
#[derive(Clone, Debug, SimpleObject)]
#[graphql(complex)]
pub(crate) struct ValidatorSet {
    #[graphql(skip)]
    scope: Scope,

    #[graphql(skip)]
    active_validators: Vec<ValidatorV1>,

    /// Total amount of stake for all active validators at the beginning of the epoch.
    pub total_stake: Option<BigInt>,

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

#[ComplexObject]
impl ValidatorSet {
    /// The current list of active validators.
    async fn active_validators(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CValidator>,
        last: Option<u64>,
        before: Option<CValidator>,
    ) -> Result<Connection<String, Validator>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("ValidatorSet", "activeValidators");
        let page = Page::from_params(limits, first, after, last, before)?;

        let mut conn = Connection::new(false, false);

        let active_validators = self.active_validators.iter().enumerate().collect();

        let (prev, next, edges) =
            page.paginate_results(active_validators, |(i, _)| CValidator::new(*i as u64));

        conn.has_previous_page = prev;
        conn.has_next_page = next;
        edges.for_each(|(cursor, (_, validator))| {
            conn.edges.push(Edge::new(
                cursor.encode_cursor(),
                Validator::from_validator_v1(self.scope.clone(), validator.clone()),
            ));
        });

        Ok(conn)
    }
}

impl ValidatorSet {
    pub(crate) fn from_validator_set_v1(scope: Scope, value: ValidatorSetV1) -> Self {
        Self {
            scope,
            active_validators: value.active_validators,
            total_stake: Some(BigInt::from(value.total_stake)),
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
