// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::package_cache::PackageCache;
use async_graphql::*;
use move_binary_format::file_format::AbilitySet;
use move_core_types::{annotated_value as A, language_storage::TypeTag};
use serde::{Deserialize, Serialize};
use sui_package_resolver::Resolver;

use crate::error::Error;

use super::open_move_type::MoveAbility;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MoveType {
    native: TypeTag,
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
}

#[derive(Serialize, Deserialize)]
pub(crate) struct MoveStructLayout {
    #[serde(rename = "type")]
    type_: String,
    fields: Vec<MoveFieldLayout>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct MoveFieldLayout {
    name: String,
    layout: MoveTypeLayout,
}

/// Represents concrete types (no type parameters, no references)
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

    /// Structured representation of the "shape" of values that match this type.
    async fn layout(&self, ctx: &Context<'_>) -> Result<MoveTypeLayout> {
        let resolver: &Resolver<PackageCache> = ctx
            .data()
            .map_err(|_| Error::Internal("Unable to fetch Package Cache.".to_string()))
            .extend()?;

        MoveTypeLayout::try_from(self.layout_impl(resolver).await.extend()?).extend()
    }

    /// The abilities this concrete type has.
    async fn abilities(&self, ctx: &Context<'_>) -> Result<Vec<MoveAbility>> {
        let resolver: &Resolver<PackageCache> = ctx
            .data()
            .map_err(|_| Error::Internal("Unable to fetch Package Cache.".to_string()))
            .extend()?;

        Ok(self
            .abilities_impl(resolver)
            .await
            .extend()?
            .into_iter()
            .map(MoveAbility::from)
            .collect())
    }
}

impl MoveType {
    pub(crate) fn new(native: TypeTag) -> MoveType {
        Self { native }
    }

    fn signature_impl(&self) -> Result<MoveTypeSignature, Error> {
        MoveTypeSignature::try_from(self.native.clone())
    }

    pub(crate) async fn layout_impl(
        &self,
        resolver: &Resolver<PackageCache>,
    ) -> Result<A::MoveTypeLayout, Error> {
        resolver
            .type_layout(self.native.clone())
            .await
            .map_err(|e| {
                Error::Internal(format!(
                    "Error calculating layout for {}: {e}",
                    self.native.to_canonical_display(/* with_prefix */ true),
                ))
            })
    }

    pub(crate) async fn abilities_impl(
        &self,
        resolver: &Resolver<PackageCache>,
    ) -> Result<AbilitySet, Error> {
        resolver.abilities(self.native.clone()).await.map_err(|e| {
            Error::Internal(format!(
                "Error calculating abilities for {}: {e}",
                self.native.to_canonical_string(/* with_prefix */ true),
            ))
        })
    }
}

impl TryFrom<TypeTag> for MoveTypeSignature {
    type Error = Error;

    fn try_from(tag: TypeTag) -> Result<Self, Error> {
        use TypeTag as T;

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
                module: s.module.to_string(),
                type_: s.name.to_string(),
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
            TL::Struct(s) => Self::Struct(s.try_into()?),
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
        MoveType::new(tag).signature_impl()
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
