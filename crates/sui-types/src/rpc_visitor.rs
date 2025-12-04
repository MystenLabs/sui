// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_visitor as AV;
use move_core_types::language_storage::TypeTag;
use move_core_types::u256::U256;

use crate::balance::Balance;
use crate::base_types::RESOLVED_STD_OPTION;
use crate::base_types::move_ascii_str_layout;
use crate::base_types::move_utf8_str_layout;
use crate::base_types::url_layout;
use crate::id::ID;
use crate::id::UID;
use crate::object::option_visitor as OV;

/// A trait for serializing Move values into some nested structured representation that supports
/// `null`, `bool`, numbers, strings, vectors, and maps (e.g. JSON or Protobuf).
///
/// Writers are allowed to fail, e.g. to limit resource usage.
pub trait Writer {
    type Value;
    type Error: std::error::Error
        + From<Error>
        + From<OV::Error>
        + From<AV::Error>
        + Send
        + Sync
        + 'static;

    type Vec: Default;
    type Map: Default;

    type Nested<'a>: Writer<Value = Self::Value, Error = Self::Error, Vec = Self::Vec, Map = Self::Map>
    where
        Self: 'a;

    /// Produce a new writer for writing into nested contexts (e.g. fields of structs or enum
    /// variants, or elements of vectors).
    fn nest(&mut self) -> Result<Self::Nested<'_>, Self::Error>;

    /// Write a `null` value.
    fn write_null(&mut self) -> Result<Self::Value, Self::Error>;

    /// Write a `true` or `false` value.
    fn write_bool(&mut self, value: bool) -> Result<Self::Value, Self::Error>;

    /// Write a numeric value that fits in a `u32`.
    fn write_number(&mut self, value: u32) -> Result<Self::Value, Self::Error>;

    /// Write a string value.
    fn write_str(&mut self, value: String) -> Result<Self::Value, Self::Error>;

    /// Write a completed vector.
    fn write_vec(&mut self, value: Self::Vec) -> Result<Self::Value, Self::Error>;

    /// Write a completed key-value map.
    fn write_map(&mut self, value: Self::Map) -> Result<Self::Value, Self::Error>;

    /// Add an element to a vector.
    fn vec_push_element(
        &mut self,
        vec: &mut Self::Vec,
        val: Self::Value,
    ) -> Result<(), Self::Error>;

    /// Add a key-value pair to a map.
    fn map_push_field(
        &mut self,
        map: &mut Self::Map,
        key: String,
        val: Self::Value,
    ) -> Result<(), Self::Error>;
}

/// A visitor that serializes Move values into some representation appropriate for RPC outputs.
///
/// The `W: Writer` type parameter determines the output format and how resource limits are
/// enforced.
pub struct RpcVisitor<W: Writer> {
    writer: W,
}

#[derive(thiserror::Error, Debug)]
#[error("Unexpected type")]
pub struct Error;

impl<W: Writer> RpcVisitor<W> {
    /// Create a new RPC visitor that writes into the given writer.
    pub fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<'b, 'l, W: Writer> AV::Visitor<'b, 'l> for RpcVisitor<W> {
    type Value = <W as Writer>::Value;
    type Error = <W as Writer>::Error;

    fn visit_u8(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: u8,
    ) -> Result<Self::Value, Self::Error> {
        self.writer.write_number(value as u32)
    }

    fn visit_u16(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: u16,
    ) -> Result<Self::Value, Self::Error> {
        self.writer.write_number(value as u32)
    }

    fn visit_u32(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: u32,
    ) -> Result<Self::Value, Self::Error> {
        self.writer.write_number(value)
    }

    fn visit_u64(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: u64,
    ) -> Result<Self::Value, Self::Error> {
        self.writer.write_str(value.to_string())
    }

    fn visit_u128(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: u128,
    ) -> Result<Self::Value, Self::Error> {
        self.writer.write_str(value.to_string())
    }

    fn visit_u256(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: U256,
    ) -> Result<Self::Value, Self::Error> {
        self.writer.write_str(value.to_string())
    }

    fn visit_bool(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: bool,
    ) -> Result<Self::Value, Self::Error> {
        self.writer.write_bool(value)
    }

    fn visit_address(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        self.writer.write_str(value.to_canonical_string(true))
    }

    fn visit_signer(
        &mut self,
        _: &AV::ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        self.writer.write_str(value.to_canonical_string(true))
    }

    fn visit_vector(
        &mut self,
        driver: &mut AV::VecDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        if driver.element_layout().is_type(&TypeTag::U8) {
            // Base64 encode arbitrary bytes
            use base64::{Engine, engine::general_purpose::STANDARD};

            if let Some(bytes) = driver
                .bytes()
                .get(driver.position()..(driver.position() + driver.len() as usize))
            {
                let b64 = STANDARD.encode(bytes);
                self.writer.write_str(b64)
            } else {
                Err(AV::Error::UnexpectedEof.into())
            }
        } else {
            let mut elems = W::Vec::default();
            {
                let nested = self.writer.nest()?;
                let mut visitor = RpcVisitor { writer: nested };

                while let Some(elem) = driver.next_element(&mut visitor)? {
                    visitor.writer.vec_push_element(&mut elems, elem)?;
                }
            }

            self.writer.write_vec(elems)
        }
    }

    fn visit_struct(
        &mut self,
        driver: &mut AV::StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let ty = &driver.struct_layout().type_;
        let layout = driver.struct_layout();

        if layout == &move_ascii_str_layout()
            || layout == &move_utf8_str_layout()
            || layout == &url_layout()
        {
            // 0x1::ascii::String or 0x1::string::String or 0x2::url::Url

            let lo = driver.position();
            driver.skip_field()?;
            let hi = driver.position();

            // HACK: Bypassing the layout to deserialize its bytes as a Rust type.
            let bytes = &driver.bytes()[lo..hi];
            let s: String = bcs::from_bytes(bytes).map_err(|_| Error)?;
            self.writer.write_str(s)
        } else if layout == &UID::layout() || layout == &ID::layout() {
            // 0x2::object::UID or 0x2::object::ID

            let lo = driver.position();
            driver.skip_field()?;
            let hi = driver.position();

            // HACK: Bypassing the layout to deserialize its bytes as a Rust type.
            let bytes = &driver.bytes()[lo..hi];
            let id = AccountAddress::from_bytes(bytes)
                .map_err(|_| Error)?
                .to_canonical_string(true);

            self.writer.write_str(id)
        } else if (&ty.address, ty.module.as_ref(), ty.name.as_ref()) == RESOLVED_STD_OPTION {
            // 0x1::option::Option

            match OV::OptionVisitor(self).visit_struct(driver)? {
                Some(value) => Ok(value),
                None => self.writer.write_null(),
            }
        } else if Balance::is_balance_layout(layout) {
            // 0x2::balance::Balance

            let lo = driver.position();
            driver.skip_field()?;
            let hi = driver.position();

            // HACK: Bypassing the layout to deserialize its bytes as a Rust type.
            let bytes = &driver.bytes()[lo..hi];
            let balance = bcs::from_bytes::<u64>(bytes)
                .map_err(|_| Error)?
                .to_string();

            self.writer.write_str(balance)
        } else {
            // Arbitrary structs

            let mut map = W::Map::default();
            {
                let nested = self.writer.nest()?;
                let mut visitor = RpcVisitor { writer: nested };

                while let Some((field, elem)) = driver.next_field(&mut visitor)? {
                    let name = field.name.to_string();
                    visitor.writer.map_push_field(&mut map, name, elem)?;
                }
            }

            self.writer.write_map(map)
        }
    }

    fn visit_variant(
        &mut self,
        driver: &mut AV::VariantDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let mut map = W::Map::default();
        {
            let mut nested = self.writer.nest()?;

            let variant = nested.write_str(driver.variant_name().to_string())?;
            nested.map_push_field(&mut map, "@variant".to_owned(), variant)?;

            let mut visitor = RpcVisitor { writer: nested };
            while let Some((field, elem)) = driver.next_field(&mut visitor)? {
                let name = field.name.to_string();
                visitor.writer.map_push_field(&mut map, name, elem)?;
            }
        }

        self.writer.write_map(map)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use move_core_types::annotated_value as A;
    use move_core_types::ident_str;
    use move_core_types::language_storage::StructTag;
    use serde::Serialize;
    use serde_json::Value;
    use serde_json::json;

    use super::*;

    use A::MoveTypeLayout as L;

    macro_rules! struct_ {
        ($type:literal { $($name:literal : $layout:expr),* $(,)?}) => {
            A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
                type_: StructTag::from_str($type).expect("Failed to parse struct"),
                fields: vec![$(A::MoveFieldLayout {
                    name: ident_str!($name).to_owned(),
                    layout: $layout,
                }),*]
            }))
        }
    }

    macro_rules! vector_ {
        ($inner:expr) => {
            A::MoveTypeLayout::Vector(Box::new($inner))
        };
    }

    struct JsonWriter;

    #[derive(thiserror::Error, Debug)]
    enum Error {
        #[error(transparent)]
        Visitor(#[from] AV::Error),

        #[error("Unexpected type")]
        UnexpectedType,
    }

    impl Writer for JsonWriter {
        type Value = Value;
        type Error = Error;

        type Vec = Vec<Value>;
        type Map = serde_json::Map<String, Value>;

        type Nested<'a> = Self;

        fn nest(&mut self) -> Result<Self::Nested<'_>, Self::Error> {
            Ok(JsonWriter)
        }

        fn write_null(&mut self) -> Result<Self::Value, Self::Error> {
            Ok(Value::Null)
        }

        fn write_bool(&mut self, value: bool) -> Result<Self::Value, Self::Error> {
            Ok(Value::Bool(value))
        }

        fn write_number(&mut self, value: u32) -> Result<Self::Value, Self::Error> {
            Ok(Value::Number(value.into()))
        }

        fn write_str(&mut self, value: String) -> Result<Self::Value, Self::Error> {
            Ok(Value::String(value))
        }

        fn write_vec(&mut self, value: Self::Vec) -> Result<Self::Value, Self::Error> {
            Ok(Value::Array(value))
        }

        fn write_map(&mut self, value: Self::Map) -> Result<Self::Value, Self::Error> {
            Ok(Value::Object(value))
        }

        fn vec_push_element(
            &mut self,
            vec: &mut Self::Vec,
            value: Self::Value,
        ) -> Result<(), Self::Error> {
            vec.push(value);
            Ok(())
        }

        fn map_push_field(
            &mut self,
            map: &mut Self::Map,
            key: String,
            val: Self::Value,
        ) -> Result<(), Self::Error> {
            map.insert(key, val);
            Ok(())
        }
    }

    impl From<OV::Error> for Error {
        fn from(OV::Error: OV::Error) -> Self {
            Error::UnexpectedType
        }
    }

    impl From<super::Error> for Error {
        fn from(super::Error: super::Error) -> Self {
            Error::UnexpectedType
        }
    }

    fn json<T: Serialize>(layout: A::MoveTypeLayout, data: T) -> Value {
        let bcs = bcs::to_bytes(&data).unwrap();
        let mut visitor = RpcVisitor::new(JsonWriter);
        A::MoveValue::visit_deserialize(&bcs, &layout, &mut visitor).unwrap()
    }

    fn address(a: &str) -> sui_sdk_types::Address {
        sui_sdk_types::Address::from_str(a).unwrap()
    }

    #[test]
    fn json_bool() {
        let actual = json(L::Bool, true);
        let expect = json!(true);
        assert_eq!(actual, expect);

        let actual = json(L::Bool, false);
        let expect = json!(false);
        assert_eq!(actual, expect);
    }

    #[test]
    fn json_u8() {
        let actual = json(L::U8, 42u8);
        let expect = json!(42u8);
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_u16() {
        let actual = json(L::U16, 424u16);
        let expect = json!(424u16);
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_u32() {
        let actual = json(L::U32, 432_432u32);
        let expect = json!(432_432u32);
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_u64() {
        let actual = json(L::U64, 432_432_432_432u64);
        let expect = json!(432_432_432_432u64.to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_u128() {
        let actual = json(L::U128, 424_242_424_242_424_242_424u128);
        let expect = json!(424_242_424_242_424_242_424u128.to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_u256() {
        let actual = json(
            L::U256,
            U256::from_str("42424242424242424242424242424242424242424").unwrap(),
        );
        let expect = json!("42424242424242424242424242424242424242424");
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_ascii_string() {
        let l = struct_!("0x1::ascii::String" {
            "bytes": vector_!(L::U8)
        });
        let actual = json(l, "The quick brown fox");
        let expect = json!("The quick brown fox");
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_utf8_string() {
        let l = struct_!("0x1::string::String" {
            "bytes": vector_!(L::U8)
        });
        let actual = json(l, "The quick brown fox");
        let expect = json!("The quick brown fox");
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_url() {
        let l = struct_!("0x2::url::Url" {
            "url": struct_!("0x1::ascii::String" {
                "bytes": vector_!(L::U8)
            })
        });
        let actual = json(l, "https://example.com");
        let expect = json!("https://example.com");
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_address() {
        let actual = json(L::Address, address("0x42"));
        let expect = json!(address("0x42").to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_signer() {
        let actual = json(L::Signer, address("0x42"));
        let expect = json!(address("0x42").to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_id() {
        let l = struct_!("0x2::object::ID" {
            "bytes": L::Address,
        });
        let actual = json(l, address("0x42"));
        let expect = json!(address("0x42").to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_uid() {
        let l = struct_!("0x2::object::UID" {
            "id": struct_!("0x2::object::ID" {
                "bytes": L::Address,
            })
        });
        let actual = json(l, address("0x42"));
        let expect = json!(address("0x42").to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_option() {
        let l = struct_!("0x42::foo::Bar" {
            "baz": struct_!("0x1::option::Option<u8>" { "vec": vector_!(L::U8) }),
        });

        let actual = json(l, Option::<Vec<u8>>::None);
        let expect = json!({
            "baz": null,
        });
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_balance() {
        let l = struct_!("0x2::balance::Balance<0x2::sui::SUI>" {
            "value": L::U64,
        });

        let actual = json(l, 100u64);
        let expect = json!(100u64.to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_compound() {
        let l = struct_!("0x42::foo::Bar" {
            "baz": struct_!("0x1::option::Option<u8>" { "vec": vector_!(L::U8) }),
            "qux": vector_!(struct_!("0x43::xy::Zzy" {
                "quy": L::U16,
                "quz": struct_!("0x1::option::Option<0x1::ascii::String>" {
                    "vec": vector_!(struct_!("0x1::ascii::String" {
                        "bytes": vector_!(L::U8),
                    }))
                }),
                "frob": L::Address,
            })),
        });

        let actual = json(
            l,
            (
                Option::<Vec<u8>>::None,
                vec![
                    (44u16, Some("Hello, world!"), address("0x45")),
                    (46u16, None, address("0x47")),
                ],
            ),
        );
        let expect = json!({
            "baz": null,
            "qux": [{
                "quy": 44,
                "quz": "Hello, world!",
                "frob": address("0x45").to_string(),
            },
            {
                "quy": 46,
                "quz": null,
                "frob": address("0x47").to_string(),
            }
            ],
        });
        assert_eq!(expect, actual);
    }
}
