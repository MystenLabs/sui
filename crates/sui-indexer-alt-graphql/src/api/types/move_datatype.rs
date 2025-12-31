// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Interface, Object, SimpleObject};
use sui_package_resolver::{DataDef, MoveData, OpenSignatureBody, VariantDef};
use tokio::sync::OnceCell;

use crate::error::RpcError;

use super::{
    move_module::MoveModule,
    move_type::{MoveAbility, abilities},
    open_move_type::OpenMoveType,
};

/// Interface implemented by all GraphQL types that represent a Move datatype definition (either a struct or an enum definition).
///
/// This interface is used to provide a way to access fields that are shared by both structs and enums, e.g., the module that the datatype belongs to, the name of the datatype, type parameters etc.
#[allow(clippy::duplicated_attributes)]
#[allow(clippy::enum_variant_names)]
#[derive(Interface)]
#[graphql(
    name = "IMoveDatatype",
    field(
        name = "module",
        ty = "Result<&MoveModule, RpcError>",
        desc = "The module that this datatype is defined in",
    ),
    field(
        name = "name",
        ty = "Result<&str, RpcError>",
        desc = "The datatype's unqualified name",
    ),
    field(
        name = "abilities",
        ty = "Option<Result<Vec<MoveAbility>, RpcError>>",
        desc = "Abilities on this datatype definition.",
    ),
    field(
        name = "type_parameters",
        ty = "Option<Result<Vec<MoveDatatypeTypeParameter>, RpcError>>",
        desc = "Constraints on the datatype's formal type parameters\n\nMove bytecode does not name type parameters, so when they are referenced (e.g. in field types), they are identified by their index in this list.",
    )
)]
pub(crate) enum IMoveDatatype {
    Datatype(MoveDatatype),
    Enum(MoveEnum),
    Struct(MoveStruct),
}

#[derive(Clone)]
pub(crate) struct MoveDatatype {
    /// The module that this datatype is defined in.
    module: MoveModule,

    /// The datatype's unqualified name.
    name: String,

    /// The lazily loaded definition of the datatype.
    contents: Arc<OnceCell<Option<DataDef>>>,
}

pub(crate) struct MoveEnum {
    super_: MoveDatatype,
}

pub(crate) struct MoveStruct {
    super_: MoveDatatype,
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

struct MoveEnumVariant<'v>(&'v VariantDef);

struct MoveField<'f> {
    name: &'f str,
    type_: &'f OpenSignatureBody,
}

/// Description of a datatype, defined in a Move module.
#[Object]
impl MoveDatatype {
    /// The module that this datatype is defined in.
    async fn module(&self, _ctx: &Context<'_>) -> Result<&MoveModule, RpcError> {
        Ok(&self.module)
    }

    /// The datatype's unqualified name.
    async fn name(&self, _ctx: &Context<'_>) -> Result<&str, RpcError> {
        Ok(&self.name)
    }

    /// Abilities on this datatype definition.
    async fn abilities(&self, ctx: &Context<'_>) -> Option<Result<Vec<MoveAbility>, RpcError>> {
        let def = self.contents(ctx).await.ok()?.as_ref()?;
        Some(Ok(abilities(def.abilities)))
    }

    /// Attempts to convert the `MoveDatatype` to a `MoveStruct`.
    async fn as_move_struct(&self, ctx: &Context<'_>) -> Option<Result<MoveStruct, RpcError>> {
        let def = self.contents(ctx).await.ok()?.as_ref()?;
        matches!(def.data, MoveData::Struct(_)).then(|| {
            Ok(MoveStruct {
                super_: self.clone(),
            })
        })
    }

    /// Attempts to convert the `MoveDatatype` to a `MoveEnum`.
    async fn as_move_enum(&self, ctx: &Context<'_>) -> Option<Result<MoveEnum, RpcError>> {
        let def = self.contents(ctx).await.ok()?.as_ref()?;
        matches!(def.data, MoveData::Enum(_)).then(|| {
            Ok(MoveEnum {
                super_: self.clone(),
            })
        })
    }

    /// Constraints on the datatype's formal type parameters.
    ///
    /// Move bytecode does not name type parameters, so when they are referenced (e.g. in field types), they are identified by their index in this list.
    async fn type_parameters(
        &self,
        ctx: &Context<'_>,
    ) -> Option<Result<Vec<MoveDatatypeTypeParameter>, RpcError>> {
        let def = self.contents(ctx).await.ok()?.as_ref()?;
        Some(Ok(def
            .type_params
            .iter()
            .map(|param| MoveDatatypeTypeParameter {
                constraints: abilities(param.constraints),
                is_phantom: param.is_phantom,
            })
            .collect()))
    }
}

/// Description of an enum type, defined in a Move module.
#[Object]
impl MoveEnum {
    /// The module that this enum is defined in.
    async fn module(&self, ctx: &Context<'_>) -> Result<&MoveModule, RpcError> {
        self.super_.module(ctx).await
    }

    /// The enum's unqualified name.
    async fn name(&self, ctx: &Context<'_>) -> Result<&str, RpcError> {
        self.super_.name(ctx).await
    }

    /// Abilities on this enum definition.
    async fn abilities(&self, ctx: &Context<'_>) -> Option<Result<Vec<MoveAbility>, RpcError>> {
        self.super_.abilities(ctx).await.ok()?
    }

    /// The names and fields of the enum's variants
    ///
    /// Field types reference type parameters by their index in the defining enum's `typeParameters` list.
    async fn variants(
        &self,
        ctx: &Context<'_>,
    ) -> Option<Result<Vec<MoveEnumVariant<'_>>, RpcError>> {
        let def = self.super_.contents(ctx).await.ok()?.as_ref()?;
        let MoveData::Enum(variants) = &def.data else {
            return None;
        };
        Some(Ok(variants.iter().map(MoveEnumVariant).collect()))
    }

    /// Constraints on the enum's formal type parameters.
    ///
    /// Move bytecode does not name type parameters, so when they are referenced (e.g. in field types), they are identified by their index in this list.
    async fn type_parameters(
        &self,
        ctx: &Context<'_>,
    ) -> Option<Result<Vec<MoveDatatypeTypeParameter>, RpcError>> {
        self.super_.type_parameters(ctx).await.ok()?
    }
}

/// Description of a struct type, defined in a Move module.
#[Object]
impl MoveStruct {
    /// The module that this struct is defined in.
    async fn module(&self, ctx: &Context<'_>) -> Result<&MoveModule, RpcError> {
        self.super_.module(ctx).await
    }

    /// The struct's unqualified name.
    async fn name(&self, ctx: &Context<'_>) -> Result<&str, RpcError> {
        self.super_.name(ctx).await
    }

    /// Abilities on this struct definition.
    async fn abilities(&self, ctx: &Context<'_>) -> Option<Result<Vec<MoveAbility>, RpcError>> {
        self.super_.abilities(ctx).await.ok()?
    }

    /// The names and types of the struct's fields.
    ///
    /// Field types reference type parameters by their index in the defining struct's `typeParameters` list.
    async fn fields(&self, ctx: &Context<'_>) -> Option<Result<Vec<MoveField<'_>>, RpcError>> {
        let def = self.super_.contents(ctx).await.ok()?.as_ref()?;
        let MoveData::Struct(fields) = &def.data else {
            return None;
        };
        Some(Ok(fields
            .iter()
            .map(|(name, type_)| MoveField {
                name: name.as_str(),
                type_,
            })
            .collect()))
    }

    /// Constraints on the struct's formal type parameters.
    ///
    /// Move bytecode does not name type parameters, so when they are referenced (e.g. in field types), they are identified by their index in this list.
    async fn type_parameters(
        &self,
        ctx: &Context<'_>,
    ) -> Option<Result<Vec<MoveDatatypeTypeParameter>, RpcError>> {
        self.super_.type_parameters(ctx).await.ok()?
    }
}

#[Object]
impl MoveEnumVariant<'_> {
    /// The variant's name.
    async fn name(&self) -> Option<&str> {
        Some(self.0.name.as_ref())
    }

    /// The names and types of the variant's fields.
    ///
    /// Field types reference type parameters by their index in the defining struct's `typeParameters` list.
    async fn fields(&self) -> Option<Vec<MoveField<'_>>> {
        Some(
            self.0
                .signatures
                .iter()
                .map(|(name, type_)| MoveField {
                    name: name.as_str(),
                    type_,
                })
                .collect(),
        )
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

impl MoveDatatype {
    /// Construct a datatype that is represented by its fully-qualified name (package, module and
    /// name). This does not check that the datatype exists , so should not be used to "fetch" a
    /// datatype based on user input.
    pub(crate) fn with_fq_name(module: MoveModule, name: String) -> Self {
        Self {
            module,
            name,
            contents: Arc::new(OnceCell::new()),
        }
    }

    /// Construct a datatype with a pre-loaded definition.
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
                    .data_def(&self.name)
                    .context("Failed to deserialize datatype definition")?)
            })
            .await
    }
}

impl MoveEnum {
    pub(crate) fn with_fq_name(module: MoveModule, name: String) -> Self {
        Self {
            super_: MoveDatatype::with_fq_name(module, name),
        }
    }

    pub(crate) fn from_def(module: MoveModule, name: String, def: DataDef) -> Self {
        Self {
            super_: MoveDatatype::from_def(module, name, def),
        }
    }
}

impl MoveStruct {
    pub(crate) fn with_fq_name(module: MoveModule, name: String) -> Self {
        Self {
            super_: MoveDatatype::with_fq_name(module, name),
        }
    }

    pub(crate) fn from_def(module: MoveModule, name: String, def: DataDef) -> Self {
        Self {
            super_: MoveDatatype::from_def(module, name, def),
        }
    }
}
