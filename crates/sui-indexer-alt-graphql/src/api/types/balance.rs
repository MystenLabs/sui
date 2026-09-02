// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::InputObject;
use async_graphql::SimpleObject;
use async_graphql::connection::Connection;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use sui_indexer_alt_reader::consistent_reader;
use sui_indexer_alt_reader::consistent_reader::ConsistentReader;
use sui_indexer_alt_reader::consistent_reader::proto::Balance as ProtoBalance;
use sui_types::TypeTag;
use sui_types::base_types::SuiAddress as NativeSuiAddress;

use crate::api::scalars::big_int::BigInt;
use crate::api::scalars::cursor;
use crate::api::scalars::sui_address::SuiAddress;
use crate::api::scalars::type_filter::TypeInput;
use crate::api::types::move_type::MoveType;
use crate::error::RpcError;
use crate::error::bad_user_input;
use crate::error::feature_unavailable;
use crate::extensions::query_limits;
use crate::pagination::Page;
use crate::scope::Scope;

/// The balance of a particular coin type held by an address.
///
/// Balances can be held in coin objects, in the address's balance accumulator, or both.
#[derive(SimpleObject)]
pub(crate) struct Balance {
    /// Coin type for the balance, such as `0x2::sui::SUI`.
    pub(crate) coin_type: Option<MoveType>,

    /// The sum of `coinBalance` and `addressBalance`.
    pub(crate) total_balance: Option<BigInt>,

    /// The balance held in coin objects owned by the address.
    pub(crate) coin_balance: Option<BigInt>,

    /// The balance held in the address's balance accumulator.
    pub(crate) address_balance: Option<BigInt>,
}

/// Identifies the balance for one coin type owned by an address.
#[derive(InputObject)]
pub(crate) struct BalanceKey {
    /// The address that owns the balance.
    pub(crate) address: SuiAddress,

    /// The coin type of the balance.
    pub(crate) coin_type: TypeInput,
}

#[derive(thiserror::Error, Debug, Clone)]
pub(crate) enum Error {
    #[error("Cursors are pinned to different checkpoints: {0} vs {1}")]
    CursorInconsistency(u64, u64),

    #[error("Request is outside consistent range")]
    OutOfRange(u64),

    #[error("Checkpoint {0} in the future")]
    Future(u64),

    #[error(
        "Cannot query balances for a parent object's address if its version is bounded. Fetch the parent at a checkpoint in the consistent range to query its balances."
    )]
    RootVersionOwnership,
}

pub(crate) type Cursor = cursor::BcsCursor<(u64, Vec<u8>)>;

impl Balance {
    fn try_from_proto(proto: ProtoBalance, scope: Scope) -> Result<Self, RpcError<Error>> {
        let coin_type: TypeTag = proto
            .coin_type
            .context("coin type missing")?
            .parse()
            .context("invalid coin type")?;

        Ok(Balance {
            coin_type: Some(MoveType::from_native(coin_type, scope)),
            total_balance: Some(BigInt::from(proto.total_balance.unwrap_or(0))),
            coin_balance: Some(BigInt::from(proto.coin_balance.unwrap_or(0))),
            address_balance: Some(BigInt::from(proto.address_balance.unwrap_or(0))),
        })
    }

    /// Fetch the balance for a single coin type owned by the given address, live at the current
    /// checkpoint.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn fetch_one(
        ctx: &Context<'_>,
        scope: &Scope,
        address: NativeSuiAddress,
        coin_type: TypeTag,
    ) -> Result<Option<Balance>, RpcError<Error>> {
        if scope.root_version().is_some() {
            return Err(bad_user_input(Error::RootVersionOwnership));
        }

        let Some(checkpoint) = scope.root_checkpoint() else {
            return Ok(None);
        };

        query_limits::rich::debit(ctx)?;
        let consistent_reader: &ConsistentReader = ctx.data()?;
        let balance = consistent_reader
            .get_balance(
                Some(checkpoint),
                address.to_string(),
                coin_type.to_canonical_string(true),
            )
            .await
            .map_err(|e| consistent_error(checkpoint, e))?;

        Ok(Some(Balance::try_from_proto(balance, scope.clone())?))
    }

    /// Fetch balances for multiple address and coin type pairs, live at the current checkpoint.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn fetch_many(
        ctx: &Context<'_>,
        scope: &Scope,
        keys: Vec<(NativeSuiAddress, TypeTag)>,
    ) -> Result<Option<Vec<Balance>>, RpcError<Error>> {
        if scope.root_version().is_some() {
            return Err(bad_user_input(Error::RootVersionOwnership));
        }

        let Some(checkpoint) = scope.root_checkpoint() else {
            return Ok(None);
        };

        query_limits::rich::debit(ctx)?;
        let consistent_reader: &ConsistentReader = ctx.data()?;
        let requests = keys
            .into_iter()
            .map(|(address, coin_type)| {
                (
                    address.to_string(),
                    coin_type.to_canonical_string(/* with_prefix */ true),
                )
            })
            .collect();

        let balances = consistent_reader
            .batch_get_balances(checkpoint, requests)
            .await
            .map_err(|e| consistent_error(checkpoint, e))?;

        Ok(Some(
            balances
                .into_iter()
                .map(|balance| Balance::try_from_proto(balance, scope.clone()))
                .collect::<Result<_, _>>()?,
        ))
    }

    /// Paginate through balances held by the given address, live at the current checkpoint.
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: Scope,
        address: NativeSuiAddress,
        page: Page<Cursor>,
    ) -> Result<Connection<String, Balance>, RpcError<Error>> {
        if scope.root_version().is_some() {
            return Err(bad_user_input(Error::RootVersionOwnership));
        }

        let Some(root_checkpoint) = scope.root_checkpoint() else {
            return Ok(Connection::new(false, false));
        };

        query_limits::rich::debit(ctx)?;
        let consistent_reader: &ConsistentReader = ctx.data()?;

        // Figure out which checkpoint to pin results to, based on the pagination cursors and
        // defaulting to the current scope. If both cursors are provided, they must agree on the
        // checkpoint they are pinning, and this checkpoint must be at or below the scope's latest
        // checkpoint.
        let checkpoint = match (page.after(), page.before()) {
            (Some(a), Some(b)) if a.0 != b.0 => {
                return Err(bad_user_input(Error::CursorInconsistency(a.0, b.0)));
            }
            (None, None) => root_checkpoint,
            (Some(c), _) | (_, Some(c)) => c.0,
        };

        let Some(scope) = scope.with_checkpoint_viewed_at(ctx, checkpoint) else {
            return Err(bad_user_input(Error::Future(checkpoint)));
        };

        let balances = consistent_reader
            .list_balances(
                Some(checkpoint),
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
            let balance = edge.value;

            let cursor = Cursor::new((checkpoint, edge.token));
            let balance = Balance::try_from_proto(balance, scope.clone())?;

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
