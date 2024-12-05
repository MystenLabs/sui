// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::*;
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Loc;
use sui_types::move_package;

use crate::consistency::{ConsistentIndexCursor, ConsistentNamedCursor};
use crate::error::Error;
use sui_package_resolver::Module as ParsedMoveModule;

use super::cursor::{JsonCursor, Page};
use super::datatype::MoveDatatype;
use super::move_enum::MoveEnum;
use super::move_function::MoveFunction;
use super::move_struct::MoveStruct;
use super::{base64::Base64, move_package::MovePackage, sui_address::SuiAddress};

#[derive(Clone)]
pub(crate) struct MoveModule {
    pub storage_id: SuiAddress,
    pub native: Vec<u8>,
    pub parsed: ParsedMoveModule,
    /// The checkpoint sequence number this was viewed at.
    pub checkpoint_viewed_at: u64,
}

pub(crate) type CFriend = JsonCursor<ConsistentIndexCursor>;
pub(crate) type CStruct = JsonCursor<ConsistentNamedCursor>;
pub(crate) type CFunction = JsonCursor<ConsistentNamedCursor>;

/// Represents a module in Move, a library that defines struct types
/// and functions that operate on these types.
#[Object]
impl MoveModule {
    /// The package that this Move module was defined in
    async fn package(&self, ctx: &Context<'_>) -> Result<MovePackage> {
        MovePackage::query(
            ctx,
            self.storage_id,
            MovePackage::by_id_at(self.checkpoint_viewed_at),
        )
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
    async fn friends(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CFriend>,
        last: Option<u64>,
        before: Option<CFriend>,
    ) -> Result<Connection<String, MoveModule>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let bytecode = self.parsed.bytecode();

        let mut connection = Connection::new(false, false);
        let Some((prev, next, checkpoint_viewed_at, cs)) = page
            .paginate_consistent_indices(bytecode.friend_decls.len(), self.checkpoint_viewed_at)?
        else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        let runtime_id = *bytecode.self_id().address();
        let Some(package) = MovePackage::query(
            ctx,
            self.storage_id,
            MovePackage::by_id_at(checkpoint_viewed_at),
        )
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
        for c in cs {
            let decl = &bytecode.friend_decls[c.ix];
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

            connection.edges.push(Edge::new(c.encode_cursor(), friend));
        }

        Ok(connection)
    }

    /// Look-up the definition of a struct defined in this module, by its name.
    #[graphql(name = "struct")]
    async fn struct_(&self, name: String) -> Result<Option<MoveStruct>> {
        self.struct_impl(name).extend()
    }

    /// Iterate through the structs defined in this module.
    async fn structs(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CStruct>,
        last: Option<u64>,
        before: Option<CStruct>,
    ) -> Result<Option<Connection<String, MoveStruct>>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let after = page.after().map(|a| a.name.as_str());
        let before = page.before().map(|b| b.name.as_str());
        let struct_range = self.parsed.structs(after, before);

        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at = cursor_viewed_at.unwrap_or(self.checkpoint_viewed_at);

        let mut connection = Connection::new(false, false);
        let struct_names = if page.is_from_front() {
            struct_range.take(page.limit()).collect()
        } else {
            let mut names: Vec<_> = struct_range.rev().take(page.limit()).collect();
            names.reverse();
            names
        };

        connection.has_previous_page = struct_names
            .first()
            .is_some_and(|fst| self.parsed.structs(None, Some(fst)).next().is_some());

        connection.has_next_page = struct_names
            .last()
            .is_some_and(|lst| self.parsed.structs(Some(lst), None).next().is_some());

        for name in struct_names {
            let Some(struct_) = self.struct_impl(name.to_string()).extend()? else {
                return Err(Error::Internal(format!(
                    "Cannot deserialize struct {name} in module {}::{}",
                    self.storage_id,
                    self.parsed.name(),
                )))
                .extend();
            };

            let cursor = JsonCursor::new(ConsistentNamedCursor {
                name: name.to_string(),
                c: checkpoint_viewed_at,
            })
            .encode_cursor();
            connection.edges.push(Edge::new(cursor, struct_));
        }

        if connection.edges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(connection))
        }
    }

    /// Look-up the definition of a enum defined in this module, by its name.
    #[graphql(name = "enum")]
    async fn enum_(&self, name: String) -> Result<Option<MoveEnum>> {
        self.enum_impl(name).extend()
    }

    /// Iterate through the enums defined in this module.
    async fn enums(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CStruct>,
        last: Option<u64>,
        before: Option<CStruct>,
    ) -> Result<Option<Connection<String, MoveEnum>>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let after = page.after().map(|a| a.name.as_str());
        let before = page.before().map(|b| b.name.as_str());
        let enum_range = self.parsed.enums(after, before);

        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at = cursor_viewed_at.unwrap_or(self.checkpoint_viewed_at);

        let mut connection = Connection::new(false, false);
        let enum_names = if page.is_from_front() {
            enum_range.take(page.limit()).collect()
        } else {
            let mut names: Vec<_> = enum_range.rev().take(page.limit()).collect();
            names.reverse();
            names
        };

        connection.has_previous_page = enum_names
            .first()
            .is_some_and(|fst| self.parsed.enums(None, Some(fst)).next().is_some());

        connection.has_next_page = enum_names
            .last()
            .is_some_and(|lst| self.parsed.enums(Some(lst), None).next().is_some());

        for name in enum_names {
            let Some(enum_) = self.enum_impl(name.to_string()).extend()? else {
                return Err(Error::Internal(format!(
                    "Cannot deserialize enum {name} in module {}::{}",
                    self.storage_id,
                    self.parsed.name(),
                )))
                .extend();
            };

            let cursor = JsonCursor::new(ConsistentNamedCursor {
                name: name.to_string(),
                c: checkpoint_viewed_at,
            })
            .encode_cursor();
            connection.edges.push(Edge::new(cursor, enum_));
        }

        if connection.edges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(connection))
        }
    }

    /// Look-up the definition of a datatype (struct or enum) defined in this module, by its name.
    async fn datatype(&self, name: String) -> Result<Option<MoveDatatype>> {
        match self.struct_impl(name.clone()) {
            Ok(Some(s)) => Ok(Some(MoveDatatype::Struct(s))),
            Ok(None) => self
                .enum_impl(name)
                .map(|x| x.map(MoveDatatype::Enum))
                .extend(),
            Err(e) => Err(e.into()),
        }
    }

    /// Iterate through the datatypes (enmums and structs) defined in this module.
    async fn datatypes(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CStruct>,
        last: Option<u64>,
        before: Option<CStruct>,
    ) -> Result<Option<Connection<String, MoveDatatype>>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let after = page.after().map(|a| a.name.as_str());
        let before = page.before().map(|b| b.name.as_str());
        let datatype_range = self.parsed.datatypes(after, before);

        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at = cursor_viewed_at.unwrap_or(self.checkpoint_viewed_at);

        let mut connection = Connection::new(false, false);
        let datatype_names = if page.is_from_front() {
            datatype_range.take(page.limit()).collect()
        } else {
            let mut names: Vec<_> = datatype_range.rev().take(page.limit()).collect();
            names.reverse();
            names
        };

        connection.has_previous_page = datatype_names
            .first()
            .is_some_and(|fst| self.parsed.datatypes(None, Some(fst)).next().is_some());

        connection.has_next_page = datatype_names
            .last()
            .is_some_and(|lst| self.parsed.datatypes(Some(lst), None).next().is_some());

        for name in datatype_names {
            let datatype = match self.struct_impl(name.to_string()) {
                Ok(None) => self
                    .enum_impl(name.to_string())
                    .map(|x| x.map(MoveDatatype::Enum))
                    .extend()?,
                Ok(Some(s)) => Some(MoveDatatype::Struct(s)),
                Err(e) => return Err(e.into()),
            }
            .ok_or_else(|| {
                Error::Internal(format!(
                    "Cannot deserialize datatype {name} in module {}::{}",
                    self.storage_id,
                    self.parsed.name(),
                ))
            })?;

            let cursor = JsonCursor::new(ConsistentNamedCursor {
                name: name.to_string(),
                c: checkpoint_viewed_at,
            })
            .encode_cursor();
            connection.edges.push(Edge::new(cursor, datatype));
        }

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
    async fn functions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CFunction>,
        last: Option<u64>,
        before: Option<CFunction>,
    ) -> Result<Option<Connection<String, MoveFunction>>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let after = page.after().map(|a| a.name.as_str());
        let before = page.before().map(|b| b.name.as_str());
        let function_range = self.parsed.functions(after, before);

        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at = cursor_viewed_at.unwrap_or(self.checkpoint_viewed_at);

        let mut connection = Connection::new(false, false);
        let function_names = if page.is_from_front() {
            function_range.take(page.limit()).collect()
        } else {
            let mut names: Vec<_> = function_range.rev().take(page.limit()).collect();
            names.reverse();
            names
        };

        connection.has_previous_page = function_names
            .first()
            .is_some_and(|fst| self.parsed.functions(None, Some(fst)).next().is_some());

        connection.has_next_page = function_names
            .last()
            .is_some_and(|lst| self.parsed.functions(Some(lst), None).next().is_some());

        for name in function_names {
            let Some(function) = self.function_impl(name.to_string()).extend()? else {
                return Err(Error::Internal(format!(
                    "Cannot deserialize function {name} in module {}::{}",
                    self.storage_id,
                    self.parsed.name(),
                )))
                .extend();
            };

            let cursor = JsonCursor::new(ConsistentNamedCursor {
                name: name.to_string(),
                c: checkpoint_viewed_at,
            })
            .encode_cursor();
            connection.edges.push(Edge::new(cursor, function));
        }

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
        Ok(Some(
            Disassembler::from_module_with_max_size(
                self.parsed.bytecode(),
                Loc::invalid(),
                *move_package::MAX_DISASSEMBLED_MODULE_SIZE,
            )
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

        MoveStruct::new(
            self.parsed.name().to_string(),
            name,
            def,
            self.checkpoint_viewed_at,
        )
        .map(Option::Some)
    }

    fn enum_impl(&self, name: String) -> Result<Option<MoveEnum>, Error> {
        let def = match self.parsed.enum_def(&name) {
            Ok(Some(def)) => def,
            Ok(None) => return Ok(None),
            Err(e) => return Err(Error::Internal(e.to_string())),
        };

        MoveEnum::new(
            self.parsed.name().to_string(),
            name,
            def,
            self.checkpoint_viewed_at,
        )
        .map(Option::Some)
    }

    pub(crate) fn function_impl(&self, name: String) -> Result<Option<MoveFunction>, Error> {
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
            self.checkpoint_viewed_at,
        )))
    }

    pub(crate) async fn query(
        ctx: &Context<'_>,
        address: SuiAddress,
        name: &str,
        checkpoint_viewed_at: u64,
    ) -> Result<Option<Self>, Error> {
        let Some(package) =
            MovePackage::query(ctx, address, MovePackage::by_id_at(checkpoint_viewed_at)).await?
        else {
            return Ok(None);
        };

        package.module_impl(name)
    }
}
