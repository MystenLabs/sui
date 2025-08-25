// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, CursorType, Edge},
    Context, SimpleObject,
};
use sui_indexer_alt_reader::consistent_reader::{self, ConsistentReader};
use sui_types::{base_types::SuiAddress, TypeTag};

use crate::{
    api::scalars::{big_int::BigInt, cursor},
    error::{bad_user_input, feature_unavailable, RpcError},
    pagination::Page,
    scope::Scope,
};

use super::move_type::MoveType;

/// The total balance for a particular coin type.
#[derive(SimpleObject)]
pub(crate) struct Balance {
    /// Coin type for the balance, such as `0x2::sui::SUI`.
    pub(crate) coin_type: Option<MoveType>,

    /// The total balance across all coin objects of this coin type.
    pub(crate) total_balance: Option<BigInt>,
}

#[derive(thiserror::Error, Debug, Clone)]
pub(crate) enum Error {
    #[error("Cursors are pinned to different checkpoints: {0} vs {1}")]
    CursorInconsistency(u64, u64),

    #[error("Request is outside consistent range")]
    OutOfRange(u64),

    #[error("Checkpoint {0} in the future")]
    Future(u64),

    #[error("Cannot query balances for a parent object's address if its version is bounded. Fetch the parent at a checkpoint in the consistent range to query its balances.")]
    RootVersionOwnership,
}

pub(crate) type Cursor = cursor::BcsCursor<(u64, Vec<u8>)>;

impl Balance {
    /// Fetch the balance for a single coin type owned by the given address, live at the current
    /// checkpoint.
    pub(crate) async fn fetch_one(
        ctx: &Context<'_>,
        scope: &Scope,
        address: SuiAddress,
        coin_type: TypeTag,
    ) -> Result<Balance, RpcError<Error>> {
        if scope.root_version().is_some() {
            return Err(bad_user_input(Error::RootVersionOwnership));
        }

        let consistent_reader: &ConsistentReader = ctx.data()?;
        let checkpoint = scope.checkpoint_viewed_at();

        let (coin_type, total_balance) = consistent_reader
            .get_balance(
                checkpoint,
                address.to_string(),
                coin_type.to_canonical_string(true),
            )
            .await
            .map_err(|e| consistent_error(checkpoint, e))?;

        Ok(Balance {
            coin_type: Some(MoveType::from_native(coin_type, scope.clone())),
            total_balance: Some(BigInt::from(total_balance)),
        })
    }

    /// Fetch balances for multiple coin types owned by the given address, live at the current
    /// checkpoint.
    pub(crate) async fn fetch_many(
        ctx: &Context<'_>,
        scope: &Scope,
        address: SuiAddress,
        coin_types: Vec<TypeTag>,
    ) -> Result<Vec<Balance>, RpcError<Error>> {
        if scope.root_version().is_some() {
            return Err(bad_user_input(Error::RootVersionOwnership));
        }

        let consistent_reader: &ConsistentReader = ctx.data()?;
        let checkpoint = scope.checkpoint_viewed_at();

        let balances = consistent_reader
            .batch_get_balances(
                checkpoint,
                address.to_string(),
                coin_types
                    .into_iter()
                    .map(|t| t.to_canonical_string(true))
                    .collect(),
            )
            .await
            .map_err(|e| consistent_error(checkpoint, e))?;

        Ok(balances
            .into_iter()
            .map(|(coin_type, total_balance)| Balance {
                coin_type: Some(MoveType::from_native(coin_type, scope.clone())),
                total_balance: Some(BigInt::from(total_balance)),
            })
            .collect())
    }

    /// Paginate through balances for coins owned by the given address, live at the current
    /// checkpoint.
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: Scope,
        address: SuiAddress,
        page: Page<Cursor>,
    ) -> Result<Connection<String, Balance>, RpcError<Error>> {
        if scope.root_version().is_some() {
            return Err(bad_user_input(Error::RootVersionOwnership));
        }

        let consistent_reader: &ConsistentReader = ctx.data()?;

        // Figure out which checkpoint to pin results to, based on the pagination cursors and
        // defaulting to the current scope. If both cursors are provided, they must agree on the
        // checkpoint they are pinning, and this checkpoint must be at or below the scope's latest
        // checkpoint.
        let checkpoint = match (page.after(), page.before()) {
            (Some(a), Some(b)) if a.0 != b.0 => {
                return Err(bad_user_input(Error::CursorInconsistency(a.0, b.0)));
            }

            (None, None) => scope.checkpoint_viewed_at(),
            (Some(c), _) | (_, Some(c)) => c.0,
        };

        let Some(scope) = scope.with_checkpoint_viewed_at(checkpoint) else {
            return Err(bad_user_input(Error::Future(checkpoint)));
        };

        let balances = consistent_reader
            .list_balances(
                checkpoint,
                address.to_string(),
                Some(page.limit() as u32),
                page.after().map(|c| c.1.clone()),
                page.before().map(|c| c.1.clone()),
                page.is_from_front(),
            )
            .await
            .map_err(|e| consistent_error(checkpoint, e))?;

        let mut conn = Connection::new(false, false);
        if balances.results.is_empty() {
            return Ok(conn);
        }

        conn.has_previous_page = balances.has_previous_page;
        conn.has_next_page = balances.has_next_page;

        for edge in balances.results {
            let (coin_type, total_balance) = edge.value;

            let cursor = Cursor::new((checkpoint, edge.token));
            let balance = Balance {
                coin_type: Some(MoveType::from_native(coin_type, scope.clone())),
                total_balance: Some(BigInt::from(total_balance)),
            };

            conn.edges.push(Edge::new(cursor.encode_cursor(), balance));
        }

        Ok(conn)
    }
}

/// Convert an error from the consistent reader into an RpcError, assuming the request was made at
/// the given `checkpoint`.
fn consistent_error(checkpoint: u64, error: consistent_reader::Error) -> RpcError<Error> {
    match error {
        consistent_reader::Error::NotConfigured => {
            feature_unavailable("fetching balances for addresses")
        }

        consistent_reader::Error::OutOfRange(_) => bad_user_input(Error::OutOfRange(checkpoint)),

        consistent_reader::Error::Internal(error) => {
            error.context("Failed to fetch balance").into()
        }
    }
}
