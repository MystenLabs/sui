// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Object};
use sui_package_resolver::FunctionDef;
use tokio::sync::OnceCell;

use crate::error::RpcError;

use super::move_module::MoveModule;

pub(crate) struct MoveFunction {
    /// The module that this function is defined in.
    module: MoveModule,

    /// The function's unqualified name.
    name: String,

    /// The lazily loaded definition of the function.
    contents: Arc<OnceCell<Option<FunctionDef>>>,
}

/// A function defined in a Move module.
#[Object]
impl MoveFunction {
    /// The module that this function is defined in.
    async fn module(&self) -> &MoveModule {
        &self.module
    }

    /// The function's unqualified name.
    async fn name(&self) -> &str {
        &self.name
    }

    /// Whether the function is marked `entry` or not.
    async fn is_entry(&self, ctx: &Context<'_>) -> Result<Option<bool>, RpcError> {
        let Some(contents) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        Ok(Some(contents.is_entry))
    }
}

impl MoveFunction {
    /// Construct a function that is represented by its module and name. This does not
    /// check that the function actually exists, so should not be used to "fetch" a function based on
    /// user input.
    pub(crate) fn with_fq_name(module: MoveModule, name: String) -> Self {
        Self {
            module,
            name,
            contents: Arc::new(OnceCell::new()),
        }
    }

    /// Construct a function with a pre-loaded function definition.
    pub(crate) fn from_def(module: MoveModule, name: String, def: FunctionDef) -> Self {
        Self {
            module,
            name,
            contents: Arc::new(OnceCell::from(Some(def))),
        }
    }

    /// Get the function definition, loading it lazily if needed.
    async fn contents(&self, ctx: &Context<'_>) -> Result<&Option<FunctionDef>, RpcError> {
        self.contents
            .get_or_try_init(|| async {
                let Some(contents) = self.module.contents(ctx).await? else {
                    return Ok(None);
                };

                let def = contents
                    .parsed
                    .function_def(&self.name)
                    .context("Failed to get function definition")?;

                Ok(def)
            })
            .await
    }
}
