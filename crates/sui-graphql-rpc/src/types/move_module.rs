// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::connection::{Connection, Edge};
use async_graphql::*;
use move_binary_format::access::ModuleAccess;
use move_binary_format::binary_views::BinaryIndexedView;
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Loc;

use crate::context_data::db_data_provider::{validate_cursor_pagination, PgManager};
use crate::error::Error;
use sui_package_resolver::Module as ParsedMoveModule;

use super::{base64::Base64, move_package::MovePackage, sui_address::SuiAddress};

#[derive(Clone)]
pub(crate) struct MoveModule {
    pub native: Vec<u8>,
    pub parsed: ParsedMoveModule,
}

#[derive(SimpleObject)]
#[graphql(complex)]
pub(crate) struct MoveModuleId {
    #[graphql(skip)]
    pub package: SuiAddress,
    pub name: String,
}

/// Represents a module in Move, a library that defines struct types
/// and functions that operate on these types.
#[Object]
impl MoveModule {
    async fn file_format_version(&self) -> u32 {
        self.parsed.bytecode().version
    }

    // TODO: impl all fields

    async fn module_id(&self) -> MoveModuleId {
        // TODO: Rethink the need for MoveModuleId -- we probably don't need it (MoveModule should
        // expose access to its package).
        let self_id = self.parsed.bytecode().self_id();
        MoveModuleId {
            package: SuiAddress::from(*self_id.address()),
            name: self_id.name().to_string(),
        }
    }

    /// Modules that this module considers friends (these modules can access `public(friend)`
    /// functions from this module).
    async fn friend_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Connection<String, MoveModule>> {
        // TODO: make cursor opaque (currently just an offset).
        validate_cursor_pagination(&first, &after, &last, &before).extend()?;

        let bytecode = self.parsed.bytecode();
        let total = bytecode.friend_decls.len();

        // Add one to make [lo, hi) a half-open interval ((after, before) is an open interval).
        let mut lo = if let Some(after) = after {
            1 + after
                .parse::<usize>()
                .map_err(|_| Error::InvalidCursor("Failed to parse 'after' cursor.".to_string()))
                .extend()?
        } else {
            0
        };

        let mut hi = if let Some(before) = before {
            before
                .parse::<usize>()
                .map_err(|_| Error::InvalidCursor("Failed to parse 'before' cursor.".to_string()))
                .extend()?
        } else {
            total
        };

        let mut connection = Connection::new(false, false);
        if hi <= lo {
            return Ok(connection);
        }

        // If there's a `first` limit, bound the upperbound to be at most `first` away from the
        // lowerbound.
        if let Some(first) = first {
            let first = first as usize;
            if hi - lo > first {
                hi = lo + first;
            }
        }

        // If there's a `last` limit, bound the lowerbound to be at most `last` away from the
        // upperbound.  NB. This applies after we bounded the upperbound, using `first`.
        if let Some(last) = last {
            let last = last as usize;
            if hi - lo > last {
                lo = hi - last;
            }
        }

        connection.has_previous_page = 0 < lo;
        connection.has_next_page = hi < total;

        let self_id = bytecode.self_id();
        let Some(package) = ctx
            .data_unchecked::<PgManager>()
            .fetch_move_package(SuiAddress::from(*self_id.address()), None)
            .await
            .extend()?
        else {
            return Err(Error::Internal(format!(
                "Failed to load package for module: {}",
                self_id.to_canonical_display(/* with_prefix */ true),
            ))
            .extend());
        };

        // Select `friend_decls[lo..hi]` using iterators to enumerate before taking a sub-sequence
        // from it, to get pairs `(i, friend_decls[i])`.
        for (idx, decl) in bytecode
            .friend_decls
            .iter()
            .enumerate()
            .skip(lo)
            .take(hi - lo)
        {
            let friend_pkg = bytecode.address_identifier_at(decl.address);
            let friend_mod = bytecode.identifier_at(decl.name);

            if friend_pkg != self_id.address() {
                return Err(Error::Internal(format!(
                    "Friend module of {} from a different package: {}::{}",
                    self_id.to_canonical_display(/* with_prefix */ true),
                    friend_pkg.to_canonical_display(/* with_prefix */ true),
                    friend_mod,
                ))
                .extend());
            }

            let Some(friend) = package.module_impl(friend_mod.as_str())? else {
                return Err(Error::Internal(format!(
                    "Failed to load friend module of {}: {}",
                    self_id.to_canonical_display(/* with_prefix */ true),
                    friend_mod,
                ))
                .extend());
            };

            connection.edges.push(Edge::new(idx.to_string(), friend));
        }

        Ok(connection)
    }

    // struct(name: String!): MoveStructDecl
    // structConnection(
    //   first: Int,
    //   after: String,
    //   last: Int,
    //   before: String,
    // ): MoveStructConnection

    // function(name: String!): MoveFunction
    // functionConnection(
    //   first: Int,
    //   after: String,
    //   last: Int,
    //   before: String,
    // ): MoveFunctionConnection

    /// The Base64 encoded bcs serialization of the module.
    async fn bytes(&self) -> Option<Base64> {
        Some(Base64::from(self.native.clone()))
    }

    /// Textual representation of the module's bytecode.
    async fn disassembly(&self) -> Result<Option<String>> {
        let view = BinaryIndexedView::Module(self.parsed.bytecode());
        Ok(Some(
            Disassembler::from_view(view, Loc::invalid())
                .map_err(|e| Error::Internal(format!("Error creating disassembler: {e}")))
                .extend()?
                .disassemble()
                .map_err(|e| Error::Internal(format!("Error creating disassembly: {e}")))
                .extend()?,
        ))
    }
}

#[ComplexObject]
impl MoveModuleId {
    /// The package that this Move module was defined in
    async fn package(&self, ctx: &Context<'_>) -> Result<MovePackage> {
        ctx.data_unchecked::<PgManager>()
            .fetch_move_package(self.package, None)
            .await
            .extend()?
            .ok_or_else(|| {
                Error::Internal(format!(
                    "Cannot load package for module {}::{}",
                    self.package, self.name,
                ))
            })
            .extend()
    }
}
