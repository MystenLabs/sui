// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_package_resolver::FunctionDef;

use crate::context_data::db_data_provider::PgManager;
use crate::error::Error;

use super::{
    move_module::MoveModule,
    open_move_type::{abilities, MoveAbility, MoveVisibility, OpenMoveType},
    sui_address::SuiAddress,
};

pub(crate) struct MoveFunction {
    package: SuiAddress,
    module: String,
    name: String,
    visibility: MoveVisibility,
    is_entry: bool,
    type_parameters: Vec<MoveFunctionTypeParameter>,
    parameters: Vec<OpenMoveType>,
    return_: Vec<OpenMoveType>,
}

#[derive(SimpleObject)]
pub(crate) struct MoveFunctionTypeParameter {
    constraints: Vec<MoveAbility>,
}

/// Signature of a function, defined in a Move module.
#[Object]
impl MoveFunction {
    /// The module this function was defined in.
    async fn module(&self, ctx: &Context<'_>) -> Result<MoveModule> {
        let Some(module) = ctx
            .data_unchecked::<PgManager>()
            .fetch_move_module(self.package, &self.module)
            .await
            .extend()?
        else {
            return Err(Error::Internal(format!(
                "Failed to load module for function: {}::{}::{}",
                self.package, self.module, self.name,
            )))
            .extend();
        };

        Ok(module)
    }

    /// The function's (unqualified) name.
    async fn name(&self) -> &str {
        &self.name
    }

    /// The function's visibility: `public`, `public(friend)`, or `private`.
    async fn visibility(&self) -> Option<&MoveVisibility> {
        Some(&self.visibility)
    }

    /// Whether the function has the `entry` modifier or not.
    async fn is_entry(&self) -> Option<bool> {
        Some(self.is_entry)
    }

    /// Constraints on the function's formal type parameters.  Move bytecode does not name type
    /// parameters, so when they are referenced (e.g. in parameter and return types) they are
    /// identified by their index in this list.
    async fn type_parameters(&self) -> Option<&Vec<MoveFunctionTypeParameter>> {
        Some(&self.type_parameters)
    }

    /// The function's parameter types.  These types can reference type parameters introduce by this
    /// function (see `typeParameters`).
    async fn parameters(&self) -> Option<&Vec<OpenMoveType>> {
        Some(&self.parameters)
    }

    /// The function's return types.  There can be multiple because functions in Move can return
    /// multiple values.  These types can reference type parameters introduced by this function (see
    /// `typeParameters`).
    #[graphql(name = "return")]
    async fn return_(&self) -> Option<&Vec<OpenMoveType>> {
        Some(&self.return_)
    }
}

impl MoveFunction {
    pub(crate) fn new(package: SuiAddress, module: String, name: String, def: FunctionDef) -> Self {
        let type_parameters = def
            .type_params
            .into_iter()
            .map(|constraints| MoveFunctionTypeParameter {
                constraints: abilities(constraints),
            })
            .collect();

        let parameters = def.parameters.into_iter().map(OpenMoveType::from).collect();
        let return_ = def.return_.into_iter().map(OpenMoveType::from).collect();

        MoveFunction {
            package,
            module,
            name,
            visibility: def.visibility.into(),
            is_entry: def.is_entry,
            type_parameters,
            parameters,
            return_,
        }
    }
}
