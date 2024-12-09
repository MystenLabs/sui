// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use move_binary_format::file_format::AbilitySet;
use move_core_types::{annotated_value as A, language_storage::TypeTag};
use serde::{Deserialize, Serialize};
use sui_types::base_types::MoveObjectType;
use sui_types::type_input::TypeInput;

use crate::data::package_resolver::PackageResolver;
use crate::error::Error;

use super::open_move_type::MoveAbility;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MoveType {
    pub native: TypeInput,
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

/// Represents concrete types (no type parameters, no references).
#[Object]
impl MoveType {
    /// Flat representation of the type signature, as a displayable string.
    async fn repr(&self) -> String {
        self.native.to_canonical_string(/* with_prefix */ true)
    }

    /// Structured representation of the type signature.
    async fn signature(&self) -> Result<MoveTypeSignature> {
        // Factor out into its own non-GraphQL, non-async function for better testability
        self.signature_impl().extend()
    }

    /// Structured representation of the "shape" of values that match this type. May return no
    /// layout if the type is invalid.
    async fn layout(&self, ctx: &Context<'_>) -> Result<Option<MoveTypeLayout>> {
        let resolver: &PackageResolver = ctx
            .data()
            .map_err(|_| Error::Internal("Unable to fetch Package Cache.".to_string()))
            .extend()?;

        let Some(layout) = self.layout_impl(resolver).await.extend()? else {
            return Ok(None);
        };

        Ok(Some(MoveTypeLayout::try_from(layout).extend()?))
    }

    /// The abilities this concrete type has. Returns no abilities if the type is invalid.
    async fn abilities(&self, ctx: &Context<'_>) -> Result<Option<Vec<MoveAbility>>> {
        let resolver: &PackageResolver = ctx
            .data()
            .map_err(|_| Error::Internal("Unable to fetch Package Cache.".to_string()))
            .extend()?;

        let Some(abilities) = self.abilities_impl(resolver).await.extend()? else {
            return Ok(None);
        };

        Ok(Some(abilities.into_iter().map(MoveAbility::from).collect()))
    }
}

impl MoveType {
    fn signature_impl(&self) -> Result<MoveTypeSignature, Error> {
        MoveTypeSignature::try_from(self.native.clone())
    }

    pub(crate) async fn layout_impl(
        &self,
        resolver: &PackageResolver,
    ) -> Result<Option<A::MoveTypeLayout>, Error> {
        let Ok(tag) = self.native.as_type_tag() else {
            return Ok(None);
        };

        Ok(Some(resolver.type_layout(tag).await.map_err(|e| {
            Error::Internal(format!(
                "Error calculating layout for {}: {e}",
                self.native.to_canonical_display(/* with_prefix */ true),
            ))
        })?))
    }

    pub(crate) async fn abilities_impl(
        &self,
        resolver: &PackageResolver,
    ) -> Result<Option<AbilitySet>, Error> {
        let Ok(tag) = self.native.as_type_tag() else {
            return Ok(None);
        };

        Ok(Some(resolver.abilities(tag).await.map_err(|e| {
            Error::Internal(format!(
                "Error calculating abilities for {}: {e}",
                self.native.to_canonical_string(/* with_prefix */ true),
            ))
        })?))
    }
}

impl From<MoveObjectType> for MoveType {
    fn from(obj: MoveObjectType) -> Self {
        let tag: TypeTag = obj.into();
        Self { native: tag.into() }
    }
}

impl From<TypeTag> for MoveType {
    fn from(tag: TypeTag) -> Self {
        Self { native: tag.into() }
    }
}

impl From<TypeInput> for MoveType {
    fn from(native: TypeInput) -> Self {
        Self { native }
    }
}

impl TryFrom<TypeInput> for MoveTypeSignature {
    type Error = Error;

    fn try_from(tag: TypeInput) -> Result<Self, Error> {
        use TypeInput as T;

        Ok(match tag {
            T::Signer => return Err(unexpected_signer_error()),

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
    type Error = Error;

    fn try_from(layout: A::MoveTypeLayout) -> Result<Self, Error> {
        use A::MoveTypeLayout as TL;

        Ok(match layout {
            TL::Signer => return Err(unexpected_signer_error()),

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
    type Error = Error;

    fn try_from(layout: A::MoveEnumLayout) -> Result<Self, Error> {
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
    type Error = Error;

    fn try_from(layout: A::MoveStructLayout) -> Result<Self, Error> {
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
    type Error = Error;

    fn try_from(layout: A::MoveFieldLayout) -> Result<Self, Error> {
        Ok(Self {
            name: layout.name.to_string(),
            layout: layout.layout.try_into()?,
        })
    }
}

/// Error from seeing a `signer` value or type, which shouldn't be possible in Sui Move.
pub(crate) fn unexpected_signer_error() -> Error {
    Error::Internal("Unexpected value of type: signer.".to_string())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    use expect_test::expect;

    fn signature(repr: impl Into<String>) -> Result<MoveTypeSignature, Error> {
        let tag = TypeTag::from_str(repr.into().as_str()).unwrap();
        MoveType::from(tag).signature_impl()
    }

    #[test]
    fn complex_type() {
        let sig = signature("vector<0x42::foo::Bar<address, u32, bool, u256>>").unwrap();
        let expect = expect![[r#"
            Vector(
                Datatype {
                    package: "0x0000000000000000000000000000000000000000000000000000000000000042",
                    module: "foo",
                    type_: "Bar",
                    type_parameters: [
                        Address,
                        U32,
                        Bool,
                        U256,
                    ],
                },
            )"#]];
        expect.assert_eq(&format!("{sig:#?}"));
    }

    #[test]
    fn signer_type() {
        let err = signature("signer").unwrap_err();
        let expect = expect![[r#"Internal("Unexpected value of type: signer.")"#]];
        expect.assert_eq(&format!("{err:?}"));
    }

    #[test]
    fn nested_signer_type() {
        let err = signature("0x42::baz::Qux<u32, vector<signer>>").unwrap_err();
        let expect = expect![[r#"Internal("Unexpected value of type: signer.")"#]];
        expect.assert_eq(&format!("{err:?}"));
    }
}
