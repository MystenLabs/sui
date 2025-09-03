// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Object};
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Loc;
use sui_package_resolver::Module as ParsedModule;
use tokio::{join, sync::OnceCell};

use crate::{
    api::scalars::base64::Base64,
    config::Limits,
    error::{resource_exhausted, RpcError},
};

use super::move_package::MovePackage;

pub(crate) struct MoveModule {
    /// The package that this module was defined in.
    package: MovePackage,

    /// The module's unqualified name.
    name: String,

    /// The lazily loaded contents of the module, in raw and structured form.
    contents: Arc<OnceCell<Option<ModuleContents>>>,
}

struct ModuleContents {
    native: Vec<u8>,
    parsed: ParsedModule,
}

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
    async fn contents(&self, ctx: &Context<'_>) -> Result<&Option<ModuleContents>, RpcError> {
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
