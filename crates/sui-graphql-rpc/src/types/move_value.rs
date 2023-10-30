// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use move_core_types::{
    account_address::AccountAddress,
    ident_str,
    identifier::{IdentStr, Identifier},
    language_storage::{StructTag, TypeTag},
    value,
};
use serde::{Deserialize, Serialize};

use crate::{
    error::{code, graphql_error},
    types::move_type::unexpected_signer_error,
};

use super::{base64::Base64, big_int::BigInt, move_type::MoveType, sui_address::SuiAddress};

const STD: AccountAddress = AccountAddress::ONE;
const SUI: AccountAddress = AccountAddress::TWO;

const MOD_ASCII: &IdentStr = ident_str!("ascii");
const MOD_OBJECT: &IdentStr = ident_str!("object");
const MOD_OPTION: &IdentStr = ident_str!("option");
const MOD_STRING: &IdentStr = ident_str!("string");

const TYP_ID: &IdentStr = ident_str!("ID");
const TYP_OPTION: &IdentStr = ident_str!("Option");
const TYP_STRING: &IdentStr = ident_str!("String");
const TYP_UID: &IdentStr = ident_str!("UID");

#[derive(SimpleObject)]
#[graphql(complex)]
pub(crate) struct MoveValue {
    #[graphql(name = "type")]
    type_: MoveType,
    bcs: Base64,
}

scalar!(
    MoveData,
    "MoveData",
    "The contents of a Move Value, corresponding to the following recursive type:

type MoveData =
    { Address: SuiAddress }
  | { UID:     SuiAddress }
  | { Bool:    bool }
  | { Number:  BigInt }
  | { String:  string }
  | { Vector:  [MoveData] }
  | { Option:   MoveData? }
  | { Struct:  [{ name: string, value: MoveData }] }"
);

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum MoveData {
    Address(SuiAddress),
    #[serde(rename = "UID")]
    Uid(SuiAddress),
    Bool(bool),
    Number(BigInt),
    String(String),
    Vector(Vec<MoveData>),
    Option(Option<Box<MoveData>>),
    Struct(Vec<MoveField>),
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct MoveField {
    name: String,
    value: MoveData,
}

#[ComplexObject]
impl MoveValue {
    async fn data(&self, ctx: &Context<'_>) -> Result<MoveData> {
        let cache = ctx.data().map_err(|_| {
            graphql_error(
                code::INTERNAL_SERVER_ERROR,
                "Unable to fetch Package Cache.",
            )
        })?;

        // Factor out into its own non-GraphQL, non-async function for better testability
        self.data_impl(self.type_.layout_impl(cache).await?)
    }
}

impl MoveValue {
    pub fn new(repr: String, bcs: Base64) -> Self {
        let type_ = MoveType::new(repr);
        Self { type_, bcs }
    }

    fn data_impl(&self, layout: value::MoveTypeLayout) -> Result<MoveData> {
        // TODO: If this becomes a performance bottleneck, it can be made more efficient by not
        // deserializing via `value::MoveValue` (but this is significantly more code).
        let value: value::MoveValue =
            bcs::from_bytes_seed(&layout, &self.bcs.0[..]).map_err(|_| {
                let type_tag: Option<TypeTag> = (&layout).try_into().ok();
                let message = if let Some(type_tag) = type_tag {
                    format!("Failed to deserialize Move value for type: {}", type_tag)
                } else {
                    "Failed to deserialize Move value for type: <unknown>".to_string()
                };

                graphql_error(code::INTERNAL_SERVER_ERROR, message)
            })?;

        MoveData::try_from(value)
    }
}

impl TryFrom<value::MoveValue> for MoveData {
    type Error = async_graphql::Error;

    fn try_from(value: value::MoveValue) -> Result<Self> {
        use value::MoveValue as V;

        Ok(match value {
            V::U8(n) => Self::Number(BigInt::from(n)),
            V::U16(n) => Self::Number(BigInt::from(n)),
            V::U32(n) => Self::Number(BigInt::from(n)),
            V::U64(n) => Self::Number(BigInt::from(n)),
            V::U128(n) => Self::Number(BigInt::from(n)),
            V::U256(n) => Self::Number(BigInt::from(n)),

            V::Bool(b) => Self::Bool(b),
            V::Address(a) => Self::Address(a.into()),

            V::Vector(v) => Self::Vector(
                v.into_iter()
                    .map(MoveData::try_from)
                    .collect::<Result<Vec<_>>>()?,
            ),

            V::Struct(s) => {
                let (type_, fields) = with_type(s)?;
                if is_type(&type_, &STD, MOD_OPTION, TYP_OPTION) {
                    // 0x1::option::Option
                    Self::Option(extract_option(&type_, fields)?)
                } else if is_type(&type_, &STD, MOD_ASCII, TYP_STRING)
                    || is_type(&type_, &STD, MOD_STRING, TYP_STRING)
                {
                    // 0x1::ascii::String, 0x1::string::String
                    Self::String(extract_string(&type_, fields)?)
                } else if is_type(&type_, &SUI, MOD_OBJECT, TYP_UID) {
                    // 0x2::object::UID
                    Self::Uid(extract_uid(&type_, fields)?)
                } else {
                    // Arbitrary structs
                    let fields: Result<Vec<_>> =
                        fields.into_iter().map(MoveField::try_from).collect();
                    Self::Struct(fields?)
                }
            }

            // Sui does not support `signer` as a type.
            V::Signer(_) => return Err(unexpected_signer_error()),
        })
    }
}

impl TryFrom<(Identifier, value::MoveValue)> for MoveField {
    type Error = async_graphql::Error;

    fn try_from((ident, value): (Identifier, value::MoveValue)) -> Result<Self> {
        Ok(MoveField {
            name: ident.to_string(),
            value: MoveData::try_from(value)?,
        })
    }
}

fn is_type(tag: &StructTag, address: &AccountAddress, module: &IdentStr, name: &IdentStr) -> bool {
    &tag.address == address
        && tag.module.as_ident_str() == module
        && tag.name.as_ident_str() == name
}

fn with_type(
    struct_: value::MoveStruct,
) -> Result<(StructTag, Vec<(Identifier, value::MoveValue)>)> {
    if let value::MoveStruct::WithTypes { type_, fields } = struct_ {
        Ok((type_, fields))
    } else {
        Err(graphql_error(
            code::INTERNAL_SERVER_ERROR,
            "Move Struct without type information.",
        )
        .into())
    }
}

macro_rules! extract_field {
    ($type:expr, $fields:expr, $name:ident) => {{
        let _name = ident_str!(stringify!($name));
        let _type = $type;
        if let Some(value) = ($fields)
            .into_iter()
            .find_map(|(name, value)| (&*name == _name).then_some(value))
        {
            value
        } else {
            return Err(graphql_error(
                code::INTERNAL_SERVER_ERROR,
                format!("Couldn't find expected field '{_name}' of {_type}."),
            )
            .into());
        }
    }};
}

/// Extracts a vector of bytes from `value`, assuming it's a `MoveValue::Vector` where all the
/// values are `MoveValue::U8`s.
fn extract_bytes(value: value::MoveValue) -> Result<Vec<u8>> {
    use value::MoveValue as V;
    let V::Vector(elements) = value else {
        return Err(graphql_error(code::INTERNAL_SERVER_ERROR, "Expected a vector.").into());
    };

    let mut bytes = Vec::with_capacity(elements.len());
    for element in elements {
        let V::U8(byte) = element else {
            return Err(graphql_error(code::INTERNAL_SERVER_ERROR, "Expected a byte.").into());
        };
        bytes.push(byte)
    }

    Ok(bytes)
}

/// Extracts a Rust String from the contents of a Move Struct assuming that struct matches the
/// contents of Move String:
///
/// ```notrust
///     { bytes: vector<u8> }
/// ```
///
/// Which is conformed to by both `std::ascii::String` and `std::string::String`.
fn extract_string(
    type_: &StructTag,
    fields: Vec<(Identifier, value::MoveValue)>,
) -> Result<String> {
    let bytes = extract_bytes(extract_field!(type_, fields, bytes))?;
    String::from_utf8(bytes).map_err(|e| {
        const PREFIX: usize = 30;
        let bytes = e.as_bytes();

        // Provide a sample of the string in question.
        let sample = if bytes.len() < PREFIX {
            String::from_utf8_lossy(bytes)
        } else {
            String::from_utf8_lossy(&bytes[..PREFIX - 3]) + "..."
        };

        graphql_error(code::INTERNAL_SERVER_ERROR, format!("{e} in {sample:?}")).into()
    })
}

/// Extracts an address from the contents of a Move Struct, assuming the struct matches the
/// following shape:
///
/// ```notrust
///     { id: 0x2::object::ID { bytes: address } }
/// ```
///
/// Which matches `0x2::object::UID`.
fn extract_uid(
    type_: &StructTag,
    fields: Vec<(Identifier, value::MoveValue)>,
) -> Result<SuiAddress> {
    use value::MoveValue as V;
    let V::Struct(s) = extract_field!(type_, fields, id) else {
        return Err(graphql_error(
            code::INTERNAL_SERVER_ERROR,
            "Expected UID.id to be a struct",
        )
        .into());
    };

    let (type_, fields) = with_type(s)?;
    if !is_type(&type_, &SUI, MOD_OBJECT, TYP_ID) {
        return Err(graphql_error(
            code::INTERNAL_SERVER_ERROR,
            "Expected UID.id to have to type ID.",
        )
        .into());
    }

    let V::Address(addr) = extract_field!(type_, fields, bytes) else {
        return Err(graphql_error(
            code::INTERNAL_SERVER_ERROR,
            "Expected ID.bytes to have type address.",
        )
        .into());
    };

    Ok(addr.into())
}

/// Extracts a value from the contents of a Move Struct, assuming the struct matches the following
/// shape:
///
/// ```notrust
///     { vec: vector<T> }
/// ```
///
/// Where `vec` contains at most one element.  This matches the shape of `0x1::option::Option<T>`.
fn extract_option(
    type_: &StructTag,
    fields: Vec<(Identifier, value::MoveValue)>,
) -> Result<Option<Box<MoveData>>> {
    let value::MoveValue::Vector(mut elements) = extract_field!(type_, fields, vec) else {
        return Err(graphql_error(
            code::INTERNAL_SERVER_ERROR,
            "Expected Option.vec to be a vector.",
        )
        .into());
    };

    if elements.len() > 1 {
        return Err(graphql_error(
            code::INTERNAL_SERVER_ERROR,
            "Expected Option.vec to contain at most one element.",
        )
        .into());
    };

    Ok(match elements.pop() {
        Some(value) => Some(Box::new(MoveData::try_from(value)?)),
        None => None,
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use expect_test::expect;
    use move_core_types::{
        u256::U256, value::MoveFieldLayout, value::MoveStructLayout as S,
        value::MoveTypeLayout as L,
    };

    use super::*;

    macro_rules! struct_layout {
        ($type:literal { $($name:literal : $layout:expr),* $(,)?}) => {
            value::MoveTypeLayout::Struct(S::WithTypes {
                type_: StructTag::from_str($type).expect("Failed to parse struct"),
                fields: vec![$(MoveFieldLayout {
                    name: ident_str!($name).to_owned(),
                    layout: $layout,
                }),*]
            })
        }
    }

    macro_rules! vector_layout {
        ($inner:expr) => {
            value::MoveTypeLayout::Vector(Box::new($inner))
        };
    }

    fn address(a: &str) -> SuiAddress {
        SuiAddress::from_str(a).unwrap()
    }

    fn data<T: Serialize>(layout: value::MoveTypeLayout, data: T) -> Result<MoveData> {
        let tag: TypeTag = (&layout).try_into().expect("Error fetching type tag");

        // The format for type from its `Display` impl does not technically match the format that
        // the RPC expects from the data layer (where a type's package should be canonicalized), but
        // it will suffice.
        data_with_tag(format!("{}", tag), layout, data)
    }

    fn data_with_tag<T: Serialize>(
        tag: impl Into<String>,
        layout: value::MoveTypeLayout,
        data: T,
    ) -> Result<MoveData> {
        let type_ = MoveType::new(tag.into());
        let bcs = Base64(bcs::to_bytes(&data).unwrap());
        MoveValue { type_, bcs }.data_impl(layout)
    }

    #[test]
    fn bool_value() {
        let v = data(L::Bool, true);
        let expect = expect!["Ok(Bool(true))"];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn u8_value() {
        let v = data(L::U8, 42u8);
        let expect = expect![[r#"Ok(Number(BigInt("42")))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn u16_value() {
        let v = data(L::U16, 424u16);
        let expect = expect![[r#"Ok(Number(BigInt("424")))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn u32_value() {
        let v = data(L::U32, 424_242u32);
        let expect = expect![[r#"Ok(Number(BigInt("424242")))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn u64_value() {
        let v = data(L::U64, 42_424_242_424u64);
        let expect = expect![[r#"Ok(Number(BigInt("42424242424")))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn u128_value() {
        let v = data(L::U128, 424_242_424_242_424_242_424u128);
        let expect = expect![[r#"Ok(Number(BigInt("424242424242424242424")))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn u256_value() {
        let v = data(
            L::U256,
            U256::from_str("42424242424242424242424242424242424242424").unwrap(),
        );
        let expect =
            expect![[r#"Ok(Number(BigInt("42424242424242424242424242424242424242424")))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn ascii_string_value() {
        let l = struct_layout!("0x1::ascii::String" {
            "bytes": vector_layout!(L::U8)
        });

        let v = data(l, "The quick brown fox");
        let expect = expect![[r#"Ok(String("The quick brown fox"))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn utf8_string_value() {
        let l = struct_layout!("0x1::string::String" {
            "bytes": vector_layout!(L::U8)
        });

        let v = data(l, "jumped over the lazy dog.");
        let expect = expect![[r#"Ok(String("jumped over the lazy dog."))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn string_encoding_error() {
        let l = struct_layout!("0x1::string::String" {
            "bytes": vector_layout!(L::U8)
        });

        let mut bytes = "Lorem ipsum dolor sit amet consectetur".as_bytes().to_vec();
        bytes[5] = 0xff;

        let v = data(l, bytes);
        let expect = expect![[r#"
            Err(
                Error {
                    message: "invalid utf-8 sequence of 1 bytes from index 5 in \"Lorem�ipsum dolor sit amet ...\"",
                    extensions: None,
                },
            )"#]];
        expect.assert_eq(&format!("{v:#?}"));
    }

    #[test]
    fn address_value() {
        let v = data(L::Address, address("0x42"));
        let expect = expect!["Ok(Address(SuiAddress([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 66])))"];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn uid_value() {
        let l = struct_layout!("0x2::object::UID" {
            "id": struct_layout!("0x2::object::ID" {
                "bytes": L::Address,
            })
        });

        let v = data(l, address("0x42"));
        let expect = expect!["Ok(Uid(SuiAddress([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 66])))"];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn compound_data() {
        let l = struct_layout!("0x42::foo::Bar" {
            "baz": struct_layout!("0x1::option::Option" { "vec": vector_layout!(L::U8) }),
            "qux": vector_layout!(struct_layout!("0x43::xy::Zzy" {
                "quy": L::U16,
                "quz": struct_layout!("0x1::option::Option" {
                    "vec": vector_layout!(struct_layout!("0x1::ascii::String" {
                        "bytes": vector_layout!(L::U8),
                    }))
                }),
                "frob": L::Address,
            })),
        });

        let v = data(
            l,
            (
                vec![] as Vec<Vec<u8>>,
                vec![
                    (44u16, vec!["Hello, world!"], address("0x45")),
                    (46u16, vec![], address("0x47")),
                ],
            ),
        );

        let expect = expect![[r#"
            Ok(
                Struct(
                    [
                        MoveField {
                            name: "baz",
                            value: Option(
                                None,
                            ),
                        },
                        MoveField {
                            name: "qux",
                            value: Vector(
                                [
                                    Struct(
                                        [
                                            MoveField {
                                                name: "quy",
                                                value: Number(
                                                    BigInt(
                                                        "44",
                                                    ),
                                                ),
                                            },
                                            MoveField {
                                                name: "quz",
                                                value: Option(
                                                    Some(
                                                        String(
                                                            "Hello, world!",
                                                        ),
                                                    ),
                                                ),
                                            },
                                            MoveField {
                                                name: "frob",
                                                value: Address(
                                                    SuiAddress(
                                                        [
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            69,
                                                        ],
                                                    ),
                                                ),
                                            },
                                        ],
                                    ),
                                    Struct(
                                        [
                                            MoveField {
                                                name: "quy",
                                                value: Number(
                                                    BigInt(
                                                        "46",
                                                    ),
                                                ),
                                            },
                                            MoveField {
                                                name: "quz",
                                                value: Option(
                                                    None,
                                                ),
                                            },
                                            MoveField {
                                                name: "frob",
                                                value: Address(
                                                    SuiAddress(
                                                        [
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            0,
                                                            71,
                                                        ],
                                                    ),
                                                ),
                                            },
                                        ],
                                    ),
                                ],
                            ),
                        },
                    ],
                ),
            )"#]];
        expect.assert_eq(&format!("{v:#?}"));
    }

    #[test]
    fn no_type_information() {
        // This layout looks like a string, but we don't have the type information, so we can't say
        // for sure -- so we always require that move struct come `WithTypes`.
        let l = L::Struct(S::WithFields(vec![MoveFieldLayout {
            name: ident_str!("bytes").to_owned(),
            layout: vector_layout!(L::U8),
        }]));

        let v = data_with_tag("0x1::string::String", l, "Hello, world!");
        let expect = expect![[r#"
            Err(
                Error {
                    message: "Move Struct without type information.",
                    extensions: None,
                },
            )"#]];
        expect.assert_eq(&format!("{v:#?}"));
    }

    #[test]
    fn no_field_information() {
        // Even less information about the layout -- even less likely to succeed.
        let l = L::Struct(S::Runtime(vec![vector_layout!(L::U8)]));
        let v = data_with_tag("0x1::string::String", l, "Hello, world!");
        let expect = expect![[r#"
            Err(
                Error {
                    message: "Move Struct without type information.",
                    extensions: None,
                },
            )"#]];
        expect.assert_eq(&format!("{v:#?}"));
    }

    #[test]
    fn signer_value() {
        let v = data(L::Signer, address("0x42"));
        let expect = expect![[r#"
            Err(
                Error {
                    message: "Unexpected value of type: signer.",
                    extensions: None,
                },
            )"#]];
        expect.assert_eq(&format!("{v:#?}"));
    }

    #[test]
    fn signer_nested_value() {
        let v = data(
            vector_layout!(L::Signer),
            vec![address("0x42"), address("0x43")],
        );
        let expect = expect![[r#"
            Err(
                Error {
                    message: "Unexpected value of type: signer.",
                    extensions: None,
                },
            )"#]];
        expect.assert_eq(&format!("{v:#?}"));
    }
}
