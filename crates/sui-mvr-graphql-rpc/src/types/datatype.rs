// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use super::move_enum::MoveEnum;
use super::move_module::MoveModule;
use super::move_struct::{MoveStruct, MoveStructTypeParameter};
use super::open_move_type::MoveAbility;

/// Interface implemented by all GraphQL types that represent a Move datatype (either structs or
/// enums). This interface is used to provide a way to access fields that are shared by both
/// structs and enums, e.g., the module that the datatype belongs to, the name of the datatype,
/// type parameters etc.
#[derive(Interface)]
#[graphql(
    name = "IMoveDatatype",
    field(
        name = "module",
        ty = "MoveModule",
        desc = "The module that the datatype belongs to."
    ),
    field(name = "name", ty = "String", desc = "The name of the datatype."),
    field(
        name = "abilities",
        ty = "Option<&Vec<MoveAbility>>",
        desc = "The abilities of the datatype."
    ),
    field(
        name = "type_parameters",
        ty = "Option<&Vec<MoveStructTypeParameter>>",
        desc = "The type parameters of the datatype."
    )
)]
pub(crate) enum IMoveDatatype {
    Datatype(MoveDatatype),
    Struct(MoveStruct),
    Enum(MoveEnum),
}

pub(crate) enum MoveDatatype {
    Struct(MoveStruct),
    Enum(MoveEnum),
}

/// The generic representation of a Move datatype (either a struct or an enum) which exposes common
/// fields and information (module, name, abilities, type parameters etc.) that is shared across
/// them.
#[Object]
impl MoveDatatype {
    async fn module(&self, ctx: &Context<'_>) -> Result<MoveModule> {
        match self {
            MoveDatatype::Struct(s) => s.module(ctx).await,
            MoveDatatype::Enum(e) => e.module(ctx).await,
        }
    }

    async fn name(&self, ctx: &Context<'_>) -> Result<&str> {
        match self {
            MoveDatatype::Struct(s) => s.name(ctx).await,
            MoveDatatype::Enum(e) => e.name(ctx).await,
        }
    }

    async fn abilities(&self, ctx: &Context<'_>) -> Result<Option<&Vec<MoveAbility>>> {
        match self {
            MoveDatatype::Struct(s) => s.abilities(ctx).await,
            MoveDatatype::Enum(e) => e.abilities(ctx).await,
        }
    }

    async fn type_parameters(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<&Vec<MoveStructTypeParameter>>> {
        match self {
            MoveDatatype::Struct(s) => s.type_parameters(ctx).await,
            MoveDatatype::Enum(e) => e.type_parameters(ctx).await,
        }
    }

    async fn as_move_enum(&self) -> Option<&MoveEnum> {
        match self {
            MoveDatatype::Enum(e) => Some(e),
            _ => None,
        }
    }

    async fn as_move_struct(&self) -> Option<&MoveStruct> {
        match self {
            MoveDatatype::Struct(s) => Some(s),
            _ => None,
        }
    }
}

impl From<MoveStruct> for MoveDatatype {
    fn from(value: MoveStruct) -> Self {
        MoveDatatype::Struct(value)
    }
}

impl From<MoveEnum> for MoveDatatype {
    fn from(value: MoveEnum) -> Self {
        MoveDatatype::Enum(value)
    }
}
