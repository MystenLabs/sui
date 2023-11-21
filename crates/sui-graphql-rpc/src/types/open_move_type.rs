// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use async_graphql::*;
use move_binary_format::{
    access::ModuleAccess,
    file_format::{SignatureToken, StructHandleIndex},
    CompiledModule,
};
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::types::move_type::unexpected_signer_error;

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

impl OpenMoveTypeSignature {
    pub(crate) fn read(
        signature: SignatureToken,
        bytecode: &CompiledModule,
    ) -> Result<Self, Error> {
        use OpenMoveTypeReference as R;
        use SignatureToken as S;

        Ok(match signature {
            S::Reference(signature) => OpenMoveTypeSignature {
                ref_: Some(R::Immutable),
                body: OpenMoveTypeSignatureBody::read(*signature, bytecode)?,
            },

            S::MutableReference(signature) => OpenMoveTypeSignature {
                ref_: Some(R::Mutable),
                body: OpenMoveTypeSignatureBody::read(*signature, bytecode)?,
            },

            signature => OpenMoveTypeSignature {
                ref_: None,
                body: OpenMoveTypeSignatureBody::read(signature, bytecode)?,
            },
        })
    }
}

impl OpenMoveTypeSignatureBody {
    fn read(signature: SignatureToken, bytecode: &CompiledModule) -> Result<Self, Error> {
        use OpenMoveTypeSignatureBody as B;
        use SignatureToken as S;

        Ok(match signature {
            S::Reference(_) | S::MutableReference(_) => return Err(unexpected_reference_error()),
            S::Signer => return Err(unexpected_signer_error()),

            S::TypeParameter(idx) => B::TypeParameter(idx),

            S::Bool => B::Bool,
            S::U8 => B::U8,
            S::U16 => B::U16,
            S::U32 => B::U32,
            S::U64 => B::U64,
            S::U128 => B::U128,
            S::U256 => B::U256,
            S::Address => B::Address,

            S::Vector(signature) => B::Vector(Box::new(OpenMoveTypeSignatureBody::read(
                *signature, bytecode,
            )?)),

            S::Struct(struct_) => {
                let (package, module, type_) = read_struct(struct_, bytecode);

                B::Struct {
                    package,
                    module,
                    type_,
                    type_parameters: vec![],
                }
            }

            S::StructInstantiation(struct_, type_parameters) => {
                let (package, module, type_) = read_struct(struct_, bytecode);

                let type_parameters = type_parameters
                    .into_iter()
                    .map(|signature| OpenMoveTypeSignatureBody::read(signature, bytecode))
                    .collect::<Result<Vec<_>, _>>()?;

                B::Struct {
                    package,
                    module,
                    type_,
                    type_parameters,
                }
            }
        })
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

/// Read the package, module and name of the struct at index `idx` of `bytecode`'s `StructHandle`
/// table.
fn read_struct(idx: StructHandleIndex, bytecode: &CompiledModule) -> (String, String, String) {
    let struct_handle = bytecode.struct_handle_at(idx);
    let module_handle = bytecode.module_handle_at(struct_handle.module);

    let package = bytecode
        .address_identifier_at(module_handle.address)
        .to_canonical_string(/* with_prefix */ true);

    let module = bytecode.identifier_at(module_handle.name).to_string();

    let type_ = bytecode.identifier_at(struct_handle.name).to_string();

    (package, module, type_)
}

/// Error from seeing a reference or mutable reference in the interior of a type signature.
fn unexpected_reference_error() -> Error {
    Error::Internal("Unexpected reference in signature.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    use expect_test::expect;
    use move_core_types::account_address::AccountAddress;
    use sui_framework::BuiltInFramework;
    use sui_types::base_types::ObjectID;

    /// Get the signature token for the first return of a function in the framework.
    fn param(
        package: AccountAddress,
        module: &str,
        func: &str,
        idx: usize,
    ) -> (SignatureToken, CompiledModule) {
        let framework = BuiltInFramework::get_package_by_id(&ObjectID::from(package));
        let bytecode = framework
            .modules()
            .into_iter()
            .find(|bytecode| bytecode.name().as_str() == module)
            .unwrap();

        let function_handle = bytecode
            .function_handles()
            .iter()
            .find(|handle| bytecode.identifier_at(handle.name).as_str() == func)
            .unwrap();

        let parameters = &bytecode.signature_at(function_handle.parameters).0;

        (parameters[idx].clone(), bytecode)
    }

    #[test]
    fn generic_signature() {
        let (st, bc) = param(AccountAddress::TWO, "table", "borrow_mut", 0);
        let signature = OpenMoveTypeSignature::read(st, &bc).unwrap();

        let expect = expect![[r#"
            OpenMoveTypeSignature {
                ref_: Some(
                    Mutable,
                ),
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
        let (st, bc) = param(AccountAddress::TWO, "sui", "transfer", 0);
        let signature = OpenMoveTypeSignature::read(st, &bc).unwrap();

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
        let (st, bc) = param(AccountAddress::TWO, "table", "borrow_mut", 0);
        let signature = OpenMoveTypeSignature::read(st, &bc).unwrap();

        let expect = expect!["&mut 0x0000000000000000000000000000000000000000000000000000000000000002::table::Table<$0, $1>"];
        expect.assert_eq(&format!("{signature}"));
    }

    #[test]
    fn instance_signature_repr() {
        let (st, bc) = param(AccountAddress::TWO, "sui", "transfer", 0);
        let signature = OpenMoveTypeSignature::read(st, &bc).unwrap();

        let expect = expect!["0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>"];
        expect.assert_eq(&format!("{signature}"));
    }

    #[test]
    fn signer_type() {
        let (_, bc) = param(AccountAddress::TWO, "transfer", "transfer", 0);
        let error = OpenMoveTypeSignature::read(SignatureToken::Signer, &bc).unwrap_err();

        let expect = expect![
            "Internal error occurred while processing request: Unexpected value of type: signer."
        ];
        expect.assert_eq(&error.to_string());
    }

    #[test]
    fn nested_signer_type() {
        let (_, bc) = param(AccountAddress::TWO, "transfer", "transfer", 0);
        let error = OpenMoveTypeSignature::read(
            SignatureToken::Vector(Box::new(SignatureToken::Signer)),
            &bc,
        )
        .unwrap_err();

        let expect = expect![
            "Internal error occurred while processing request: Unexpected value of type: signer."
        ];
        expect.assert_eq(&error.to_string());
    }

    #[test]
    fn nested_reference() {
        let (st, bc) = param(AccountAddress::TWO, "table", "borrow_mut", 0);
        let error =
            OpenMoveTypeSignature::read(SignatureToken::Vector(Box::new(st)), &bc).unwrap_err();

        let expect = expect![
            "Internal error occurred while processing request: Unexpected reference in signature."
        ];
        expect.assert_eq(&error.to_string());
    }
}
