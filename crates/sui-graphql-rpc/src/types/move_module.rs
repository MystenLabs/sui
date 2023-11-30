// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::connection::{Connection, Edge};
use async_graphql::*;
use move_binary_format::access::ModuleAccess;
use move_binary_format::binary_views::BinaryIndexedView;
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Loc;

use crate::config::ServiceConfig;
use crate::context_data::db_data_provider::{validate_cursor_pagination, PgManager};
use crate::error::Error;
use sui_package_resolver::Module as ParsedMoveModule;

use super::move_function::MoveFunction;
use super::move_struct::MoveStruct;
use super::{base64::Base64, move_package::MovePackage, sui_address::SuiAddress};

#[derive(Clone)]
pub(crate) struct MoveModule {
    pub storage_id: SuiAddress,
    pub native: Vec<u8>,
    pub parsed: ParsedMoveModule,
}

/// Represents a module in Move, a library that defines struct types
/// and functions that operate on these types.
#[Object]
impl MoveModule {
    /// The package that this Move module was defined in
    async fn package(&self, ctx: &Context<'_>) -> Result<MovePackage> {
        ctx.data_unchecked::<PgManager>()
            .fetch_move_package(self.storage_id, None)
            .await
            .extend()?
            .ok_or_else(|| {
                Error::Internal(format!(
                    "Cannot load package for module {}::{}",
                    self.storage_id,
                    self.parsed.name(),
                ))
            })
            .extend()
    }

    /// The module's (unqualified) name.
    async fn name(&self) -> &str {
        self.parsed.name()
    }

    /// Format version of this module's bytecode.
    async fn file_format_version(&self) -> u32 {
        self.parsed.bytecode().version
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

        let runtime_id = *bytecode.self_id().address();
        let Some(package) = ctx
            .data_unchecked::<PgManager>()
            .fetch_move_package(self.storage_id, None)
            .await
            .extend()?
        else {
            return Err(Error::Internal(format!(
                "Failed to load package for module: {}",
                self.storage_id,
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

            if friend_pkg != &runtime_id {
                return Err(Error::Internal(format!(
                    "Friend module of {} from a different package: {}::{}",
                    runtime_id.to_canonical_display(/* with_prefix */ true),
                    friend_pkg.to_canonical_display(/* with_prefix */ true),
                    friend_mod,
                ))
                .extend());
            }

            let Some(friend) = package.module_impl(friend_mod.as_str()).extend()? else {
                return Err(Error::Internal(format!(
                    "Failed to load friend module of {}::{}: {}",
                    self.storage_id,
                    self.parsed.name(),
                    friend_mod,
                ))
                .extend());
            };

            connection.edges.push(Edge::new(idx.to_string(), friend));
        }

        Ok(connection)
    }

    /// Look-up the definition of a struct defined in this module, by its name.
    #[graphql(name = "struct")]
    async fn struct_(&self, name: String) -> Result<Option<MoveStruct>> {
        self.struct_impl(name).extend()
    }

    /// Iterate through the structs defined in this module.
    async fn struct_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, MoveStruct>>> {
        let default_page_size = ctx
            .data::<ServiceConfig>()
            .map_err(|_| Error::Internal("Unable to fetch service configuration.".to_string()))
            .extend()?
            .limits
            .max_page_size;

        // TODO: make cursor opaque.
        // for now it same as struct name
        validate_cursor_pagination(&first, &after, &last, &before).extend()?;

        let struct_range = self.parsed.structs(after.as_deref(), before.as_deref());

        let total = struct_range.clone().count() as u64;
        let (skip, take) = match (first, last) {
            (Some(first), Some(last)) if last < first => (first - last, last),
            (Some(first), _) => (0, first),
            (None, Some(last)) if last < total => (total - last, last),
            (None, _) => (0, default_page_size),
        };

        let mut connection = Connection::new(false, false);
        for name in struct_range.skip(skip as usize).take(take as usize) {
            let Some(struct_) = self.struct_impl(name.to_string()).extend()? else {
                return Err(Error::Internal(format!(
                    "Cannot deserialize struct {name} in module {}::{}",
                    self.storage_id,
                    self.parsed.name(),
                )))
                .extend();
            };

            connection.edges.push(Edge::new(name.to_string(), struct_));
        }

        connection.has_previous_page = connection.edges.first().is_some_and(|fst| {
            self.parsed
                .structs(None, Some(&fst.cursor))
                .next()
                .is_some()
        });

        connection.has_next_page = connection.edges.last().is_some_and(|lst| {
            self.parsed
                .structs(Some(&lst.cursor), None)
                .next()
                .is_some()
        });

        if connection.edges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(connection))
        }
    }

    /// Look-up the signature of a function defined in this module, by its name.
    async fn function(&self, name: String) -> Result<Option<MoveFunction>> {
        self.function_impl(name).extend()
    }

    /// Iterate through the signatures of functions defined in this module.
    async fn function_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, MoveFunction>>> {
        let default_page_size = ctx
            .data::<ServiceConfig>()
            .map_err(|_| Error::Internal("Unable to fetch service configuration.".to_string()))
            .extend()?
            .limits
            .max_page_size;

        // TODO: make cursor opaque.
        // for now it same as function name
        validate_cursor_pagination(&first, &after, &last, &before).extend()?;

        let function_range = self.parsed.functions(after.as_deref(), before.as_deref());

        let total = function_range.clone().count() as u64;
        let (skip, take) = match (first, last) {
            (Some(first), Some(last)) if last < first => (first - last, last),
            (Some(first), _) => (0, first),
            (None, Some(last)) if last < total => (total - last, last),
            (None, _) => (0, default_page_size),
        };

        let mut connection = Connection::new(false, false);
        for name in function_range.skip(skip as usize).take(take as usize) {
            let Some(function) = self.function_impl(name.to_string()).extend()? else {
                return Err(Error::Internal(format!(
                    "Cannot deserialize function {name} in module {}::{}",
                    self.storage_id,
                    self.parsed.name(),
                )))
                .extend();
            };

            connection.edges.push(Edge::new(name.to_string(), function));
        }

        connection.has_previous_page = connection.edges.first().is_some_and(|fst| {
            self.parsed
                .functions(None, Some(&fst.cursor))
                .next()
                .is_some()
        });

        connection.has_next_page = connection.edges.last().is_some_and(|lst| {
            self.parsed
                .functions(Some(&lst.cursor), None)
                .next()
                .is_some()
        });

        if connection.edges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(connection))
        }
    }

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

impl MoveModule {
    fn struct_impl(&self, name: String) -> Result<Option<MoveStruct>, Error> {
        let def = match self.parsed.struct_def(&name) {
            Ok(Some(def)) => def,
            Ok(None) => return Ok(None),
            Err(e) => return Err(Error::Internal(e.to_string())),
        };

        Ok(Some(MoveStruct::new(
            self.parsed.name().to_string(),
            name,
            def,
        )))
    }

    fn function_impl(&self, name: String) -> Result<Option<MoveFunction>, Error> {
        let def = match self.parsed.function_def(&name) {
            Ok(Some(def)) => def,
            Ok(None) => return Ok(None),
            Err(e) => return Err(Error::Internal(e.to_string())),
        };

        Ok(Some(MoveFunction::new(
            self.storage_id,
            self.parsed.name().to_string(),
            name,
            def,
        )))
    }
}
