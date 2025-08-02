// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{connection::Connection, *};
use sui_types::transaction::EndOfEpochTransactionKind as NativeEndOfEpochTransactionKind;

use crate::{
    api::{
        scalars::cursor::JsonCursor, types::transaction_kind::change_epoch::ChangeEpochTransaction,
    },
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

type CTransaction = JsonCursor<usize>;

#[derive(Clone)]
pub struct EndOfEpochTransaction {
    pub native: Vec<NativeEndOfEpochTransactionKind>,
    pub scope: Scope,
}

#[derive(Union, Clone)]
pub enum EndOfEpochTransactionKind {
    ChangeEpoch(ChangeEpochTransaction),
    // TODO: Add more transaction types incrementally
}

/// System transaction that supersedes `ChangeEpochTransaction` as the new way to run transactions at the end of an epoch. Behaves similarly to `ChangeEpochTransaction` but can accommodate other optional transactions to run at the end of the epoch.
#[Object]
impl EndOfEpochTransaction {
    /// The list of system transactions that did run at the end of the epoch.
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
    ) -> Result<Connection<String, EndOfEpochTransactionKind>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("EndOfEpochTransaction", "transactions");
        let page = Page::from_params(limits, first, after, last, before)?;

        let cursors = page.paginate_indices(self.native.len());
        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);

        for edge in cursors.edges {
            if let Some(tx_kind) = EndOfEpochTransactionKind::from(
                self.native[*edge.cursor].clone(),
                self.scope.clone(),
            ) {
                conn.edges.push(async_graphql::connection::Edge::new(
                    edge.cursor.to_string(),
                    tx_kind,
                ));
            }
        }

        Ok(conn)
    }
}

impl EndOfEpochTransactionKind {
    pub fn from(kind: NativeEndOfEpochTransactionKind, scope: Scope) -> Option<Self> {
        use EndOfEpochTransactionKind as K;
        use NativeEndOfEpochTransactionKind as N;

        match kind {
            N::ChangeEpoch(ce) => {
                Some(K::ChangeEpoch(ChangeEpochTransaction { native: ce, scope }))
            }
            // TODO: Handle other transaction types incrementally
            _ => None,
        }
    }
}
