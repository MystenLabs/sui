// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A visitor implementation that constructs JSON values from BCS bytes.
//!
//! This visitor traverses BCS-encoded Move data and builds a `serde_json::Value`
//! representation. Note that this approach loads the entire JSON structure into
//! memory, which may have significant memory implications for large objects or
//! collections. It should not be used in memory-constrained contexts like RPC
//! handlers where the size of the data is unbounded.

use move_core_types::{
    account_address::AccountAddress,
    annotated_value::{MoveStruct, MoveTypeLayout, MoveValue},
    annotated_visitor::{self, StructDriver, ValueDriver, VariantDriver, VecDriver, Visitor},
    language_storage::TypeTag,
    u256::U256,
};
use serde_json::{Map, Value};
use sui_package_resolver::{error::Error as ResolverError, PackageStore, Resolver};
use sui_types::{
    base_types::{move_ascii_str_layout, move_utf8_str_layout, RESOLVED_STD_OPTION},
    event::Event,
    id::{ID, UID},
    object::option_visitor::{OptionVisitor, OptionVisitorError},
    proto_value::{is_balance, url_layout},
};

/// Error type for JSON visitor operations
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Visitor(#[from] annotated_visitor::Error),

    #[error("Unexpected type")]
    UnexpectedType,
}

impl OptionVisitorError for Error {
    fn unexpected_type() -> Self {
        Error::UnexpectedType
    }
}

/// Error type for deserialization operations that involve both type resolution and BCS deserialization.
#[derive(thiserror::Error, Debug)]
pub enum DeserializationError {
    /// Failed to fetch type layout from the package resolver.
    #[error("Failed to fetch type layout: {0}")]
    LayoutFetch(#[from] ResolverError),

    /// Failed to deserialize BCS data to JSON.
    #[error("Failed to deserialize BCS data: {0}")]
    Deserialization(#[from] anyhow::Error),
}

/// A visitor that constructs JSON values from BCS bytes.
///
/// Number representation:
/// - u8, u16, u32 are represented as JSON numbers
/// - u64, u128, u256 are represented as strings to avoid precision loss
///
/// Special types:
/// - Addresses use full 64-character hex format with "0x" prefix
/// - Byte vectors (`Vec<u8>`) are Base64-encoded strings
pub struct JsonVisitor;

impl JsonVisitor {
    pub fn new() -> Self {
        Self
    }

    /// Deserialize BCS bytes as JSON using the provided type layout.
    pub fn deserialize_value(bytes: &[u8], layout: &MoveTypeLayout) -> anyhow::Result<Value> {
        let mut visitor = Self::new();
        Ok(MoveValue::visit_deserialize(bytes, layout, &mut visitor)?)
    }

    /// Deserialize BCS bytes as a JSON object representing a struct.
    pub fn deserialize_struct(
        bytes: &[u8],
        layout: &move_core_types::annotated_value::MoveStructLayout,
    ) -> anyhow::Result<Value> {
        let mut visitor = Self::new();
        Ok(MoveStruct::visit_deserialize(bytes, layout, &mut visitor)?)
    }

    /// Deserialize a single event to JSON using type resolution.
    ///
    /// This function:
    /// 1. Resolves the type layout for the event's type
    /// 2. Deserializes the BCS-encoded event contents to JSON
    ///
    /// If you need to deserialize multiple events, use
    /// [`deserialize_events`](Self::deserialize_events) instead, which processes
    /// events concurrently for better performance.
    pub async fn deserialize_event<S>(
        event: &Event,
        resolver: &Resolver<S>,
    ) -> Result<Value, DeserializationError>
    where
        S: PackageStore,
    {
        let type_tag = TypeTag::Struct(Box::new(event.type_.clone()));
        let layout = resolver.type_layout(type_tag).await?;
        Ok(Self::deserialize_value(&event.contents, &layout)?)
    }

    /// Deserialize multiple events to JSON concurrently.
    ///
    /// This function processes all events in parallel for better performance.
    ///
    /// If multiple events are from the same package, use
    /// a `Resolver` with a cached `PackageStore` (e.g., `RpcPackageStore::with_cache()`)
    /// to avoid fetching the same package multiple times.
    pub async fn deserialize_events<S>(
        events: &[Event],
        resolver: &Resolver<S>,
    ) -> Result<Vec<Value>, DeserializationError>
    where
        S: PackageStore,
    {
        use futures::future::try_join_all;

        let futures = events
            .iter()
            .map(|event| Self::deserialize_event(event, resolver));
        try_join_all(futures).await
    }
}

impl Default for JsonVisitor {
    fn default() -> Self {
        Self::new()
    }
}

impl<'b, 'l> Visitor<'b, 'l> for JsonVisitor {
    type Value = Value;
    type Error = Error;

    fn visit_u8(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u8,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u16(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u16,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u32(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u32,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u64,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::String(value.to_string()))
    }

    fn visit_u128(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u128,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::String(value.to_string()))
    }

    fn visit_u256(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: U256,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::String(value.to_string()))
    }

    fn visit_bool(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: bool,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::Bool(value))
    }

    fn visit_address(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        // Use to_canonical_string(true) for full address with "0x" prefix
        Ok(Value::String(value.to_canonical_string(true)))
    }

    fn visit_signer(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        // Signers are also addresses in JSON representation
        Ok(Value::String(value.to_canonical_string(true)))
    }

    fn visit_vector(
        &mut self,
        driver: &mut VecDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        // If this is a vector of u8 (bytes), encode it using Base64
        if driver
            .element_layout()
            .is_type(&move_core_types::language_storage::TypeTag::U8)
        {
            use base64::{engine::general_purpose::STANDARD, Engine};

            if let Some(bytes) = driver
                .bytes()
                .get(driver.position()..(driver.position() + driver.len() as usize))
            {
                Ok(Value::String(STANDARD.encode(bytes)))
            } else {
                Err(annotated_visitor::Error::UnexpectedEof.into())
            }
        } else {
            // Regular vector - collect elements
            let mut elements = Vec::new();
            while let Some(elem) = driver.next_element(self)? {
                elements.push(elem);
            }
            Ok(Value::Array(elements))
        }
    }

    fn visit_struct(
        &mut self,
        driver: &mut StructDriver<'_, 'b, 'l>,
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
            let s: &str = bcs::from_bytes(bytes).map_err(|_| Error::UnexpectedType)?;
            Ok(Value::String(s.to_string()))
        } else if layout == &UID::layout() || layout == &ID::layout() {
            // 0x2::object::UID or 0x2::object::ID

            let lo = driver.position();
            driver.skip_field()?;
            let hi = driver.position();

            // HACK: Bypassing the layout to deserialize its bytes as a Rust type.
            let bytes = &driver.bytes()[lo..hi];
            let id = AccountAddress::from_bytes(bytes)
                .map_err(|_| Error::UnexpectedType)?
                .to_canonical_string(true);
            Ok(Value::String(id))
        } else if (&ty.address, ty.module.as_ref(), ty.name.as_ref()) == RESOLVED_STD_OPTION {
            // 0x1::option::Option
            match OptionVisitor(self).visit_struct(driver)? {
                Some(value) => Ok(value),
                None => Ok(Value::Null),
            }
        } else if is_balance(layout) {
            // 0x2::balance::Balance

            let lo = driver.position();
            driver.skip_field()?;
            let hi = driver.position();

            // HACK: Bypassing the layout to deserialize its bytes as a Rust type.
            let bytes = &driver.bytes()[lo..hi];
            let balance = bcs::from_bytes::<u64>(bytes)
                .map_err(|_| Error::UnexpectedType)?
                .to_string();
            Ok(Value::String(balance))
        } else {
            // Arbitrary structs
            let mut fields = Map::new();

            while let Some((field, value)) = driver.next_field(self)? {
                fields.insert(field.name.to_string(), value);
            }

            Ok(Value::Object(fields))
        }
    }

    fn visit_variant(
        &mut self,
        driver: &mut VariantDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let mut fields = Map::new();

        fields.insert(
            "@variant".to_string(),
            Value::String(driver.variant_name().to_string()),
        );

        // Add all variant fields
        while let Some((field, value)) = driver.next_field(self)? {
            fields.insert(field.name.to_string(), value);
        }

        Ok(Value::Object(fields))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_core_types::{
        annotated_value::{self as A, MoveTypeLayout, MoveValue},
        identifier::Identifier,
        language_storage::StructTag,
    };
    use serde_json::json;
    use std::str::FromStr;

    // ===== Test Helper Functions =====

    /// Parse a struct type string (e.g., "0x1::module::Struct<T>")
    fn struct_type(type_str: &str) -> StructTag {
        StructTag::from_str(type_str).unwrap()
    }

    /// Create a Move struct layout from a type string and field specifications
    fn make_layout(type_str: &str, fields: Vec<(&str, MoveTypeLayout)>) -> MoveTypeLayout {
        MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
            type_: struct_type(type_str),
            fields: fields
                .into_iter()
                .map(|(name, layout)| A::MoveFieldLayout {
                    name: Identifier::new(name).unwrap(),
                    layout,
                })
                .collect(),
        }))
    }

    /// Create a Move struct value from a type string and field values
    fn make_value(type_str: &str, fields: Vec<(&str, MoveValue)>) -> MoveValue {
        MoveValue::Struct(A::MoveStruct {
            type_: struct_type(type_str),
            fields: fields
                .into_iter()
                .map(|(name, value)| (Identifier::new(name).unwrap(), value))
                .collect(),
        })
    }

    /// Deserialize BCS bytes to JSON using the JsonVisitor
    /// For simple cases, you can pass a Rust value that serializes to the expected BCS format
    fn to_json<T: serde::Serialize>(layout: MoveTypeLayout, data: T) -> serde_json::Value {
        let bcs = bcs::to_bytes(&data).unwrap();
        let mut visitor = JsonVisitor::new();
        MoveValue::visit_deserialize(&bcs, &layout, &mut visitor).unwrap()
    }

    /// Parse an address string (e.g., "0x42") into an AccountAddress
    fn address(a: &str) -> AccountAddress {
        AccountAddress::from_str(a).unwrap()
    }

    use MoveTypeLayout as L;

    #[test]
    fn test_ascii_string() {
        let l = make_layout(
            "0x1::ascii::String",
            vec![("bytes", L::Vector(Box::new(L::U8)))],
        );
        let actual = to_json(l, "The quick brown fox");
        let expect = json!("The quick brown fox");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_utf8_string() {
        let l = make_layout(
            "0x1::string::String",
            vec![("bytes", L::Vector(Box::new(L::U8)))],
        );
        let actual = to_json(l, "The quick brown fox");
        let expect = json!("The quick brown fox");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_url() {
        let l = make_layout(
            "0x2::url::Url",
            vec![(
                "url",
                make_layout(
                    "0x1::ascii::String",
                    vec![("bytes", L::Vector(Box::new(L::U8)))],
                ),
            )],
        );
        let actual = to_json(l, "https://example.com");
        let expect = json!("https://example.com");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_id() {
        let l = make_layout("0x2::object::ID", vec![("bytes", L::Address)]);
        let actual = to_json(l, address("0x42"));
        let expect = json!("0x0000000000000000000000000000000000000000000000000000000000000042");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_uid() {
        let l = make_layout(
            "0x2::object::UID",
            vec![(
                "id",
                make_layout("0x2::object::ID", vec![("bytes", L::Address)]),
            )],
        );
        let actual = to_json(l, address("0x42"));
        let expect = json!("0x0000000000000000000000000000000000000000000000000000000000000042");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_option() {
        use MoveValue as V;

        // Option is a struct with a "vec" field in Move
        let l = make_layout(
            "0x1::option::Option<u64>",
            vec![("vec", L::Vector(Box::new(L::U64)))],
        );

        // None case: Option with empty vector
        let none_value = make_value("0x1::option::Option<u64>", vec![("vec", V::Vector(vec![]))]);
        let actual = to_json(l.clone(), none_value.undecorate());
        let expect = json!(null);
        assert_eq!(expect, actual);

        // Some case: Option with single element vector
        let some_value = make_value(
            "0x1::option::Option<u64>",
            vec![("vec", V::Vector(vec![V::U64(42)]))],
        );
        let actual = to_json(l, some_value.undecorate());
        let expect = json!("42");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_balance() {
        let l = make_layout(
            "0x2::balance::Balance<0x2::sui::SUI>",
            vec![("value", L::U64)],
        );

        let actual = to_json(l, 100u64);
        let expect = json!("100");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_compound() {
        use MoveTypeLayout as T;
        use MoveValue as V;

        // Test a struct containing multiple special types
        let layout = make_layout(
            "0x42::foo::Compound",
            vec![
                (
                    "name",
                    make_layout(
                        "0x1::string::String",
                        vec![("bytes", T::Vector(Box::new(T::U8)))],
                    ),
                ),
                (
                    "id",
                    make_layout("0x2::object::ID", vec![("bytes", T::Address)]),
                ),
                (
                    "balance",
                    make_layout(
                        "0x2::balance::Balance<0x2::sui::SUI>",
                        vec![("value", T::U64)],
                    ),
                ),
                (
                    "url",
                    make_layout(
                        "0x2::url::Url",
                        vec![(
                            "url",
                            make_layout(
                                "0x1::ascii::String",
                                vec![("bytes", T::Vector(Box::new(T::U8)))],
                            ),
                        )],
                    ),
                ),
                (
                    "opt_value",
                    make_layout(
                        "0x1::option::Option<u64>",
                        vec![("vec", T::Vector(Box::new(T::U64)))],
                    ),
                ),
            ],
        );

        // Create value with Some(999)
        let value = make_value(
            "0x42::foo::Compound",
            vec![
                (
                    "name",
                    make_value(
                        "0x1::string::String",
                        vec![(
                            "bytes",
                            V::Vector("Test Object".bytes().map(V::U8).collect()),
                        )],
                    ),
                ),
                (
                    "id",
                    make_value(
                        "0x2::object::ID",
                        vec![("bytes", V::Address(address("0x42")))],
                    ),
                ),
                (
                    "balance",
                    make_value(
                        "0x2::balance::Balance<0x2::sui::SUI>",
                        vec![("value", V::U64(1000))],
                    ),
                ),
                (
                    "url",
                    make_value(
                        "0x2::url::Url",
                        vec![(
                            "url",
                            make_value(
                                "0x1::ascii::String",
                                vec![(
                                    "bytes",
                                    V::Vector("https://example.com".bytes().map(V::U8).collect()),
                                )],
                            ),
                        )],
                    ),
                ),
                (
                    "opt_value",
                    make_value(
                        "0x1::option::Option<u64>",
                        vec![("vec", V::Vector(vec![V::U64(999)]))],
                    ),
                ),
            ],
        );

        let actual = to_json(layout, value.undecorate());
        let expected = json!({
            "name": "Test Object",
            "id": "0x0000000000000000000000000000000000000000000000000000000000000042",
            "balance": "1000",
            "url": "https://example.com",
            "opt_value": "999",
        });
        assert_eq!(actual, expected);

        // Test with None for optional
        let layout2 = make_layout(
            "0x42::foo::Compound2",
            vec![(
                "opt",
                make_layout(
                    "0x1::option::Option<u128>",
                    vec![("vec", T::Vector(Box::new(T::U128)))],
                ),
            )],
        );

        let value2 = make_value(
            "0x42::foo::Compound2",
            vec![(
                "opt",
                make_value(
                    "0x1::option::Option<u128>",
                    vec![
                        ("vec", V::Vector(vec![])), // Empty vector for None
                    ],
                ),
            )],
        );

        let actual2 = to_json(layout2, value2.undecorate());
        let expected2 = json!({ "opt": null });
        assert_eq!(actual2, expected2);
    }

    #[test]
    fn test_simple_struct_to_json() {
        use MoveTypeLayout as T;
        use MoveValue as V;

        let layout = make_layout(
            "0x2::coin::Coin<0x2::sui::SUI>",
            vec![("id", T::Address), ("balance", T::U64)],
        );
        let value = make_value(
            "0x2::coin::Coin<0x2::sui::SUI>",
            vec![
                ("id", V::Address(AccountAddress::ONE)),
                ("balance", V::U64(1000)),
            ],
        );

        let actual = to_json(layout, value.undecorate());
        let expected = json!({
            "id": "0x0000000000000000000000000000000000000000000000000000000000000001",
            "balance": "1000"
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_nested_struct_with_vector() {
        use MoveTypeLayout as T;
        use MoveValue as V;

        let inner_layout = make_layout("0x1::test::Inner", vec![("value", T::U32)]);
        let layout = make_layout(
            "0x1::test::Outer",
            vec![
                ("items", T::Vector(Box::new(inner_layout.clone()))),
                ("count", T::U64),
            ],
        );
        let inner1 = make_value("0x1::test::Inner", vec![("value", V::U32(10))]);
        let inner2 = make_value("0x1::test::Inner", vec![("value", V::U32(20))]);
        let value = make_value(
            "0x1::test::Outer",
            vec![
                ("items", V::Vector(vec![inner1, inner2])),
                ("count", V::U64(2)),
            ],
        );

        let actual = to_json(layout, value.undecorate());
        let expected = json!({
            "items": [
                {
                    "value": 10
                },
                {
                    "value": 20
                }
            ],
            "count": "2"
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_variant() {
        use std::collections::BTreeMap;
        use MoveTypeLayout as T;
        use MoveValue as V;

        // Create an enum layout (like Option<u64>)
        let variant_layout = A::MoveFieldLayout {
            name: Identifier::new("value").unwrap(),
            layout: T::U64,
        };

        let mut variants = BTreeMap::new();
        variants.insert((Identifier::new("None").unwrap(), 0), vec![]);
        variants.insert(
            (Identifier::new("Some").unwrap(), 1),
            vec![variant_layout.clone()],
        );

        let enum_layout = T::Enum(Box::new(A::MoveEnumLayout {
            type_: struct_type("0x1::option::Option<u64>"),
            variants,
        }));

        // Test "Some" variant
        let some_value = V::Variant(A::MoveVariant {
            type_: struct_type("0x1::option::Option<u64>"),
            variant_name: Identifier::new("Some").unwrap(),
            tag: 1,
            fields: vec![(Identifier::new("value").unwrap(), V::U64(42))],
        });

        let actual = to_json(enum_layout.clone(), some_value.undecorate());
        let expected = json!({
            "@variant": "Some",
            "value": "42"  // u64 as string
        });
        assert_eq!(actual, expected);

        // Test "None" variant
        let none_value = V::Variant(A::MoveVariant {
            type_: struct_type("0x1::option::Option<u64>"),
            variant_name: Identifier::new("None").unwrap(),
            tag: 0,
            fields: vec![],
        });

        let actual = to_json(enum_layout, none_value.undecorate());
        let expected = json!({
            "@variant": "None"
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_byte_vector_base64() {
        use MoveTypeLayout as T;
        use MoveValue as V;

        let layout = make_layout(
            "0x1::test::Data",
            vec![
                ("bytes", T::Vector(Box::new(T::U8))),
                ("numbers", T::Vector(Box::new(T::U32))),
            ],
        );

        // "Hello" in bytes
        let bytes_vec = vec![72u8, 101, 108, 108, 111];
        let value = make_value(
            "0x1::test::Data",
            vec![
                (
                    "bytes",
                    V::Vector(bytes_vec.into_iter().map(V::U8).collect()),
                ),
                ("numbers", V::Vector(vec![V::U32(1), V::U32(2), V::U32(3)])),
            ],
        );

        let actual = to_json(layout, value.undecorate());
        let expected = json!({
            "bytes": "SGVsbG8=",  // "Hello" Base64 encoded
            "numbers": [1, 2, 3]  // u32 values as JSON numbers
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_large_numbers() {
        use MoveTypeLayout as T;
        use MoveValue as V;

        let layout = make_layout(
            "0x1::test::Numbers",
            vec![
                ("small_u64", T::U64),
                ("large_u64", T::U64),
                ("u128_value", T::U128),
                ("u256_value", T::U256),
            ],
        );
        let value = make_value(
            "0x1::test::Numbers",
            vec![
                ("small_u64", V::U64(1000)),
                ("large_u64", V::U64(u64::MAX)),
                ("u128_value", V::U128(u128::MAX)),
                ("u256_value", V::U256(U256::from(123456789u128))),
            ],
        );

        let json = to_json(layout, value.undecorate());
        assert_eq!(json["small_u64"], json!("1000"));
        assert_eq!(json["large_u64"], json!(u64::MAX.to_string()));
        assert_eq!(json["u128_value"], json!(u128::MAX.to_string()));
        assert_eq!(json["u256_value"], json!("123456789"));
    }
}
