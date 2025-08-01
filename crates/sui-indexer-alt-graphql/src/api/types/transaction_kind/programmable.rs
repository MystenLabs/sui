// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, CursorType, Edge},
    *,
};
use sui_types::transaction::ProgrammableTransaction as NativeProgrammableTransaction;

use crate::{
    api::scalars::{base64::Base64, cursor::JsonCursor},
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

type CInput = JsonCursor<usize>;
type CTransaction = JsonCursor<usize>;

/// A user transaction that allows the interleaving of native commands (like transfer, split coins, merge coins, etc) and move calls, executed atomically.
#[derive(Clone)]
pub struct ProgrammableTransactionBlock {
    pub native: NativeProgrammableTransaction,
    pub scope: Scope,
}

#[Object]
impl ProgrammableTransactionBlock {
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
        let limits = pagination.limits("ProgrammableTransactionBlock", "inputs");
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
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
    ) -> Result<Connection<String, ProgrammableTransaction>, RpcError> {
        let pagination = ctx.data::<PaginationConfig>()?;
        let limits = pagination.limits("ProgrammableTransactionBlock", "transactions");
        let page = Page::from_params(limits, first, after, last, before)?;

        let cursors = page.paginate_indices(self.native.commands.len());
        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);

        for edge in cursors.edges {
            let transaction = ProgrammableTransaction::from(
                self.native.commands[*edge.cursor].clone(),
                self.scope.clone(),
            );

            conn.edges
                .push(Edge::new(edge.cursor.encode_cursor(), transaction));
        }

        Ok(conn)
    }
}

/// Input argument to a Programmable Transaction Block (PTB) command.
#[derive(Union, Clone)]
pub enum TransactionInput {
    Pure(Pure),
}

/// A single transaction, or command, in the programmable transaction block.
#[derive(Union, Clone)]
pub enum ProgrammableTransaction {
    MoveCall(MoveCallTransaction),
}

/// BCS encoded primitive value (not an object or Move struct).
#[derive(SimpleObject, Clone)]
pub struct Pure {
    /// BCS serialized and Base64 encoded primitive value.
    bytes: Option<Base64>,
}

// TODO(DVX-1373): Implement MoveCallTransaction
/// A call to either an entry or a public Move function.
#[derive(SimpleObject, Clone)]
pub struct MoveCallTransaction {
    /// Placeholder field
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

impl TransactionInput {
    pub fn from(input: sui_types::transaction::CallArg, _scope: Scope) -> Self {
        use sui_types::transaction::CallArg;

        match input {
            CallArg::Pure(bytes) => Self::Pure(Pure {
                bytes: Some(Base64::from(bytes)),
            }),
            // TODO: Handle other input types
            _ => Self::Pure(Pure {
                bytes: Some(Base64::from(b"TODO: Unsupported input type".to_vec())),
            }),
        }
    }
}

impl ProgrammableTransaction {
    pub fn from(command: sui_types::transaction::Command, _scope: Scope) -> Self {
        use sui_types::transaction::Command;

        match command {
            Command::MoveCall(_) => Self::MoveCall(MoveCallTransaction { dummy: None }),
            // TODO: Handle other command types, for now just use MoveCall as placeholder
            _ => Self::MoveCall(MoveCallTransaction { dummy: None }),
        }
    }
}
