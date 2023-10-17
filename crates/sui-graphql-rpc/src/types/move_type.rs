// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use async_graphql::*;
use move_core_types::language_storage::TypeTag;
use serde::{Deserialize, Serialize};

use crate::error::{code, graphql_error};

/// Represents concrete types (no type parameters, no references)
#[derive(SimpleObject)]
#[graphql(complex)]
pub(crate) struct MoveType {
    /// Flat representation of the type signature, as a displayable string.
    repr: String,
}

scalar!(
    MoveTypeSignature,
    "MoveTypeSignature",
    r#"The signature of a concrete Move Type (a type with all its type
parameters instantiated with concrete types, that contains no
references), corresponding to the following recursive type:

type MoveTypeSignature =
    "address"
  | "bool"
  | "u8" | "u16" | ... | "u256"
  | { vector: MoveTypeSignature }
  | {
      struct: {
        package: string,
        module: string,
        type: string,
        typeParameters: [MoveTypeSignature],
      }
    }"#
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
    Struct {
        package: String,
        module: String,
        #[serde(rename = "type")]
        type_: String,
        type_parameters: Vec<MoveTypeSignature>,
    },
}

#[ComplexObject]
impl MoveType {
    /// Structured representation of the type signature.
    async fn signature(&self) -> Result<MoveTypeSignature> {
        // Factor out into its own non-GraphQL, non-async function for better testability
        self.signature_impl()
    }
}

impl MoveType {
    pub(crate) fn new(repr: String) -> MoveType {
        Self { repr }
    }

    fn signature_impl(&self) -> Result<MoveTypeSignature> {
        let tag = TypeTag::from_str(&self.repr).map_err(|e| {
            graphql_error(
                code::INTERNAL_SERVER_ERROR,
                format!("Error parsing type '{}': {e}", self.repr),
            )
        })?;

        MoveTypeSignature::try_from(tag)
    }
}

impl TryFrom<TypeTag> for MoveTypeSignature {
    type Error = async_graphql::Error;

    fn try_from(tag: TypeTag) -> Result<Self> {
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

            T::Vector(v) => Self::Vector(Box::new(MoveTypeSignature::try_from(*v)?)),

            T::Struct(s) => Self::Struct {
                package: format!("0x{}", s.address.to_canonical_string()),
                module: s.module.to_string(),
                type_: s.name.to_string(),
                type_parameters: s
                    .type_params
                    .into_iter()
                    .map(MoveTypeSignature::try_from)
                    .collect::<Result<Vec<_>>>()?,
            },
        })
    }
}

/// Error from seeing a `signer` value or type, which shouldn't be possible in Sui Move.
pub(crate) fn unexpected_signer_error() -> Error {
    graphql_error(
        code::INTERNAL_SERVER_ERROR,
        "Unexpected value of type: signer.",
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    use expect_test::expect;

    fn signature(repr: impl Into<String>) -> Result<MoveTypeSignature> {
        MoveType::new(repr.into()).signature_impl()
    }

    #[test]
    fn complex_type() {
        let sig = signature("vector<0x42::foo::Bar<address, u32, bool, u256>>").unwrap();
        let expect = expect![[r#"
            Vector(
                Struct {
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
    fn tag_parse_error() {
        let err = signature("not_a_type").unwrap_err();
        let expect = expect![[
            r#"Error { message: "Error parsing type 'not_a_type': unexpected token Name(\"not_a_type\"), expected type tag", extensions: None }"#
        ]];
        expect.assert_eq(&format!("{err:?}"));
    }

    #[test]
    fn signer_type() {
        let err = signature("signer").unwrap_err();
        let expect = expect![[
            r#"Error { message: "Unexpected value of type: signer.", extensions: None }"#
        ]];
        expect.assert_eq(&format!("{err:?}"));
    }

    #[test]
    fn nested_signer_type() {
        let err = signature("0x42::baz::Qux<u32, vector<signer>>").unwrap_err();
        let expect = expect![[
            r#"Error { message: "Unexpected value of type: signer.", extensions: None }"#
        ]];
        expect.assert_eq(&format!("{err:?}"));
    }
}
