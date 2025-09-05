// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Object, SimpleObject};
use sui_package_resolver::FunctionDef;
use tokio::sync::OnceCell;

use crate::error::RpcError;

use super::{
    move_module::{MoveModule, MoveVisibility},
    move_type::{abilities, MoveAbility},
    open_move_type::OpenMoveType,
};

pub(crate) struct MoveFunction {
    /// The module that this function is defined in.
    module: MoveModule,

    /// The function's unqualified name.
    name: String,

    /// The lazily loaded definition of the function.
    contents: Arc<OnceCell<Option<FunctionDef>>>,
}

/// Declaration of a type parameter on a Move function.
#[derive(SimpleObject)]
pub(crate) struct MoveFunctionTypeParameter {
    /// Ability constraints on this type parameter.
    constraints: Vec<MoveAbility>,
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

    /// The function's parameter types. These types can reference type parameters introduced by this function (see `typeParameters`).
    async fn parameters(&self, ctx: &Context<'_>) -> Result<Option<Vec<OpenMoveType>>, RpcError> {
        let Some(contents) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        Ok(Some(
            contents
                .parameters
                .iter()
                .cloned()
                .map(OpenMoveType::from)
                .collect(),
        ))
    }

    /// The function's return types. There can be multiple because functions in Move can return multiple values. These types can reference type parameters introduced by this function (see `typeParameters`).
    async fn return_(&self, ctx: &Context<'_>) -> Result<Option<Vec<OpenMoveType>>, RpcError> {
        let Some(contents) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        Ok(Some(
            contents
                .return_
                .iter()
                .cloned()
                .map(OpenMoveType::from)
                .collect(),
        ))
    }

    /// Constraints on the function's formal type parameters.
    ///
    /// Move bytecode does not name type parameters, so when they are referenced (e.g. in parameter and return types), they are identified by their index in this list.
    async fn type_parameters(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Vec<MoveFunctionTypeParameter>>, RpcError> {
        let Some(contents) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        Ok(Some(
            contents
                .type_params
                .iter()
                .map(|c| MoveFunctionTypeParameter {
                    constraints: abilities(*c),
                })
                .collect(),
        ))
    }

    /// The function's visibility: `public`, `public(friend)`, or `private`.
    async fn visibility(&self, ctx: &Context<'_>) -> Result<Option<MoveVisibility>, RpcError> {
        let Some(contents) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        Ok(Some(contents.visibility.into()))
    }
}

impl MoveFunction {
    /// Construct a function that is represented by its fully-qualified name (package, module and
    /// name). This does not check that the function actually exists, so should not be used to
    /// "fetch" a function based on user input.
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
