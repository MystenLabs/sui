// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use move_core_types::language_storage::TypeTag;
use serde::{Deserialize, Serialize};
use sui_types::{base_types::MoveObjectType, type_input::TypeInput};

use crate::error::RpcError;

/// Represents concrete types (no type parameters, no references).
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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

/// Represents concrete types (no type parameters, no references).
#[Object]
impl MoveType {
    /// Flat representation of the type signature, as a displayable string.
    async fn repr(&self) -> Option<String> {
        Some(self.native.to_canonical_string(/* with_prefix */ true))
    }

    /// Structured representation of the type signature.
    async fn signature(&self) -> Result<Option<MoveTypeSignature>, RpcError> {
        // Factor out into its own non-GraphQL, non-async function for better testability
        Ok(Some(self.signature_impl()?))
    }
}

impl MoveType {
    fn signature_impl(&self) -> Result<MoveTypeSignature, RpcError> {
        MoveTypeSignature::try_from(self.native.clone())
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
    type Error = RpcError;

    fn try_from(tag: TypeInput) -> Result<Self, Self::Error> {
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

/// Error from seeing a `signer` value or type, which shouldn't be possible in Sui Move.
pub(crate) fn unexpected_signer_error() -> RpcError {
    anyhow::anyhow!("Unexpected value of type: signer.").into()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_move_type_from_type_tag() {
        let tag = TypeTag::from_str("u64").unwrap();
        let move_type = MoveType::from(tag);
        assert_eq!(move_type.native.to_canonical_string(true), "u64");
    }

    #[test]
    fn test_move_type_from_type_input() {
        let input = TypeInput::U64;
        let move_type = MoveType::from(input);
        assert_eq!(move_type.native.to_canonical_string(true), "u64");
    }

    #[test]
    fn test_move_type_from_move_object_type() {
        use std::str::FromStr;

        // Create a MoveObjectType for 0x2::coin::Coin<0x2::sui::SUI>
        let sui_type_tag = TypeTag::from_str("0x2::sui::SUI").unwrap();
        let move_object_type = MoveObjectType::coin(sui_type_tag);

        let move_type = MoveType::from(move_object_type.clone());

        let canonical = move_type.native.to_canonical_string(true);
        assert_eq!(canonical, "0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>");
    }

    #[test]
    fn test_complex_type() {
        let tag = TypeTag::from_str("vector<0x42::foo::Bar<address, u32>>").unwrap();
        let move_type = MoveType::from(tag);
        assert_eq!(move_type.native.to_canonical_string(true), "vector<0x0000000000000000000000000000000000000000000000000000000000000042::foo::Bar<address,u32>>");
    }

    #[test]
    fn test_signature_primitive_types() {
        let u64_type = MoveType::from(TypeInput::U64);
        let sig = u64_type.signature_impl().unwrap();
        assert_eq!(sig, MoveTypeSignature::U64);

        let bool_type = MoveType::from(TypeInput::Bool);
        let sig = bool_type.signature_impl().unwrap();
        assert_eq!(sig, MoveTypeSignature::Bool);

        let address_type = MoveType::from(TypeInput::Address);
        let sig = address_type.signature_impl().unwrap();
        assert_eq!(sig, MoveTypeSignature::Address);
    }

    #[test]
    fn test_signature_vector_type() {
        let tag = TypeTag::from_str("vector<u64>").unwrap();
        let move_type = MoveType::from(tag);
        let sig = move_type.signature_impl().unwrap();

        match sig {
            MoveTypeSignature::Vector(inner) => {
                assert_eq!(*inner, MoveTypeSignature::U64);
            }
            _ => panic!("Expected Vector type signature"),
        }
    }

    #[test]
    fn test_signature_struct_type() {
        let tag = TypeTag::from_str("0x2::coin::Coin<0x2::sui::SUI>").unwrap();
        let move_type = MoveType::from(tag);
        let sig = move_type.signature_impl().unwrap();

        match sig {
            MoveTypeSignature::Datatype {
                package,
                module,
                type_,
                type_parameters,
            } => {
                assert!(package
                    .contains("0000000000000000000000000000000000000000000000000000000000000002"));
                assert_eq!(module, "coin");
                assert_eq!(type_, "Coin");
                assert_eq!(type_parameters.len(), 1);

                match &type_parameters[0] {
                    MoveTypeSignature::Datatype {
                        module: inner_module,
                        type_: inner_type,
                        ..
                    } => {
                        assert_eq!(inner_module, "sui");
                        assert_eq!(inner_type, "SUI");
                    }
                    _ => panic!("Expected nested Datatype"),
                }
            }
            _ => panic!("Expected Datatype signature"),
        }
    }

    #[test]
    fn test_signature_nested_generic_type() {
        let tag = TypeTag::from_str("vector<0x42::foo::Bar<address, u32>>").unwrap();
        let move_type = MoveType::from(tag);
        let sig = move_type.signature_impl().unwrap();

        match sig {
            MoveTypeSignature::Vector(inner) => match *inner {
                MoveTypeSignature::Datatype {
                    package,
                    module,
                    type_,
                    type_parameters,
                } => {
                    assert_eq!(
                        package,
                        "0x0000000000000000000000000000000000000000000000000000000000000042"
                    );
                    assert_eq!(module, "foo");
                    assert_eq!(type_, "Bar");
                    assert_eq!(type_parameters.len(), 2);
                    assert_eq!(type_parameters[0], MoveTypeSignature::Address);
                    assert_eq!(type_parameters[1], MoveTypeSignature::U32);
                }
                _ => panic!("Expected Datatype inside Vector"),
            },
            _ => panic!("Expected Vector signature"),
        }
    }

    #[test]
    fn test_signature_signer_error() {
        let tag = TypeTag::from_str("signer").unwrap();
        let move_type = MoveType::from(tag);
        let result = move_type.signature_impl();
        assert!(matches!(result, Err(RpcError::InternalError(_))));
    }
}
