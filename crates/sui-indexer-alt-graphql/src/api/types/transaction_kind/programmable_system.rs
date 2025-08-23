// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, CursorType, Edge},
    Context, Object,
};
use sui_types::transaction::ProgrammableTransaction as NativeProgrammableTransaction;

use crate::{
    api::scalars::cursor::JsonCursor,
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

use super::programmable::{Command, TransactionInput};

type CInput = JsonCursor<usize>;
type CCommand = JsonCursor<usize>;

/// ProgrammableSystemTransaction is identical to ProgrammableTransaction, but GraphQL does not allow multiple variants with the same type.
#[derive(Clone)]
pub struct ProgrammableSystemTransaction {
    pub native: NativeProgrammableTransaction,
    pub scope: Scope,
}

#[Object]
impl ProgrammableSystemTransaction {
    /// Input objects or primitive values.
    async fn inputs(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CInput>,
        last: Option<u64>,
        before: Option<CInput>,
    ) -> Result<Connection<String, TransactionInput>, RpcError> {
        let pagination = ctx.data::<PaginationConfig>()?;
        let limits = pagination.limits("ProgrammableSystemTransaction", "inputs");
        let page = Page::from_params(limits, first, after, last, before)?;

        let cursors = page.paginate_indices(self.native.inputs.len());
        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);

        for edge in cursors.edges {
            let input = TransactionInput::from(
                self.native.inputs[*edge.cursor].clone(),
                self.scope.clone(),
            );
            conn.edges
                .push(Edge::new(edge.cursor.encode_cursor(), input));
        }

        Ok(conn)
    }

    /// The transaction commands, executed sequentially.
    async fn commands(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CCommand>,
        last: Option<u64>,
        before: Option<CCommand>,
    ) -> Result<Connection<String, Command>, RpcError> {
        let pagination = ctx.data::<PaginationConfig>()?;
        let limits = pagination.limits("ProgrammableSystemTransaction", "commands");
        let page = Page::from_params(limits, first, after, last, before)?;

        let cursors = page.paginate_indices(self.native.commands.len());
        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);

        for edge in cursors.edges {
            let command = Command::from(
                self.scope.clone(),
                self.native.commands[*edge.cursor].clone(),
            );

            conn.edges
                .push(Edge::new(edge.cursor.encode_cursor(), command));
        }

        Ok(conn)
    }
}
