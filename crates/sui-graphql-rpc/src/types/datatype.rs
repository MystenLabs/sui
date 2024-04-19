// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use super::move_enum::MoveEnum;
use super::move_module::MoveModule;
use super::move_struct::{MoveStruct, MoveStructTypeParameter};
use super::open_move_type::MoveAbility;

#[derive(Interface)]
#[graphql(
    name = "IMoveDatatype",
    field(name = "module", ty = "MoveModule"),
    field(name = "name", ty = "String"),
    field(name = "abilities", ty = "Option<&Vec<MoveAbility>>"),
    field(name = "type_parameters", ty = "Option<&Vec<MoveStructTypeParameter>>")
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
