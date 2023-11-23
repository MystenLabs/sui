// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use async_graphql::*;
use serde::{Deserialize, Serialize};
use sui_package_resolver::OpenSignatureBody;

/// Represents types that could contain references or free type parameters.  Such types can appear
/// as function parameters, in fields of structs, or as actual type parameter.
#[derive(SimpleObject)]
#[graphql(complex)]
pub(crate) struct OpenMoveType {
    /// Structured representation of the type signature.
    signature: OpenMoveTypeSignature,
}

scalar!(
    OpenMoveTypeSignature,
    "OpenMoveTypeSignature",
    "The shape of an abstract Move Type (a type that can contain free type parameters, and can \
     optionally be taken by reference), corresponding to the following recursive type:

type OpenMoveTypeSignature = {
  ref: (\"&\" | \"&mut\")?,
  body: OpenMoveTypeSignatureBody,
}

type OpenMoveTypeSignatureBody =
    \"address\"
  | \"bool\"
  | \"u8\" | \"u16\" | ... | \"u256\"
  | { vector: OpenMoveTypeSignatureBody }
  | {
      struct {
        package: string,
        module: string,
        type: string,
        typeParameters: [OpenMoveTypeSignatureBody]?
      }
    }
  | { TypeParameter: number }"
);

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct OpenMoveTypeSignature {
    #[serde(rename = "ref")]
    ref_: Option<OpenMoveTypeReference>,
    body: OpenMoveTypeSignatureBody,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum OpenMoveTypeReference {
    #[serde(rename = "&")]
    Immutable,

    #[serde(rename = "&mut")]
    Mutable,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) enum OpenMoveTypeSignatureBody {
    TypeParameter(u16),
    Address,
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Vector(Box<OpenMoveTypeSignatureBody>),
    Struct {
        package: String,
        module: String,
        #[serde(rename = "type")]
        type_: String,
        type_parameters: Vec<OpenMoveTypeSignatureBody>,
    },
}

#[ComplexObject]
impl OpenMoveType {
    /// Flat representation of the type signature, as a displayable string.
    async fn repr(&self) -> String {
        self.signature.to_string()
    }
}

impl From<OpenSignatureBody> for OpenMoveTypeSignature {
    fn from(signature: OpenSignatureBody) -> Self {
        OpenMoveTypeSignature {
            ref_: None,
            body: signature.into(),
        }
    }
}

impl From<OpenSignatureBody> for OpenMoveTypeSignatureBody {
    fn from(signature: OpenSignatureBody) -> Self {
        use OpenMoveTypeSignatureBody as OMTSB;
        use OpenSignatureBody as OSB;

        match signature {
            OSB::Address => OMTSB::Address,
            OSB::Bool => OMTSB::Bool,
            OSB::U8 => OMTSB::U8,
            OSB::U16 => OMTSB::U16,
            OSB::U32 => OMTSB::U32,
            OSB::U64 => OMTSB::U64,
            OSB::U128 => OMTSB::U128,
            OSB::U256 => OMTSB::U256,

            OSB::Vector(signature) => OMTSB::Vector(Box::new(OMTSB::from(*signature))),

            OSB::Struct(struct_, type_params) => OMTSB::Struct {
                package: struct_.package.to_canonical_string(/* with_prefix */ true),
                module: struct_.module.to_string(),
                type_: struct_.name.to_string(),
                type_parameters: type_params.into_iter().map(OMTSB::from).collect(),
            },

            OSB::TypeParameter(idx) => OMTSB::TypeParameter(idx),
        }
    }
}

impl fmt::Display for OpenMoveTypeSignature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use OpenMoveTypeReference as R;
        let OpenMoveTypeSignature { ref_, body } = self;

        if let Some(r) = ref_ {
            match r {
                R::Immutable => write!(f, "&")?,
                R::Mutable => write!(f, "&mut ")?,
            }
        }

        write!(f, "{body}")
    }
}

impl fmt::Display for OpenMoveTypeSignatureBody {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use OpenMoveTypeSignatureBody as B;

        match self {
            B::TypeParameter(idx) => write!(f, "${idx}"),

            B::Address => write!(f, "address"),
            B::Bool => write!(f, "bool"),
            B::U8 => write!(f, "u8"),
            B::U16 => write!(f, "u16"),
            B::U32 => write!(f, "u32"),
            B::U64 => write!(f, "u64"),
            B::U128 => write!(f, "u128"),
            B::U256 => write!(f, "u256"),
            B::Vector(sig) => write!(f, "vector<{sig}>"),

            B::Struct {
                package,
                module,
                type_,
                type_parameters,
            } => {
                write!(f, "{package}::{module}::{type_}")?;

                let mut params = type_parameters.iter();
                let Some(param) = params.next() else {
                    return Ok(());
                };

                write!(f, "<{param}")?;
                for param in params {
                    write!(f, ", {param}")?;
                }
                write!(f, ">")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    use expect_test::expect;
    use move_core_types::language_storage::StructTag;
    use sui_package_resolver::{StructKey, StructRef};

    use OpenSignatureBody as S;

    fn struct_key(s: &str) -> StructKey {
        StructRef::from(&StructTag::from_str(s).unwrap()).as_key()
    }

    #[test]
    fn generic_signature() {
        let signature = OpenMoveTypeSignature::from(S::Struct(
            struct_key("0x2::table::Table"),
            vec![S::TypeParameter(0), S::TypeParameter(1)],
        ));

        let expect = expect![[r#"
            OpenMoveTypeSignature {
                ref_: None,
                body: Struct {
                    package: "0x0000000000000000000000000000000000000000000000000000000000000002",
                    module: "table",
                    type_: "Table",
                    type_parameters: [
                        TypeParameter(
                            0,
                        ),
                        TypeParameter(
                            1,
                        ),
                    ],
                },
            }"#]];
        expect.assert_eq(&format!("{signature:#?}"));
    }

    #[test]
    fn instance_signature() {
        let signature = OpenMoveTypeSignature::from(S::Struct(
            struct_key("0x2::coin::Coin"),
            vec![S::Struct(struct_key("0x2::sui::SUI"), vec![])],
        ));

        let expect = expect![[r#"
            OpenMoveTypeSignature {
                ref_: None,
                body: Struct {
                    package: "0x0000000000000000000000000000000000000000000000000000000000000002",
                    module: "coin",
                    type_: "Coin",
                    type_parameters: [
                        Struct {
                            package: "0x0000000000000000000000000000000000000000000000000000000000000002",
                            module: "sui",
                            type_: "SUI",
                            type_parameters: [],
                        },
                    ],
                },
            }"#]];
        expect.assert_eq(&format!("{signature:#?}"));
    }

    #[test]
    fn generic_signature_repr() {
        let signature = OpenMoveTypeSignature::from(S::Struct(
            struct_key("0x2::table::Table"),
            vec![S::TypeParameter(0), S::TypeParameter(1)],
        ));

        let expect = expect!["0x0000000000000000000000000000000000000000000000000000000000000002::table::Table<$0, $1>"];
        expect.assert_eq(&format!("{signature}"));
    }

    #[test]
    fn instance_signature_repr() {
        let signature = OpenMoveTypeSignature::from(S::Struct(
            struct_key("0x2::coin::Coin"),
            vec![S::Struct(struct_key("0x2::sui::SUI"), vec![])],
        ));

        let expect = expect!["0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>"];
        expect.assert_eq(&format!("{signature}"));
    }
}
