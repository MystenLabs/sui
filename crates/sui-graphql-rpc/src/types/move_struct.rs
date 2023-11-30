// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_package_resolver::StructDef;

use crate::context_data::db_data_provider::PgManager;
use crate::error::Error;

use super::{
    move_module::MoveModule,
    open_move_type::{abilities, MoveAbility, OpenMoveType},
    sui_address::SuiAddress,
};

pub(crate) struct MoveStruct {
    defining_id: SuiAddress,
    module: String,
    name: String,
    abilities: Vec<MoveAbility>,
    type_parameters: Vec<MoveStructTypeParameter>,
    fields: Vec<MoveField>,
}

#[derive(SimpleObject)]
pub(crate) struct MoveStructTypeParameter {
    constraints: Vec<MoveAbility>,
    is_phantom: bool,
}

#[derive(SimpleObject)]
#[graphql(complex)]
pub(crate) struct MoveField {
    name: String,
    #[graphql(skip)]
    type_: OpenMoveType,
}

/// Description of a type, defined in a Move module.
#[Object]
impl MoveStruct {
    /// The module this struct was originally defined in.
    async fn module(&self, ctx: &Context<'_>) -> Result<MoveModule> {
        let Some(module) = ctx
            .data_unchecked::<PgManager>()
            .fetch_move_module(self.defining_id, &self.module)
            .await
            .extend()?
        else {
            return Err(Error::Internal(format!(
                "Failed to load module for struct: {}::{}::{}",
                self.defining_id, self.module, self.name,
            )))
            .extend();
        };

        Ok(module)
    }

    /// The struct's (unqualified) type name.
    async fn name(&self) -> &str {
        &self.name
    }

    /// Abilities this struct has.
    async fn abilities(&self) -> Option<&Vec<MoveAbility>> {
        Some(&self.abilities)
    }

    /// Constraints on the struct's formal type parameters.  Move bytecode does not name type
    /// parameters, so when they are referenced (e.g. in field types) they are identified by their
    /// index in this list.
    async fn type_parameters(&self) -> Option<&Vec<MoveStructTypeParameter>> {
        Some(&self.type_parameters)
    }

    /// The names and types of the struct's fields.  Field types reference type parameters, by their
    /// index in the defining struct's `typeParameters` list.
    async fn fields(&self) -> Option<&Vec<MoveField>> {
        Some(&self.fields)
    }
}

#[ComplexObject]
impl MoveField {
    #[graphql(name = "type")]
    async fn type_(&self) -> Option<&OpenMoveType> {
        Some(&self.type_)
    }
}

impl MoveStruct {
    pub(crate) fn new(module: String, name: String, def: StructDef) -> Self {
        let type_parameters = def
            .type_params
            .into_iter()
            .map(|param| MoveStructTypeParameter {
                constraints: abilities(param.constraints),
                is_phantom: param.is_phantom,
            })
            .collect();

        let fields = def
            .fields
            .into_iter()
            .map(|(name, signature)| MoveField {
                name,
                type_: signature.into(),
            })
            .collect();

        MoveStruct {
            defining_id: SuiAddress::from(def.defining_id),
            module,
            name,
            abilities: abilities(def.abilities),
            type_parameters,
            fields,
        }
    }
}
