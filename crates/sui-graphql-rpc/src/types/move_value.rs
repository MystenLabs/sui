// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use move_core_types::{
    account_address::AccountAddress,
    annotated_value as A, ident_str,
    identifier::{IdentStr, Identifier},
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};
use sui_types::object::bounded_visitor::BoundedVisitor;

use crate::data::package_resolver::PackageResolver;
use crate::{error::Error, types::json::Json, types::move_type::unexpected_signer_error};

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
    /// The value's Move type.
    #[graphql(name = "type")]
    type_: MoveType,
    /// The BCS representation of this value, Base64 encoded.
    bcs: Base64,
}

scalar!(
    MoveData,
    "MoveData",
    "The contents of a Move Value, corresponding to the following recursive type:

type MoveData =
    { Address: SuiAddress }
  | { UID:     SuiAddress }
  | { ID:      SuiAddress }
  | { Bool:    bool }
  | { Number:  BigInt }
  | { String:  string }
  | { Vector:  [MoveData] }
  | { Option:   MoveData? }
  | { Struct:  [{ name: string , value: MoveData }] }
  | { Variant: {
      name: string,
      fields: [{ name: string, value: MoveData }],
  }"
);

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum MoveData {
    Address(SuiAddress),
    #[serde(rename = "UID")]
    Uid(SuiAddress),
    #[serde(rename = "ID")]
    Id(SuiAddress),
    Bool(bool),
    Number(BigInt),
    String(String),
    Vector(Vec<MoveData>),
    Option(Option<Box<MoveData>>),
    Struct(Vec<MoveField>),
    Variant(MoveVariant),
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct MoveVariant {
    name: String,
    fields: Vec<MoveField>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct MoveField {
    name: String,
    value: MoveData,
}

/// An instance of a Move type.
#[ComplexObject]
impl MoveValue {
    /// Structured contents of a Move value.
    async fn data(&self, ctx: &Context<'_>) -> Result<MoveData> {
        let resolver: &PackageResolver = ctx
            .data()
            .map_err(|_| Error::Internal("Unable to fetch Package Cache.".to_string()))
            .extend()?;

        let Some(layout) = self.type_.layout_impl(resolver).await.extend()? else {
            return Err(Error::Internal(
                "Move value must have valid layout".to_string(),
            ))
            .extend();
        };

        // Factor out into its own non-GraphQL, non-async function for better testability
        self.data_impl(layout).extend()
    }

    /// Representation of a Move value in JSON, where:
    ///
    /// - Addresses, IDs, and UIDs are represented in canonical form, as JSON strings.
    /// - Bools are represented by JSON boolean literals.
    /// - u8, u16, and u32 are represented as JSON numbers.
    /// - u64, u128, and u256 are represented as JSON strings.
    /// - Vectors are represented by JSON arrays.
    /// - Structs are represented by JSON objects.
    /// - Empty optional values are represented by `null`.
    ///
    /// This form is offered as a less verbose convenience in cases where the layout of the type is
    /// known by the client.
    async fn json(&self, ctx: &Context<'_>) -> Result<Json> {
        let resolver: &PackageResolver = ctx
            .data()
            .map_err(|_| Error::Internal("Unable to fetch Package Cache.".to_string()))
            .extend()?;

        let Some(layout) = self.type_.layout_impl(resolver).await.extend()? else {
            return Err(Error::Internal(
                "Move value must have valid layout".to_string(),
            ))
            .extend();
        };

        // Factor out into its own non-GraphQL, non-async function for better testability
        self.json_impl(layout).extend()
    }
}

impl MoveValue {
    pub fn new(tag: TypeTag, bcs: Base64) -> Self {
        let type_ = MoveType::from(tag);
        Self { type_, bcs }
    }

    fn value_impl(&self, layout: A::MoveTypeLayout) -> Result<A::MoveValue, Error> {
        // TODO (annotated-visitor): deserializing directly using a custom visitor.
        BoundedVisitor::deserialize_value(&self.bcs.0[..], &layout).map_err(|_| {
            let type_tag: TypeTag = (&layout).into();
            Error::Internal(format!(
                "Failed to deserialize Move value for type: {}",
                type_tag
            ))
        })
    }

    fn data_impl(&self, layout: A::MoveTypeLayout) -> Result<MoveData, Error> {
        MoveData::try_from(self.value_impl(layout)?)
    }

    fn json_impl(&self, layout: A::MoveTypeLayout) -> Result<Json, Error> {
        Ok(try_to_json_value(self.value_impl(layout)?)?.into())
    }
}

impl TryFrom<A::MoveValue> for MoveData {
    type Error = Error;

    fn try_from(value: A::MoveValue) -> Result<Self, Error> {
        use A::MoveValue as V;

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
                    .collect::<Result<Vec<_>, _>>()?,
            ),

            V::Struct(s) => {
                let A::MoveStruct { type_, fields } = s;
                if is_type(&type_, &STD, MOD_OPTION, TYP_OPTION) {
                    // 0x1::option::Option
                    Self::Option(match extract_option(&type_, fields)? {
                        Some(value) => Some(Box::new(MoveData::try_from(value)?)),
                        None => None,
                    })
                } else if is_type(&type_, &STD, MOD_ASCII, TYP_STRING)
                    || is_type(&type_, &STD, MOD_STRING, TYP_STRING)
                {
                    // 0x1::ascii::String, 0x1::string::String
                    Self::String(extract_string(&type_, fields)?)
                } else if is_type(&type_, &SUI, MOD_OBJECT, TYP_UID) {
                    // 0x2::object::UID
                    Self::Uid(extract_uid(&type_, fields)?.into())
                } else if is_type(&type_, &SUI, MOD_OBJECT, TYP_ID) {
                    // 0x2::object::ID
                    Self::Id(extract_id(&type_, fields)?.into())
                } else {
                    // Arbitrary structs
                    let fields: Result<Vec<_>, _> =
                        fields.into_iter().map(MoveField::try_from).collect();
                    Self::Struct(fields?)
                }
            }

            V::Variant(A::MoveVariant {
                type_: _,
                variant_name,
                tag: _,
                fields,
            }) => {
                let fields = fields
                    .into_iter()
                    .map(MoveField::try_from)
                    .collect::<Result<_, _>>()?;
                Self::Variant(MoveVariant {
                    name: variant_name.to_string(),
                    fields,
                })
            }

            // Sui does not support `signer` as a type.
            V::Signer(_) => return Err(unexpected_signer_error()),
        })
    }
}

impl TryFrom<(Identifier, A::MoveValue)> for MoveField {
    type Error = Error;

    fn try_from((ident, value): (Identifier, A::MoveValue)) -> Result<Self, Error> {
        Ok(MoveField {
            name: ident.to_string(),
            value: MoveData::try_from(value)?,
        })
    }
}

fn try_to_json_value(value: A::MoveValue) -> Result<Value, Error> {
    use A::MoveValue as V;
    Ok(match value {
        V::U8(n) => Value::Number(n.into()),
        V::U16(n) => Value::Number(n.into()),
        V::U32(n) => Value::Number(n.into()),
        V::U64(n) => Value::String(n.to_string()),
        V::U128(n) => Value::String(n.to_string()),
        V::U256(n) => Value::String(n.to_string()),

        V::Bool(b) => Value::Boolean(b),
        V::Address(a) => Value::String(a.to_canonical_string(/* with_prefix */ true)),

        V::Vector(xs) => Value::List(
            xs.into_iter()
                .map(try_to_json_value)
                .collect::<Result<_, _>>()?,
        ),

        V::Struct(s) => {
            let A::MoveStruct { type_, fields } = s;
            if is_type(&type_, &STD, MOD_OPTION, TYP_OPTION) {
                // 0x1::option::Option
                match extract_option(&type_, fields)? {
                    Some(value) => try_to_json_value(value)?,
                    None => Value::Null,
                }
            } else if is_type(&type_, &STD, MOD_ASCII, TYP_STRING)
                || is_type(&type_, &STD, MOD_STRING, TYP_STRING)
            {
                // 0x1::ascii::String, 0x1::string::String
                Value::String(extract_string(&type_, fields)?)
            } else if is_type(&type_, &SUI, MOD_OBJECT, TYP_UID) {
                // 0x2::object::UID
                Value::String(
                    extract_uid(&type_, fields)?.to_canonical_string(/* with_prefix */ true),
                )
            } else if is_type(&type_, &SUI, MOD_OBJECT, TYP_ID) {
                // 0x2::object::ID
                Value::String(
                    extract_id(&type_, fields)?.to_canonical_string(/* with_prefix */ true),
                )
            } else {
                // Arbitrary structs
                Value::Object(
                    fields
                        .into_iter()
                        .map(|(name, value)| {
                            Ok((Name::new(name.to_string()), try_to_json_value(value)?))
                        })
                        .collect::<Result<_, Error>>()?,
                )
            }
        }

        V::Variant(A::MoveVariant {
            type_: _,
            variant_name,
            tag: _,
            fields,
        }) => {
            let fields = fields
                .into_iter()
                .map(|(name, value)| Ok((Name::new(name.to_string()), try_to_json_value(value)?)))
                .collect::<Result<_, Error>>()?;
            Value::Object(
                vec![(Name::new(variant_name.to_string()), Value::Object(fields))]
                    .into_iter()
                    .collect(),
            )
        }
        // Sui does not support `signer` as a type.
        V::Signer(_) => return Err(unexpected_signer_error()),
    })
}

fn is_type(tag: &StructTag, address: &AccountAddress, module: &IdentStr, name: &IdentStr) -> bool {
    &tag.address == address
        && tag.module.as_ident_str() == module
        && tag.name.as_ident_str() == name
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
            return Err(Error::Internal(format!(
                "Couldn't find expected field '{_name}' of {_type}."
            )));
        }
    }};
}

/// Extracts a vector of bytes from `value`, assuming it's a `MoveValue::Vector` where all the
/// values are `MoveValue::U8`s.
fn extract_bytes(value: A::MoveValue) -> Result<Vec<u8>, Error> {
    use A::MoveValue as V;
    let V::Vector(elements) = value else {
        return Err(Error::Internal("Expected a vector.".to_string()));
    };

    let mut bytes = Vec::with_capacity(elements.len());
    for element in elements {
        let V::U8(byte) = element else {
            return Err(Error::Internal("Expected a byte.".to_string()));
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
    fields: Vec<(Identifier, A::MoveValue)>,
) -> Result<String, Error> {
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

        Error::Internal(format!("{e} in {sample:?}"))
    })
}

/// Extracts an address from the contents of a Move Struct, assuming the struct matches the
/// following shape:
///
/// ```notrust
///     { bytes: address }
/// ```
///
/// Which matches `0x2::object::ID`.
fn extract_id(
    type_: &StructTag,
    fields: Vec<(Identifier, A::MoveValue)>,
) -> Result<AccountAddress, Error> {
    use A::MoveValue as V;
    let V::Address(addr) = extract_field!(type_, fields, bytes) else {
        return Err(Error::Internal(
            "Expected ID.bytes to have type address.".to_string(),
        ));
    };

    Ok(addr)
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
    fields: Vec<(Identifier, A::MoveValue)>,
) -> Result<AccountAddress, Error> {
    use A::MoveValue as V;
    let V::Struct(s) = extract_field!(type_, fields, id) else {
        return Err(Error::Internal(
            "Expected UID.id to be a struct".to_string(),
        ));
    };

    let A::MoveStruct { type_, fields } = s;
    if !is_type(&type_, &SUI, MOD_OBJECT, TYP_ID) {
        return Err(Error::Internal(
            "Expected UID.id to have type ID.".to_string(),
        ));
    }

    extract_id(&type_, fields)
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
    fields: Vec<(Identifier, A::MoveValue)>,
) -> Result<Option<A::MoveValue>, Error> {
    let A::MoveValue::Vector(mut elements) = extract_field!(type_, fields, vec) else {
        return Err(Error::Internal(
            "Expected Option.vec to be a vector.".to_string(),
        ));
    };

    if elements.len() > 1 {
        return Err(Error::Internal(
            "Expected Option.vec to contain at most one element.".to_string(),
        ));
    };

    Ok(elements.pop())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use expect_test::expect;
    use move_core_types::{
        annotated_value::{self as A, MoveFieldLayout, MoveStructLayout as S, MoveTypeLayout as L},
        u256::U256,
    };

    use super::*;

    macro_rules! struct_layout {
        ($type:literal { $($name:literal : $layout:expr),* $(,)?}) => {
            A::MoveTypeLayout::Struct(Box::new(S {
                type_: StructTag::from_str($type).expect("Failed to parse struct"),
                fields: vec![$(MoveFieldLayout {
                    name: ident_str!($name).to_owned(),
                    layout: $layout,
                }),*]
            }))
        }
    }

    macro_rules! vector_layout {
        ($inner:expr) => {
            A::MoveTypeLayout::Vector(Box::new($inner))
        };
    }

    fn address(a: &str) -> SuiAddress {
        SuiAddress::from_str(a).unwrap()
    }

    fn data<T: Serialize>(layout: A::MoveTypeLayout, data: T) -> Result<MoveData, Error> {
        let tag: TypeTag = (&layout).into();

        // The format for type from its `Display` impl does not technically match the format that
        // the RPC expects from the data layer (where a type's package should be canonicalized), but
        // it will suffice.
        data_with_tag(format!("{}", tag), layout, data)
    }

    fn data_with_tag<T: Serialize>(
        tag: impl Into<String>,
        layout: A::MoveTypeLayout,
        data: T,
    ) -> Result<MoveData, Error> {
        let tag = TypeTag::from_str(tag.into().as_str()).unwrap();
        let type_ = MoveType::from(tag);
        let bcs = Base64(bcs::to_bytes(&data).unwrap());
        MoveValue { type_, bcs }.data_impl(layout)
    }

    fn json<T: Serialize>(layout: A::MoveTypeLayout, data: T) -> Result<Json, Error> {
        let tag: TypeTag = (&layout).into();
        let type_ = MoveType::from(tag);
        let bcs = Base64(bcs::to_bytes(&data).unwrap());
        MoveValue { type_, bcs }.json_impl(layout)
    }

    #[test]
    fn bool_data() {
        let v = data(L::Bool, true);
        let expect = expect!["Ok(Bool(true))"];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn bool_json() {
        let v = json(L::Bool, true).unwrap();
        let expect = expect!["true"];
        expect.assert_eq(&format!("{v}"));
    }

    #[test]
    fn u8_data() {
        let v = data(L::U8, 42u8);
        let expect = expect![[r#"Ok(Number(BigInt("42")))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn u8_json() {
        let v = json(L::U8, 42u8).unwrap();
        let expect = expect!["42"];
        expect.assert_eq(&format!("{v}"));
    }

    #[test]
    fn u16_data() {
        let v = data(L::U16, 424u16);
        let expect = expect![[r#"Ok(Number(BigInt("424")))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn u16_json() {
        let v = json(L::U16, 424u16).unwrap();
        let expect = expect!["424"];
        expect.assert_eq(&format!("{v}"));
    }

    #[test]
    fn u32_data() {
        let v = data(L::U32, 424_242u32);
        let expect = expect![[r#"Ok(Number(BigInt("424242")))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn u32_json() {
        let v = json(L::U32, 424_242u32).unwrap();
        let expect = expect!["424242"];
        expect.assert_eq(&format!("{v}"));
    }

    #[test]
    fn u64_data() {
        let v = data(L::U64, 42_424_242_424u64);
        let expect = expect![[r#"Ok(Number(BigInt("42424242424")))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn u64_json() {
        let v = json(L::U64, 42_424_242_424u64).unwrap();
        let expect = expect![[r#""42424242424""#]];
        expect.assert_eq(&format!("{v}"));
    }

    #[test]
    fn u128_data() {
        let v = data(L::U128, 424_242_424_242_424_242_424u128);
        let expect = expect![[r#"Ok(Number(BigInt("424242424242424242424")))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn u128_json() {
        let v = json(L::U128, 424_242_424_242_424_242_424u128).unwrap();
        let expect = expect![[r#""424242424242424242424""#]];
        expect.assert_eq(&format!("{v}"));
    }

    #[test]
    fn u256_data() {
        let v = data(
            L::U256,
            U256::from_str("42424242424242424242424242424242424242424").unwrap(),
        );
        let expect =
            expect![[r#"Ok(Number(BigInt("42424242424242424242424242424242424242424")))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn u256_json() {
        let v = json(
            L::U256,
            U256::from_str("42424242424242424242424242424242424242424").unwrap(),
        )
        .unwrap();
        let expect = expect![[r#""42424242424242424242424242424242424242424""#]];
        expect.assert_eq(&format!("{v}"));
    }

    #[test]
    fn ascii_string_data() {
        let l = struct_layout!("0x1::ascii::String" {
            "bytes": vector_layout!(L::U8)
        });

        let v = data(l, "The quick brown fox");
        let expect = expect![[r#"Ok(String("The quick brown fox"))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn ascii_string_json() {
        let l = struct_layout!("0x1::ascii::String" {
            "bytes": vector_layout!(L::U8)
        });

        let v = json(l, "The quick brown fox").unwrap();
        let expect = expect![[r#""The quick brown fox""#]];
        expect.assert_eq(&format!("{v}"));
    }

    #[test]
    fn utf8_string_data() {
        let l = struct_layout!("0x1::string::String" {
            "bytes": vector_layout!(L::U8)
        });

        let v = data(l, "jumped over the lazy dog.");
        let expect = expect![[r#"Ok(String("jumped over the lazy dog."))"#]];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn utf8_string_json() {
        let l = struct_layout!("0x1::string::String" {
            "bytes": vector_layout!(L::U8)
        });

        let v = json(l, "jumped over the lazy dog.").unwrap();
        let expect = expect![[r#""jumped over the lazy dog.""#]];
        expect.assert_eq(&format!("{v}"));
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
                Internal(
                    "invalid utf-8 sequence of 1 bytes from index 5 in \"Lorem�ipsum dolor sit amet ...\"",
                ),
            )"#]];
        expect.assert_eq(&format!("{v:#?}"));
    }

    #[test]
    fn address_data() {
        let v = data(L::Address, address("0x42"));
        let expect = expect!["Ok(Address(SuiAddress([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 66])))"];
        expect.assert_eq(&format!("{v:?}"));
    }

    #[test]
    fn address_json() {
        let v = json(L::Address, address("0x42")).unwrap();
        let expect =
            expect![[r#""0x0000000000000000000000000000000000000000000000000000000000000042""#]];
        expect.assert_eq(&format!("{v}"));
    }

    #[test]
    fn uid_data() {
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
    fn uid_json() {
        let l = struct_layout!("0x2::object::UID" {
            "id": struct_layout!("0x2::object::ID" {
                "bytes": L::Address,
            })
        });

        let v = json(l, address("0x42")).unwrap();
        let expect =
            expect![[r#""0x0000000000000000000000000000000000000000000000000000000000000042""#]];
        expect.assert_eq(&format!("{v}"));
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
    fn compound_json() {
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

        let v = json(
            l,
            (
                vec![] as Vec<Vec<u8>>,
                vec![
                    (44u16, vec!["Hello, world!"], address("0x45")),
                    (46u16, vec![], address("0x47")),
                ],
            ),
        )
        .unwrap();

        let expect = expect![[
            r#"{baz: null,qux: [{quy: 44,quz: "Hello, world!",frob: "0x0000000000000000000000000000000000000000000000000000000000000045"},{quy: 46,quz: null,frob: "0x0000000000000000000000000000000000000000000000000000000000000047"}]}"#
        ]];
        expect.assert_eq(&format!("{v}"));
    }

    #[test]
    fn signer_value() {
        let v = data(L::Signer, address("0x42"));
        let expect = expect![[r#"
            Err(
                Internal(
                    "Unexpected value of type: signer.",
                ),
            )"#]];
        expect.assert_eq(&format!("{v:#?}"));
    }

    #[test]
    fn signer_json() {
        let err = json(L::Signer, address("0x42")).unwrap_err();
        let expect = expect![[r#"Internal("Unexpected value of type: signer.")"#]];
        expect.assert_eq(&format!("{err:?}"));
    }

    #[test]
    fn signer_nested_data() {
        let v = data(
            vector_layout!(L::Signer),
            vec![address("0x42"), address("0x43")],
        );
        let expect = expect![[r#"
            Err(
                Internal(
                    "Unexpected value of type: signer.",
                ),
            )"#]];
        expect.assert_eq(&format!("{v:#?}"));
    }

    #[test]
    fn signer_nested_json() {
        let err = json(
            vector_layout!(L::Signer),
            vec![address("0x42"), address("0x43")],
        )
        .unwrap_err();

        let expect = expect![[r#"Internal("Unexpected value of type: signer.")"#]];
        expect.assert_eq(&format!("{err:?}"));
    }
}
