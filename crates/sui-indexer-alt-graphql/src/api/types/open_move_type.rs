// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use async_graphql::{scalar, Object};
use serde::{Deserialize, Serialize};
use sui_package_resolver::{OpenSignature, OpenSignatureBody, Reference};

pub(crate) struct OpenMoveType {
    signature: OpenMoveTypeSignature,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct OpenMoveTypeSignature {
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
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
    Datatype {
        package: String,
        module: String,
        #[serde(rename = "type")]
        type_: String,
        #[serde(rename = "typeParameters")]
        type_parameters: Vec<OpenMoveTypeSignatureBody>,
    },
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
      datatype {
        package: string,
        module: string,
        type: string,
        typeParameters: [OpenMoveTypeSignatureBody]
      }
    }
  | { typeParameter: number }"
);

/// Represents types that could contain references or free type parameters.  Such types can appear
/// as function parameters, in fields of structs, or as actual type parameter.
#[Object]
impl OpenMoveType {
    /// Structured representation of the type signature.
    async fn signature(&self) -> &OpenMoveTypeSignature {
        &self.signature
    }

    /// Flat representation of the type signature, as a displayable string.
    async fn repr(&self) -> String {
        self.signature.to_string()
    }
}

impl From<OpenSignature> for OpenMoveType {
    fn from(signature: OpenSignature) -> Self {
        OpenMoveType {
            signature: signature.into(),
        }
    }
}

impl From<OpenSignatureBody> for OpenMoveType {
    fn from(signature: OpenSignatureBody) -> Self {
        OpenMoveType {
            signature: signature.into(),
        }
    }
}

impl From<OpenSignature> for OpenMoveTypeSignature {
    fn from(signature: OpenSignature) -> Self {
        OpenMoveTypeSignature {
            ref_: signature.ref_.map(OpenMoveTypeReference::from),
            body: signature.body.into(),
        }
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

            OSB::Datatype(struct_, type_params) => OMTSB::Datatype {
                package: struct_.package.to_canonical_string(/* with_prefix */ true),
                module: struct_.module.to_string(),
                type_: struct_.name.to_string(),
                type_parameters: type_params.into_iter().map(OMTSB::from).collect(),
            },

            OSB::TypeParameter(idx) => OMTSB::TypeParameter(idx),
        }
    }
}

impl From<Reference> for OpenMoveTypeReference {
    fn from(ref_: Reference) -> Self {
        use OpenMoveTypeReference as M;
        use Reference as R;

        match ref_ {
            R::Immutable => M::Immutable,
            R::Mutable => M::Mutable,
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

            B::Datatype {
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

    use insta::assert_snapshot;
    use move_core_types::language_storage::StructTag;
    use sui_package_resolver::{DatatypeKey, DatatypeRef};

    use OpenSignatureBody as S;

    fn struct_key(s: &str) -> DatatypeKey {
        DatatypeRef::from(&StructTag::from_str(s).unwrap()).as_key()
    }

    #[test]
    fn generic_signature() {
        let signature = OpenMoveTypeSignature::from(S::Datatype(
            struct_key("0x2::table::Table"),
            vec![S::TypeParameter(0), S::TypeParameter(1)],
        ));

        assert_snapshot!(format!("{signature:#?}"), @r###"
        OpenMoveTypeSignature {
            ref_: None,
            body: Datatype {
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
        }
        "###);
    }

    #[test]
    fn instance_signature() {
        let signature = OpenMoveTypeSignature::from(S::Datatype(
            struct_key("0x2::coin::Coin"),
            vec![S::Datatype(struct_key("0x2::sui::SUI"), vec![])],
        ));

        assert_snapshot!(format!("{signature:#?}"), @r###"
        OpenMoveTypeSignature {
            ref_: None,
            body: Datatype {
                package: "0x0000000000000000000000000000000000000000000000000000000000000002",
                module: "coin",
                type_: "Coin",
                type_parameters: [
                    Datatype {
                        package: "0x0000000000000000000000000000000000000000000000000000000000000002",
                        module: "sui",
                        type_: "SUI",
                        type_parameters: [],
                    },
                ],
            },
        }
        "###);
    }

    #[test]
    fn generic_signature_repr() {
        let signature = OpenMoveTypeSignature::from(S::Datatype(
            struct_key("0x2::table::Table"),
            vec![S::TypeParameter(0), S::TypeParameter(1)],
        ));

        assert_snapshot!(format!("{signature}"), @"0x0000000000000000000000000000000000000000000000000000000000000002::table::Table<$0, $1>");
    }

    #[test]
    fn instance_signature_repr() {
        let signature = OpenMoveTypeSignature::from(S::Datatype(
            struct_key("0x2::coin::Coin"),
            vec![S::Datatype(struct_key("0x2::sui::SUI"), vec![])],
        ));

        assert_snapshot!(format!("{signature}"), @"0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>");
    }
}
