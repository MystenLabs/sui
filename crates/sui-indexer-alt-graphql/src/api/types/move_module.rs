// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{anyhow, Context as _};
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    Context, Enum, Object,
};
use move_binary_format::file_format::Visibility;
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Loc;
use sui_package_resolver::Module as ParsedModule;
use tokio::{join, sync::OnceCell};

use crate::{
    api::scalars::{base64::Base64, cursor::JsonCursor},
    config::Limits,
    error::{resource_exhausted, RpcError},
    pagination::{Page, PaginationConfig},
};

use super::{move_function::MoveFunction, move_package::MovePackage};

#[derive(Clone)]
pub(crate) struct MoveModule {
    /// The package that this module was defined in.
    package: MovePackage,

    /// The module's unqualified name.
    name: String,

    /// The lazily loaded contents of the module, in raw and structured form.
    contents: Arc<OnceCell<Option<ModuleContents>>>,
}

/// The visibility modifier describes which modules can access this module member.
///
/// By default, a module member can be called only within the same module.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum MoveVisibility {
    /// A public member can be accessed by any module.
    Public,
    /// A private member can be accessed in the module it is defined in.
    Private,
    /// A friend member can be accessed in the module it is defined in and any other module in its package that is explicitly specified in its friend list.
    Friend,
}

pub(crate) struct ModuleContents {
    native: Vec<u8>,
    pub(crate) parsed: ParsedModule,
}

/// Cursor for iterating over friend modules. Points to the friend by its index in the friend list.
type CFriend = JsonCursor<usize>;

/// Cursor for iterating over functioons in a module. Points to the function by its name.
type CFunction = JsonCursor<String>;

/// Modules are a unit of code organization in Move.
///
/// Modules belong to packages, and contain type and function definitions.
#[Object]
impl MoveModule {
    /// The package that this module was defined in.
    async fn package(&self) -> Option<&MovePackage> {
        Some(&self.package)
    }

    /// The module's unqualified name.
    async fn name(&self) -> &str {
        &self.name
    }

    /// Base64 encoded bytes of the serialized CompiledModule.
    async fn bytes(&self, ctx: &Context<'_>) -> Result<Option<Base64>, RpcError> {
        let Some(contents) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        Ok(Some(Base64::from(contents.native.clone())))
    }

    /// Textual representation of the module's bytecode.
    async fn disassembly(&self, ctx: &Context<'_>) -> Result<Option<String>, RpcError> {
        let limits: &Limits = ctx.data()?;

        let Some(contents) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        Ok(Some(
            Disassembler::from_module_with_max_size(
                contents.parsed.bytecode(),
                Loc::invalid(),
                Some(limits.max_disassembled_module_size),
            )
            .context("Failed to initialize disassembler")?
            .disassemble()
            .map_err(resource_exhausted)?,
        ))
    }

    /// Bytecode format version.
    async fn file_format_version(&self, ctx: &Context<'_>) -> Result<Option<u32>, RpcError> {
        let Some(contents) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        Ok(Some(contents.parsed.bytecode().version()))
    }

    /// Modules that this module considers friends. These modules can call `public(package)` functions in this module.
    async fn friends(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CFriend>,
        last: Option<u64>,
        before: Option<CFriend>,
    ) -> Result<Option<Connection<String, MoveModule>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("MoveModule", "friends");
        let page = Page::from_params(limits, first, after, last, before)?;

        let Some(contents) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        let bytecode = contents.parsed.bytecode();
        let runtime_id = *bytecode.self_id().address();

        let friends = bytecode.friend_decls();
        let cursors = page.paginate_indices(friends.len());

        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);
        for edge in cursors.edges {
            let decl = &friends[*edge.cursor];
            let friend_pkg = bytecode.address_identifier_at(decl.address);
            let friend_mod = bytecode.identifier_at(decl.name);

            if *friend_pkg != runtime_id {
                return Err(anyhow!("Cross-package friend modules").into());
            }

            let friend = MoveModule::with_fq_name(self.package.clone(), friend_mod.to_string());
            conn.edges
                .push(Edge::new(edge.cursor.encode_cursor(), friend));
        }

        Ok(Some(conn))
    }

    /// The function named `name` in this module.
    async fn function(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> Result<Option<MoveFunction>, RpcError> {
        let Some(contents) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        let Some(def) = contents
            .parsed
            .function_def(&name)
            .context("Failed to get function definition")?
        else {
            return Ok(None);
        };

        Ok(Some(MoveFunction::from_def(self.clone(), name, def)))
    }

    /// Paginate through this module's function definitions.
    async fn functions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CFunction>,
        last: Option<u64>,
        before: Option<CFunction>,
    ) -> Result<Option<Connection<String, MoveFunction>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("MoveModule", "functions");
        let page = Page::from_params(limits, first, after, last, before)?;

        let Some(contents) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        let function_range = contents.parsed.functions(
            page.after().map(|c| c.as_ref()),
            page.before().map(|c| c.as_ref()),
        );

        let mut conn = Connection::new(false, false);
        let functions = if page.is_from_front() {
            function_range.take(page.limit()).collect()
        } else {
            let mut functions: Vec<_> = function_range.rev().take(page.limit()).collect();
            functions.reverse();
            functions
        };

        conn.has_previous_page = functions
            .first()
            .is_some_and(|fst| contents.parsed.functions(None, Some(fst)).next().is_some());

        conn.has_next_page = functions
            .last()
            .is_some_and(|lst| contents.parsed.functions(Some(lst), None).next().is_some());

        for function in functions {
            conn.edges.push(Edge::new(
                JsonCursor::new(function.to_owned()).encode_cursor(),
                MoveFunction::with_fq_name(self.clone(), function.to_owned()),
            ));
        }

        Ok(Some(conn))
    }
}

impl MoveModule {
    /// Construct a module that is represented by just its fully-qualified name. This does not
    /// check that the module actually exists, so should not be used to "fetch" a module based on
    /// user input.
    pub(crate) fn with_fq_name(package: MovePackage, name: String) -> Self {
        Self {
            package,
            name,
            contents: Arc::new(OnceCell::new()),
        }
    }

    /// Get the native CompiledModule, loading it lazily if needed.
    pub(crate) async fn contents(
        &self,
        ctx: &Context<'_>,
    ) -> Result<&Option<ModuleContents>, RpcError> {
        self.contents
            .get_or_try_init(|| async {
                let (native, parsed) = join!(self.package.native(ctx), self.package.parsed(ctx));
                let (Some(native), Some(parsed)) = (native?.as_ref(), parsed?.as_ref()) else {
                    return Ok(None);
                };

                let Some(native) = native.serialized_module_map().get(&self.name).cloned() else {
                    return Ok(None);
                };

                let parsed = parsed
                    .module(&self.name)
                    .context("Couldn't find parsed module, for existing native module")?
                    .clone();

                Ok(Some(ModuleContents { native, parsed }))
            })
            .await
    }
}

impl From<Visibility> for MoveVisibility {
    fn from(visibility: Visibility) -> Self {
        use MoveVisibility as M;
        use Visibility as V;

        match visibility {
            V::Private => M::Private,
            V::Public => M::Public,
            V::Friend => M::Friend,
        }
    }
}
