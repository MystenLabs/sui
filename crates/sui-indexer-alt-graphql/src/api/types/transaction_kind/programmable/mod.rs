// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Context;
use async_graphql::Object;
use async_graphql::connection::Connection;
use sui_types::transaction::ProgrammableTransaction as NativeProgrammableTransaction;

use crate::api::scalars::cursor::JsonCursor;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::pagination::PaginationConfig;
use crate::scope::Scope;

pub use commands::Command;
pub use inputs::TransactionInput;

pub mod commands;
pub mod inputs;

type CInput = JsonCursor<usize>;
type CCommand = JsonCursor<usize>;

/// A user transaction that allows the interleaving of native commands (like transfer, split coins, merge coins, etc) and move calls, executed atomically.
#[derive(Clone)]
pub struct ProgrammableTransaction {
    pub native: NativeProgrammableTransaction,
    pub scope: Scope,
}

#[Object]
impl ProgrammableTransaction {
    /// Input objects or primitive values.
    async fn inputs(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CInput>,
        last: Option<u64>,
        before: Option<CInput>,
    ) -> Option<Result<Connection<String, TransactionInput>, RpcError>> {
        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("ProgrammableTransaction", "inputs");
                let page = Page::from_params(limits, first, after, last, before)?;

                let resolver = self.scope.package_resolver();
                let pure_layouts = match resolver.pure_input_layouts(&self.native).await {
                    Ok(layouts) => layouts,
                    Err(_) => vec![None; self.native.inputs.len()],
                };

                page.paginate_indices(self.native.inputs.len(), |i| {
                    Ok(TransactionInput::from(
                        self.native.inputs[i].clone(),
                        pure_layouts[i].clone(),
                        self.scope.clone(),
                    ))
                })
            }
            .await,
        )
    }

    /// The transaction commands, executed sequentially.
    async fn commands(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CCommand>,
        last: Option<u64>,
        before: Option<CCommand>,
    ) -> Option<Result<Connection<String, Command>, RpcError>> {
        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("ProgrammableTransaction", "commands");
                let page = Page::from_params(limits, first, after, last, before)?;

                page.paginate_indices(self.native.commands.len(), |i| {
                    Ok(Command::from(
                        self.scope.clone(),
                        self.native.commands[i].clone(),
                    ))
                })
            }
            .await,
        )
    }
}
