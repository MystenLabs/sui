// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Object, SimpleObject};
use sui_package_resolver::{DataDef, MoveData, OpenSignatureBody};
use tokio::sync::OnceCell;

use crate::error::RpcError;

use super::{
    move_module::MoveModule,
    move_type::{abilities, MoveAbility},
    open_move_type::OpenMoveType,
};

pub(crate) struct MoveStruct {
    /// The module that this struct is defined in.
    module: MoveModule,

    /// The struct's unqualified name.
    name: String,

    /// The lazily loaded definition of the struct.
    contents: Arc<OnceCell<Option<DataDef>>>,
}

/// Declaration of a type parameter on a Move struct.
#[derive(SimpleObject)]
pub(crate) struct MoveDatatypeTypeParameter {
    /// Ability constraints on this type parameter.
    constraints: Vec<MoveAbility>,

    /// Whether this type parameter is marked `phantom` or not.
    ///
    /// Phantom type parameters are not referenced in the struct's fields.
    is_phantom: bool,
}

struct MoveField<'f> {
    name: &'f str,
    type_: &'f OpenSignatureBody,
}

/// Description of a struct type, defined in a Move module.
#[Object]
impl MoveStruct {
    /// The module that this struct is defined in.
    async fn module(&self) -> &MoveModule {
        &self.module
    }

    /// The struct's unqualified name.
    async fn name(&self) -> &str {
        &self.name
    }

    /// Abilities on this struct definition.
    async fn abilities(&self, ctx: &Context<'_>) -> Result<Option<Vec<MoveAbility>>, RpcError> {
        let Some(def) = self.contents(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(abilities(def.abilities)))
    }

    /// The names and types of the struct's fields.
    ///
    /// Field types reference type parameters by their index in the defining struct's `typeParameters` list.
    async fn fields(&self, ctx: &Context<'_>) -> Result<Option<Vec<MoveField<'_>>>, RpcError> {
        let Some(def) = self.contents(ctx).await? else {
            return Ok(None);
        };

        let MoveData::Struct(fields) = &def.data else {
            return Ok(None);
        };

        Ok(Some(
            fields
                .iter()
                .map(|(name, type_)| MoveField {
                    name: name.as_str(),
                    type_,
                })
                .collect(),
        ))
    }

    /// Constraints on the struct's formal type parameters.
    ///
    /// Move bytecode does not name type parameters, so when they are referenced (e.g. in field types), they are identified by their index in this list.
    async fn type_parameters(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Vec<MoveDatatypeTypeParameter>>, RpcError> {
        let Some(def) = self.contents(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(
            def.type_params
                .iter()
                .map(|param| MoveDatatypeTypeParameter {
                    constraints: abilities(param.constraints),
                    is_phantom: param.is_phantom,
                })
                .collect(),
        ))
    }
}

#[Object]
impl MoveField<'_> {
    /// The field's name.
    async fn name(&self) -> Option<&str> {
        Some(self.name)
    }

    /// The field's type.
    ///
    /// This type can reference type parameters introduced by the defining struct (see `typeParameters`).
    async fn type_(&self) -> Option<OpenMoveType> {
        Some(OpenMoveType::from(self.type_.clone()))
    }
}

impl MoveStruct {
    /// Construct a struct that is represented by its module and name. This does not check that the
    /// datatype exists and is a struct, so should not be used to "fetch" a struct based on user
    /// input.
    pub(crate) fn with_fq_name(module: MoveModule, name: String) -> Self {
        Self {
            module,
            name,
            contents: Arc::new(OnceCell::new()),
        }
    }

    /// Construct a struct with a pre-loaded struct definition. This does not check that the
    /// datatype definition is for a struct, so it is the caller's responsibility to check that.
    pub(crate) fn from_def(module: MoveModule, name: String, def: DataDef) -> Self {
        Self {
            module,
            name,
            contents: Arc::new(OnceCell::from(Some(def))),
        }
    }

    async fn contents(&self, ctx: &Context<'_>) -> Result<&Option<DataDef>, RpcError> {
        self.contents
            .get_or_try_init(|| async {
                let Some(module) = self.module.contents(ctx).await?.as_ref() else {
                    return Ok(None);
                };

                Ok(module
                    .parsed
                    .struct_def(&self.name)
                    .context("Failed to deserialize struct definition")?)
            })
            .await
    }
}
