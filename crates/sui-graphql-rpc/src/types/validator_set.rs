// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::cursor::{JsonCursor, Page};
use async_graphql::connection::{Connection, CursorType, Edge};

use super::big_int::BigInt;
use super::validator::Validator;
use async_graphql::*;

/// Representation of `0x3::validator_set::ValidatorSet`.
#[derive(Clone, Debug, SimpleObject, Default)]
#[graphql(complex)]
pub(crate) struct ValidatorSet {
    /// Total amount of stake for all active validators at the beginning of the epoch.
    pub total_stake: Option<BigInt>,

    #[graphql(skip)]
    /// The current list of active validators.
    pub active_validators: Option<Vec<Validator>>,

    /// Validators that are pending removal from the active validator set, expressed as indices in
    /// to `activeValidators`.
    pub pending_removals: Option<Vec<u64>>,

    // TODO: finish implementing the commented out fields below.

    // pending_active_validators: Option<MoveObject>,
    pub pending_active_validators_size: Option<u64>,
    // stake_pool_mappings: Option<MoveObject>,
    pub stake_pool_mappings_size: Option<u64>,
    // inactive_pools: Option<MoveObject>,
    pub inactive_pools_size: Option<u64>,
    // validator_candidates: Option<MoveObject>,
    pub validator_candidates_size: Option<u64>,
}

type CValidator = JsonCursor<usize>;

#[ComplexObject]
impl ValidatorSet {
    /// The current set of active validators.
    async fn active_validators(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        before: Option<CValidator>,
        last: Option<u64>,
        after: Option<CValidator>,
    ) -> Result<Connection<String, Validator>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let mut connection = Connection::new(false, false);
        let Some(validators) = &self.active_validators else {
            return Ok(connection);
        };

        let Some((prev, next, cs)) = page.paginate_indices(validators.len()) else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            connection
                .edges
                .push(Edge::new(c.encode_cursor(), validators[*c].clone()));
        }

        Ok(connection)
    }
}
