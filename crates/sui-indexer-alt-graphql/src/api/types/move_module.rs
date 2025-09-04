// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{anyhow, Context as _};
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    Context, Object,
};
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

pub(crate) struct ModuleContents {
    native: Vec<u8>,
    pub(crate) parsed: ParsedModule,
}

type CFriend = JsonCursor<usize>;

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
