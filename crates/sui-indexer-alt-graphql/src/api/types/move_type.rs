// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use anyhow::anyhow;
use async_graphql::{scalar, Enum, Object};
use move_binary_format::file_format::{Ability, AbilitySet};
use move_core_types::annotated_value as A;
use serde::{Deserialize, Serialize};
use sui_package_resolver::error::Error as ResolverError;
use sui_types::{type_input::TypeInput, TypeTag};

use crate::{
    error::{bad_user_input, resource_exhausted, RpcError},
    scope::Scope,
};

/// Abilities are keywords in Sui Move that define how types behave at the compiler level.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum MoveAbility {
    /// Enables values to be copied.
    Copy,
    /// Enables values to be popped/dropped.
    Drop,
    /// Enables values to be held directly in global storage.
    Key,
    /// Enables values to be held inside a struct in global storage.
    Store,
}

#[derive(Clone)]
pub(crate) struct MoveType {
    native: TypeInput,
    scope: Scope,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) enum MoveTypeSignature {
    Address,
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Vector(Box<MoveTypeSignature>),
    Datatype {
        package: String,
        module: String,
        #[serde(rename = "type")]
        type_: String,
        #[serde(rename = "typeParameters")]
        type_parameters: Vec<MoveTypeSignature>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum MoveTypeLayout {
    Address,
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Vector(Box<MoveTypeLayout>),
    Struct(MoveStructLayout),
    Enum(MoveEnumLayout),
}

#[derive(Serialize, Deserialize)]
pub(crate) struct MoveStructLayout {
    #[serde(rename = "type")]
    type_: String,
    fields: Vec<MoveFieldLayout>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct MoveEnumLayout {
    variants: Vec<MoveVariantLayout>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct MoveVariantLayout {
    name: String,
    layout: Vec<MoveFieldLayout>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct MoveFieldLayout {
    name: String,
    layout: MoveTypeLayout,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Error parsing type: {0}")]
    Parse(#[from] anyhow::Error),

    #[error("Error resolving type: {0}")]
    Resolve(#[from] ResolverError),
}

/// Represents instances of concrete types (no type parameters, no references).
#[Object]
impl MoveType {
    /// Flat representation of the type signature, as a displayable string.
    async fn repr(&self) -> String {
        self.native.to_canonical_string(/* with_prefix */ true)
    }

    /// Structured representation of the type signature.
    async fn signature(&self) -> Result<MoveTypeSignature, RpcError> {
        MoveTypeSignature::try_from(self.native.clone())
    }

    /// Structured representation of the "shape" of values that match this type. May return no
    /// layout if the type is invalid.
    async fn layout(&self) -> Result<Option<MoveTypeLayout>, RpcError> {
        let Some(layout) = self.layout_impl().await? else {
            return Ok(None);
        };

        Ok(Some(MoveTypeLayout::try_from(layout)?))
    }

    /// The abilities this concrete type has. Returns no abilities if the type is invalid.
    async fn abilities(&self) -> Result<Option<Vec<MoveAbility>>, RpcError> {
        Ok(self.abilities_impl().await?.map(abilities))
    }
}

scalar!(
    MoveTypeSignature,
    "MoveTypeSignature",
    "The signature of a concrete Move Type (a type with all its type parameters instantiated with \
     concrete types, that contains no references), corresponding to the following recursive type:

type MoveTypeSignature =
    \"address\"
  | \"bool\"
  | \"u8\" | \"u16\" | ... | \"u256\"
  | { vector: MoveTypeSignature }
  | {
      datatype: {
        package: string,
        module: string,
        type: string,
        typeParameters: [MoveTypeSignature],
      }
    }"
);

scalar!(
    MoveTypeLayout,
    "MoveTypeLayout",
    "The shape of a concrete Move Type (a type with all its type parameters instantiated with \
     concrete types), corresponding to the following recursive type:

type MoveTypeLayout =
    \"address\"
  | \"bool\"
  | \"u8\" | \"u16\" | ... | \"u256\"
  | { vector: MoveTypeLayout }
  | {
      struct: {
        type: string,
        fields: [{ name: string, layout: MoveTypeLayout }],
      }
    }
  | { enum: [{
          type: string,
          variants: [{
              name: string,
              fields: [{ name: string, layout: MoveTypeLayout }],
          }]
      }]
  }"
);

impl MoveType {
    /// Read a type from user input. Canonicalizing involves replacing package addresses, to the
    /// ones that originally defined the type, while in the input a type can be referred to at any
    /// package at or after the one that first defined it.
    pub(crate) async fn canonicalize(
        input: TypeTag,
        scope: Scope,
    ) -> Result<Option<Self>, RpcError<Error>> {
        use ResolverError as RE;

        let canonical = match scope.package_resolver().canonical_type(input.clone()).await {
            Ok(canonical) => canonical,

            Err(
                RE::FunctionNotFound(_, _, _)
                | RE::ModuleNotFound(_, _)
                | RE::NotAPackage(_)
                | RE::NotAnIdentifier(_)
                | RE::PackageNotFound(_)
                | RE::DatatypeNotFound(_, _, _),
            ) => return Ok(None),

            Err(
                err @ (RE::TooManyTypeNodes(_, _)
                | RE::TooManyTypeParams(_, _)
                | RE::TypeParamNesting(_, _)
                | RE::ValueNesting(_)),
            ) => return Err(resource_exhausted(err)),

            Err(err @ (RE::TypeArityMismatch(_, _) | RE::TypeParamOOB(_, _))) => {
                return Err(bad_user_input(Error::Resolve(err)))
            }

            Err(
                err @ (RE::Bcs(_)
                | RE::Store { .. }
                | RE::Deserialize(_)
                | RE::EmptyPackage(_)
                | RE::LinkageNotFound(_)
                | RE::NoTypeOrigin(_, _, _)
                | RE::UnexpectedReference
                | RE::UnexpectedSigner
                | RE::UnexpectedError(_)),
            ) => {
                return Err(anyhow!(err)
                    .context(format!(
                        "Error canonicalizing type {}",
                        input.to_canonical_display(/* with_prefix */ true)
                    ))
                    .into());
            }
        };

        Ok(Some(Self {
            native: canonical.into(),
            scope,
        }))
    }

    /// Construct a `MoveType` from a native `TypeTag`. Use this when surfacing a stored type i.e.
    /// not user input.
    pub(crate) fn from_native(tag: TypeTag, scope: Scope) -> Self {
        Self {
            native: tag.into(),
            scope,
        }
    }

    /// Construct a `MoveType` directly from a `TypeInput`. Use this when you already have a
    /// `TypeInput` (which is MoveType's internal representation) and don't want conversion to fail.
    pub(crate) fn from_input(input: TypeInput, scope: Scope) -> Self {
        Self {
            native: input,
            scope,
        }
    }

    /// Get the native `TypeTag` for this type, if it is valid.
    pub(crate) fn to_type_tag(&self) -> Option<TypeTag> {
        self.native.to_type_tag().ok()
    }

    /// Get the annotated type layout for this type, if it is valid.
    pub(crate) async fn layout_impl(&self) -> Result<Option<A::MoveTypeLayout>, RpcError> {
        let Some(tag) = self.to_type_tag() else {
            return Ok(None);
        };

        let layout = self
            .scope
            .package_resolver()
            .type_layout(tag)
            .await
            .map_err(|err| {
                internal_resolution_error(err, || {
                    format!(
                        "Error calculating layout for {}",
                        self.native.to_canonical_display(/* with_prefix */ true)
                    )
                })
            })?;

        Ok(Some(layout))
    }

    /// Get the abilities for this type, if it is valid.
    pub(crate) async fn abilities_impl(&self) -> Result<Option<AbilitySet>, RpcError> {
        let Some(tag) = self.to_type_tag() else {
            return Ok(None);
        };

        let set = self
            .scope
            .package_resolver()
            .abilities(tag)
            .await
            .map_err(|err| {
                internal_resolution_error(err, || {
                    format!(
                        "Error calculating abilities for {}",
                        self.native.to_canonical_display(/* with_prefix */ true)
                    )
                })
            })?;

        Ok(Some(set))
    }
}

impl TryFrom<TypeInput> for MoveTypeSignature {
    type Error = RpcError;

    fn try_from(tag: TypeInput) -> Result<Self, RpcError> {
        use TypeInput as T;

        Ok(match tag {
            T::Signer => return Err(anyhow!("Unexpected 'signer' type").into()),

            T::U8 => Self::U8,
            T::U16 => Self::U16,
            T::U32 => Self::U32,
            T::U64 => Self::U64,
            T::U128 => Self::U128,
            T::U256 => Self::U256,

            T::Bool => Self::Bool,
            T::Address => Self::Address,

            T::Vector(v) => Self::Vector(Box::new(Self::try_from(*v)?)),

            T::Struct(s) => Self::Datatype {
                package: s.address.to_canonical_string(/* with_prefix */ true),
                module: s.module,
                type_: s.name,
                type_parameters: s
                    .type_params
                    .into_iter()
                    .map(Self::try_from)
                    .collect::<Result<Vec<_>, _>>()?,
            },
        })
    }
}

impl TryFrom<A::MoveTypeLayout> for MoveTypeLayout {
    type Error = RpcError;

    fn try_from(layout: A::MoveTypeLayout) -> Result<Self, RpcError> {
        use A::MoveTypeLayout as TL;

        Ok(match layout {
            TL::Signer => return Err(anyhow!("Unexpected 'signer' type").into()),

            TL::U8 => Self::U8,
            TL::U16 => Self::U16,
            TL::U32 => Self::U32,
            TL::U64 => Self::U64,
            TL::U128 => Self::U128,
            TL::U256 => Self::U256,

            TL::Bool => Self::Bool,
            TL::Address => Self::Address,

            TL::Vector(v) => Self::Vector(Box::new(Self::try_from(*v)?)),
            TL::Struct(s) => Self::Struct((*s).try_into()?),
            TL::Enum(e) => Self::Enum((*e).try_into()?),
        })
    }
}

impl TryFrom<A::MoveEnumLayout> for MoveEnumLayout {
    type Error = RpcError;

    fn try_from(layout: A::MoveEnumLayout) -> Result<Self, RpcError> {
        let A::MoveEnumLayout { variants, .. } = layout;
        let mut variant_layouts = Vec::new();
        for ((name, _), variant_fields) in variants {
            let mut field_layouts = Vec::new();
            for field in variant_fields {
                field_layouts.push(MoveFieldLayout::try_from(field)?);
            }
            variant_layouts.push(MoveVariantLayout {
                name: name.to_string(),
                layout: field_layouts,
            });
        }

        Ok(MoveEnumLayout {
            variants: variant_layouts,
        })
    }
}

impl TryFrom<A::MoveStructLayout> for MoveStructLayout {
    type Error = RpcError;

    fn try_from(layout: A::MoveStructLayout) -> Result<Self, RpcError> {
        Ok(Self {
            type_: layout.type_.to_canonical_string(/* with_prefix */ true),
            fields: layout
                .fields
                .into_iter()
                .map(MoveFieldLayout::try_from)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl TryFrom<A::MoveFieldLayout> for MoveFieldLayout {
    type Error = RpcError;

    fn try_from(layout: A::MoveFieldLayout) -> Result<Self, RpcError> {
        Ok(Self {
            name: layout.name.to_string(),
            layout: layout.layout.try_into()?,
        })
    }
}

impl From<Ability> for MoveAbility {
    fn from(ability: Ability) -> Self {
        use Ability as A;
        use MoveAbility as M;

        match ability {
            A::Copy => M::Copy,
            A::Drop => M::Drop,
            A::Store => M::Store,
            A::Key => M::Key,
        }
    }
}

/// Convert an `AbilitySet` from the binary format into a vector of `MoveAbility` (a GraphQL type).
pub(crate) fn abilities(set: AbilitySet) -> Vec<MoveAbility> {
    set.into_iter().map(MoveAbility::from).collect()
}

/// Convert a package resolver error into an `RpcError` where the type involved has already been
/// vetted (it is a checked external input, or it is a type that comes from some stored value).
fn internal_resolution_error<F, C>(err: ResolverError, context: F) -> RpcError
where
    C: fmt::Display + Send + Sync + 'static,
    F: FnOnce() -> C,
{
    use ResolverError as RE;

    match &err {
        RE::TooManyTypeNodes(_, _)
        | RE::TooManyTypeParams(_, _)
        | RE::TypeParamNesting(_, _)
        | RE::ValueNesting(_) => resource_exhausted(err),

        RE::Bcs(_)
        | RE::Store { .. }
        | RE::Deserialize(_)
        | RE::EmptyPackage(_)
        | RE::FunctionNotFound(_, _, _)
        | RE::LinkageNotFound(_)
        | RE::ModuleNotFound(_, _)
        | RE::NoTypeOrigin(_, _, _)
        | RE::NotAPackage(_)
        | RE::NotAnIdentifier(_)
        | RE::PackageNotFound(_)
        | RE::DatatypeNotFound(_, _, _)
        | RE::TypeArityMismatch(_, _)
        | RE::TypeParamOOB(_, _)
        | RE::UnexpectedReference
        | RE::UnexpectedSigner
        | RE::UnexpectedError(_) => anyhow!(err).context(context()).into(),
    }
}
