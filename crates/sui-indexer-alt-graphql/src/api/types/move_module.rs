// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use async_graphql::{
    Context, Enum, Object,
    connection::{Connection, CursorType, Edge},
};
use move_binary_format::file_format::Visibility;
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Loc;
use sui_package_resolver::Module as ParsedModule;
use tokio::{join, sync::OnceCell};

use crate::{
    api::scalars::{base64::Base64, cursor::JsonCursor},
    config::Limits,
    error::{RpcError, resource_exhausted},
    pagination::{Page, PaginationConfig},
};

use super::{
    move_datatype::{MoveDatatype, MoveEnum, MoveStruct},
    move_function::MoveFunction,
    move_package::MovePackage,
};

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

/// Cursor for iterating over datatypes in a module. Points to the datatype by its name.
type CDatatype = JsonCursor<String>;

/// Cursor for iterating over enums in a module. Points to the enum by its name.
type CEnum = JsonCursor<String>;

/// Cursor for iterating over friend modules. Points to the friend by its index in the friend list.
type CFriend = JsonCursor<usize>;

/// Cursor for iterating over functioons in a module. Points to the function by its name.
type CFunction = JsonCursor<String>;

/// Cursor for iterating over structs in a module. Points to the struct by its name.
type CStruct = JsonCursor<String>;

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
    async fn bytes(&self, ctx: &Context<'_>) -> Option<Result<Base64, RpcError>> {
        let contents = self.contents(ctx).await.ok()?.as_ref()?;
        Some(Ok(Base64::from(contents.native.clone())))
    }

    /// The datatype (struct or enum) named `name` in this module.
    async fn datatype(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> Option<Result<MoveDatatype, RpcError>> {
        async {
            let Some(contents) = self.contents(ctx).await?.as_ref() else {
                return Ok(None);
            };

            let Some(def) = contents
                .parsed
                .data_def(&name)
                .context("Failed to get datatype definition")?
            else {
                return Ok(None);
            };

            Ok(Some(MoveDatatype::from_def(self.clone(), name, def)))
        }
        .await
        .transpose()
    }

    /// Paginate through this module's datatype definitions.
    async fn datatypes(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CDatatype>,
        last: Option<u64>,
        before: Option<CDatatype>,
    ) -> Option<Result<Connection<String, MoveDatatype>, RpcError>> {
        async {
            let pagination: &PaginationConfig = ctx.data()?;
            let limits = pagination.limits("MoveModule", "datatypes");
            let page = Page::from_params(limits, first, after, last, before)?;

            let Some(contents) = self.contents(ctx).await?.as_ref() else {
                return Ok(None);
            };

            let datatype_range = contents.parsed.datatypes(
                page.after().map(|c| c.as_ref()),
                page.before().map(|c| c.as_ref()),
            );

            let mut conn = Connection::new(false, false);
            let datatypes = if page.is_from_front() {
                datatype_range.take(page.limit()).collect()
            } else {
                let mut datatypes: Vec<_> = datatype_range.rev().take(page.limit()).collect();
                datatypes.reverse();
                datatypes
            };

            conn.has_previous_page = datatypes
                .first()
                .is_some_and(|fst| contents.parsed.datatypes(None, Some(fst)).next().is_some());

            conn.has_next_page = datatypes
                .last()
                .is_some_and(|lst| contents.parsed.datatypes(Some(lst), None).next().is_some());

            for datatype_name in datatypes {
                conn.edges.push(Edge::new(
                    JsonCursor::new(datatype_name.to_owned()).encode_cursor(),
                    MoveDatatype::with_fq_name(self.clone(), datatype_name.to_owned()),
                ));
            }

            Ok(Some(conn))
        }
        .await
        .transpose()
    }

    /// Textual representation of the module's bytecode.
    async fn disassembly(&self, ctx: &Context<'_>) -> Option<Result<String, RpcError>> {
        async {
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
        .await
        .transpose()
    }

    /// The enum named `name` in this module.
    async fn enum_(&self, ctx: &Context<'_>, name: String) -> Option<Result<MoveEnum, RpcError>> {
        async {
            let Some(contents) = self.contents(ctx).await?.as_ref() else {
                return Ok(None);
            };

            let Some(def) = contents
                .parsed
                .enum_def(&name)
                .context("Failed to get enum definition")?
            else {
                return Ok(None);
            };

            Ok(Some(MoveEnum::from_def(self.clone(), name, def)))
        }
        .await
        .transpose()
    }

    /// Paginate through this module's enum definitions.
    async fn enums(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CEnum>,
        last: Option<u64>,
        before: Option<CEnum>,
    ) -> Option<Result<Connection<String, MoveEnum>, RpcError>> {
        async {
            let pagination: &PaginationConfig = ctx.data()?;
            let limits = pagination.limits("MoveModule", "enums");
            let page = Page::from_params(limits, first, after, last, before)?;

            let Some(contents) = self.contents(ctx).await?.as_ref() else {
                return Ok(None);
            };

            let enum_range = contents.parsed.enums(
                page.after().map(|c| c.as_ref()),
                page.before().map(|c| c.as_ref()),
            );

            let mut conn = Connection::new(false, false);
            let enums = if page.is_from_front() {
                enum_range.take(page.limit()).collect()
            } else {
                let mut enums: Vec<_> = enum_range.rev().take(page.limit()).collect();
                enums.reverse();
                enums
            };

            conn.has_previous_page = enums
                .first()
                .is_some_and(|fst| contents.parsed.enums(None, Some(fst)).next().is_some());

            conn.has_next_page = enums
                .last()
                .is_some_and(|lst| contents.parsed.enums(Some(lst), None).next().is_some());

            for enum_name in enums {
                conn.edges.push(Edge::new(
                    JsonCursor::new(enum_name.to_owned()).encode_cursor(),
                    MoveEnum::with_fq_name(self.clone(), enum_name.to_owned()),
                ));
            }

            Ok(Some(conn))
        }
        .await
        .transpose()
    }

    /// Bytecode format version.
    async fn file_format_version(&self, ctx: &Context<'_>) -> Option<Result<u32, RpcError>> {
        let contents = self.contents(ctx).await.ok()?.as_ref()?;
        Some(Ok(contents.parsed.bytecode().version()))
    }

    /// Modules that this module considers friends. These modules can call `public(package)` functions in this module.
    async fn friends(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CFriend>,
        last: Option<u64>,
        before: Option<CFriend>,
    ) -> Option<Result<Connection<String, MoveModule>, RpcError>> {
        async {
            let pagination: &PaginationConfig = ctx.data()?;
            let limits = pagination.limits("MoveModule", "friends");
            let page = Page::from_params(limits, first, after, last, before)?;

            let Some(contents) = self.contents(ctx).await?.as_ref() else {
                return Ok(None);
            };

            let bytecode = contents.parsed.bytecode();
            let runtime_id = *bytecode.self_id().address();

            let friends = bytecode.friend_decls();
            let conn: Connection<String, MoveModule> =
                page.paginate_indices(friends.len(), |i| -> Result<_, RpcError> {
                    let decl = &friends[i];
                    let friend_pkg = bytecode.address_identifier_at(decl.address);
                    let friend_mod = bytecode.identifier_at(decl.name);

                    if *friend_pkg != runtime_id {
                        return Err(anyhow!("Cross-package friend modules").into());
                    }

                    Ok(MoveModule::with_fq_name(
                        self.package.clone(),
                        friend_mod.to_string(),
                    ))
                })?;
            Ok(Some(conn))
        }
        .await
        .transpose()
    }

    /// The function named `name` in this module.
    async fn function(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> Option<Result<MoveFunction, RpcError>> {
        async {
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
        .await
        .transpose()
    }

    /// Paginate through this module's function definitions.
    async fn functions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CFunction>,
        last: Option<u64>,
        before: Option<CFunction>,
    ) -> Option<Result<Connection<String, MoveFunction>, RpcError>> {
        async {
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
        .await
        .transpose()
    }

    /// The struct named `name` in this module.
    async fn struct_(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> Option<Result<MoveStruct, RpcError>> {
        async {
            let Some(contents) = self.contents(ctx).await?.as_ref() else {
                return Ok(None);
            };

            let Some(def) = contents
                .parsed
                .struct_def(&name)
                .context("Failed to get struct definition")?
            else {
                return Ok(None);
            };

            Ok(Some(MoveStruct::from_def(self.clone(), name, def)))
        }
        .await
        .transpose()
    }

    /// Paginate through this module's struct definitions.
    async fn structs(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CStruct>,
        last: Option<u64>,
        before: Option<CStruct>,
    ) -> Option<Result<Connection<String, MoveStruct>, RpcError>> {
        async {
            let pagination: &PaginationConfig = ctx.data()?;
            let limits = pagination.limits("MoveModule", "structs");
            let page = Page::from_params(limits, first, after, last, before)?;

            let Some(contents) = self.contents(ctx).await?.as_ref() else {
                return Ok(None);
            };

            let struct_range = contents.parsed.structs(
                page.after().map(|c| c.as_ref()),
                page.before().map(|c| c.as_ref()),
            );

            let mut conn = Connection::new(false, false);
            let structs = if page.is_from_front() {
                struct_range.take(page.limit()).collect()
            } else {
                let mut structs: Vec<_> = struct_range.rev().take(page.limit()).collect();
                structs.reverse();
                structs
            };

            conn.has_previous_page = structs
                .first()
                .is_some_and(|fst| contents.parsed.structs(None, Some(fst)).next().is_some());

            conn.has_next_page = structs
                .last()
                .is_some_and(|lst| contents.parsed.structs(Some(lst), None).next().is_some());

            for struct_name in structs {
                conn.edges.push(Edge::new(
                    JsonCursor::new(struct_name.to_owned()).encode_cursor(),
                    MoveStruct::with_fq_name(self.clone(), struct_name.to_owned()),
                ));
            }

            Ok(Some(conn))
        }
        .await
        .transpose()
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
