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
    annotated_visitor as AV,
    language_storage::TypeTag,
    u256::U256,
};
use serde_json::{Map, Value};
use sui_package_resolver::{PackageStore, Resolver, error::Error as ResolverError};
use sui_types::{
    balance::Balance,
    base_types::{RESOLVED_STD_OPTION, move_ascii_str_layout, move_utf8_str_layout, url_layout},
    event::Event,
    id::{ID, UID},
    object::option_visitor as OV,
};

/// Error type for JSON visitor operations
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Unexpected type")]
    UnexpectedType,

    #[error(transparent)]
    Visitor(#[from] AV::Error),
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
    /// Deserialize BCS bytes as JSON using the provided type layout.
    pub fn deserialize_value(bytes: &[u8], layout: &MoveTypeLayout) -> anyhow::Result<Value> {
        Ok(MoveValue::visit_deserialize(bytes, layout, &mut Self)?)
    }

    /// Deserialize BCS bytes as a JSON object representing a struct.
    pub fn deserialize_struct(
        bytes: &[u8],
        layout: &move_core_types::annotated_value::MoveStructLayout,
    ) -> anyhow::Result<Value> {
        Ok(MoveStruct::visit_deserialize(bytes, layout, &mut Self)?)
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

impl<'b, 'l> AV::Visitor<'b, 'l> for JsonVisitor {
    type Value = Value;
    type Error = Error;

    fn visit_u8(
        &mut self,
        _driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: u8,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u16(
        &mut self,
        _driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: u16,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u32(
        &mut self,
        _driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: u32,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64(
        &mut self,
        _driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: u64,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::String(value.to_string()))
    }

    fn visit_u128(
        &mut self,
        _driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: u128,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::String(value.to_string()))
    }

    fn visit_u256(
        &mut self,
        _driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: U256,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::String(value.to_string()))
    }

    fn visit_bool(
        &mut self,
        _driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: bool,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::Bool(value))
    }

    fn visit_address(
        &mut self,
        _driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::String(value.to_canonical_string(true)))
    }

    fn visit_signer(
        &mut self,
        _driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        // Signers are also addresses in JSON representation
        Ok(Value::String(value.to_canonical_string(true)))
    }

    fn visit_vector(
        &mut self,
        driver: &mut AV::VecDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        // If this is a vector of u8 (bytes), encode it using Base64
        if driver.element_layout().is_type(&TypeTag::U8) {
            use base64::{Engine, engine::general_purpose::STANDARD};

            if let Some(bytes) = driver
                .bytes()
                .get(driver.position()..(driver.position() + driver.len() as usize))
            {
                Ok(Value::String(STANDARD.encode(bytes)))
            } else {
                Err(AV::Error::UnexpectedEof.into())
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
            let s: &str = bcs::from_bytes(bytes).map_err(|_| AV::Error::UnexpectedEof)?;
            Ok(Value::String(s.to_string()))
        } else if layout == &UID::layout() || layout == &ID::layout() {
            // 0x2::object::UID or 0x2::object::ID

            let lo = driver.position();
            driver.skip_field()?;
            let hi = driver.position();

            // HACK: Bypassing the layout to deserialize its bytes as a Rust type.
            let bytes = &driver.bytes()[lo..hi];
            let id = AccountAddress::from_bytes(bytes)
                .map_err(|_| AV::Error::UnexpectedEof)?
                .to_canonical_string(true);
            Ok(Value::String(id))
        } else if (&ty.address, ty.module.as_ref(), ty.name.as_ref()) == RESOLVED_STD_OPTION {
            // 0x1::option::Option
            match OV::OptionVisitor(self).visit_struct(driver)? {
                Some(value) => Ok(value),
                None => Ok(Value::Null),
            }
        } else if Balance::is_balance_layout(layout) {
            // 0x2::balance::Balance

            let lo = driver.position();
            driver.skip_field()?;
            let hi = driver.position();

            // HACK: Bypassing the layout to deserialize its bytes as a Rust type.
            let bytes = &driver.bytes()[lo..hi];
            let balance = bcs::from_bytes::<u64>(bytes)
                .map_err(|_| AV::Error::UnexpectedEof)?
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
        driver: &mut AV::VariantDriver<'_, 'b, 'l>,
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

impl From<OV::Error> for Error {
    fn from(OV::Error: OV::Error) -> Self {
        Error::UnexpectedType
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_core_types::{
        annotated_value::{self as A, MoveTypeLayout as L},
        identifier::Identifier,
        language_storage::StructTag,
    };
    use serde_json::json;
    use std::str::FromStr;

    // ===== Test Helper Functions =====
    // Aligned with helpers from bounded_visitor in sui-types

    /// Create an identifier for test purposes
    fn ident_(name: &str) -> Identifier {
        Identifier::new(name).unwrap()
    }

    /// Parse an address string (e.g., "0x42") into an AccountAddress
    fn address(a: &str) -> AccountAddress {
        AccountAddress::from_str(a).unwrap()
    }

    /// Create a struct layout for test purposes
    fn layout_(rep: &str, fields: Vec<(&str, MoveTypeLayout)>) -> MoveTypeLayout {
        let type_ = StructTag::from_str(rep).unwrap();
        let fields = fields
            .into_iter()
            .map(|(name, layout)| A::MoveFieldLayout::new(ident_(name), layout))
            .collect();

        MoveTypeLayout::Struct(Box::new(A::MoveStructLayout { type_, fields }))
    }

    /// Create an enum layout for test purposes
    fn enum_(rep: &str, variants: Vec<(&str, Vec<(&str, MoveTypeLayout)>)>) -> MoveTypeLayout {
        let type_ = StructTag::from_str(rep).unwrap();
        let variants = variants
            .into_iter()
            .enumerate()
            .map(|(tag, (name, fields))| {
                let fields = fields
                    .into_iter()
                    .map(|(name, layout)| A::MoveFieldLayout::new(ident_(name), layout))
                    .collect();
                ((ident_(name), tag as u16), fields)
            })
            .collect();

        MoveTypeLayout::Enum(Box::new(A::MoveEnumLayout { type_, variants }))
    }

    /// Deserialize BCS bytes to JSON using the JsonVisitor
    /// For simple cases, you can pass a Rust value that serializes to the expected BCS format
    fn to_json<T: serde::Serialize>(layout: MoveTypeLayout, data: T) -> serde_json::Value {
        let bcs = bcs::to_bytes(&data).unwrap();
        MoveValue::visit_deserialize(&bcs, &layout, &mut JsonVisitor).unwrap()
    }

    #[test]
    fn test_ascii_string() {
        let l = L::Struct(Box::new(move_ascii_str_layout()));
        let actual = to_json(l, "The quick brown fox");
        let expect = json!("The quick brown fox");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_utf8_string() {
        let l = L::Struct(Box::new(move_utf8_str_layout()));
        let actual = to_json(l, "The quick brown fox");
        let expect = json!("The quick brown fox");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_url() {
        let l = L::Struct(Box::new(url_layout()));
        let actual = to_json(l, "https://example.com");
        let expect = json!("https://example.com");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_id() {
        let l = layout_("0x2::object::ID", vec![("bytes", L::Address)]);
        let actual = to_json(l, address("0x42"));
        let expect = json!("0x0000000000000000000000000000000000000000000000000000000000000042");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_uid() {
        let l = L::Struct(Box::new(UID::layout()));
        let actual = to_json(l, address("0x42"));
        let expect = json!("0x0000000000000000000000000000000000000000000000000000000000000042");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_option() {
        // Option is a struct with a "vec" field in Move
        let l = layout_(
            "0x1::option::Option<u64>",
            vec![("vec", L::Vector(Box::new(L::U64)))],
        );

        // None case: Option with empty vector
        let actual = to_json(l.clone(), None::<u64>);
        let expect = json!(null);
        assert_eq!(expect, actual);

        // Some case: Option with single element vector
        let actual = to_json(l, Some(42u64));
        let expect = json!("42");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_balance() {
        let l = layout_(
            "0x2::balance::Balance<0x2::sui::SUI>",
            vec![("value", L::U64)],
        );

        let actual = to_json(l, 100u64);
        let expect = json!("100");
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_compound() {
        // Test a struct containing multiple special types
        let layout = layout_(
            "0x42::foo::Compound",
            vec![
                ("name", L::Struct(Box::new(move_utf8_str_layout()))),
                ("id", L::Struct(Box::new(ID::layout()))),
                (
                    "balance",
                    layout_(
                        "0x2::balance::Balance<0x2::sui::SUI>",
                        vec![("value", L::U64)],
                    ),
                ),
                ("url", L::Struct(Box::new(url_layout()))),
                (
                    "opt_value",
                    layout_(
                        "0x1::option::Option<u64>",
                        vec![("vec", L::Vector(Box::new(L::U64)))],
                    ),
                ),
            ],
        );

        let actual = to_json(
            layout,
            (
                "Test Object",
                address("0x42"),
                1000u64,
                "https://example.com",
                Some(999u64),
            ),
        );

        let expected = json!({
            "name": "Test Object",
            "id": "0x0000000000000000000000000000000000000000000000000000000000000042",
            "balance": "1000",
            "url": "https://example.com",
            "opt_value": "999",
        });
        assert_eq!(actual, expected);

        // Test with None for optional
        let layout2 = layout_(
            "0x42::foo::Compound2",
            vec![(
                "opt",
                layout_(
                    "0x1::option::Option<u128>",
                    vec![("vec", L::Vector(Box::new(L::U128)))],
                ),
            )],
        );

        let actual2 = to_json(layout2, None::<u128>);
        let expected2 = json!({ "opt": null });
        assert_eq!(actual2, expected2);
    }

    #[test]
    fn test_simple_struct_to_json() {
        let layout = layout_(
            "0x2::coin::Coin<0x2::sui::SUI>",
            vec![("id", L::Address), ("balance", L::U64)],
        );

        let actual = to_json(layout, (AccountAddress::ONE, 1000u64));
        let expected = json!({
            "id": "0x0000000000000000000000000000000000000000000000000000000000000001",
            "balance": "1000"
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_nested_struct_with_vector() {
        let inner_layout = layout_("0x1::test::Inner", vec![("value", L::U32)]);
        let layout = layout_(
            "0x1::test::Outer",
            vec![
                ("items", L::Vector(Box::new(inner_layout.clone()))),
                ("count", L::U64),
            ],
        );

        let actual = to_json(layout, (vec![10u32, 20u32], 2u64));
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
        // Create an enum layout (like Option<u64>)
        let enum_layout = enum_(
            "0x1::option::Option<u64>",
            vec![("None", vec![]), ("Some", vec![("value", L::U64)])],
        );

        // Test "Some" variant
        let actual = to_json(enum_layout.clone(), Some(42u64));
        let expected = json!({
            "@variant": "Some",
            "value": "42"  // u64 as string
        });
        assert_eq!(actual, expected);

        // Test "None" variant
        let actual = to_json(enum_layout, None::<u64>);
        let expected = json!({
            "@variant": "None"
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_byte_vector_base64() {
        let layout = layout_(
            "0x1::test::Data",
            vec![
                ("bytes", L::Vector(Box::new(L::U8))),
                ("numbers", L::Vector(Box::new(L::U32))),
            ],
        );

        // "Hello" in bytes
        let actual = to_json(layout, (vec![72u8, 101, 108, 108, 111], vec![1u32, 2, 3]));
        let expected = json!({
            "bytes": "SGVsbG8=",  // "Hello" Base64 encoded
            "numbers": [1, 2, 3]  // u32 values as JSON numbers
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_large_numbers() {
        let layout = layout_(
            "0x1::test::Numbers",
            vec![
                ("small_u64", L::U64),
                ("large_u64", L::U64),
                ("u128_value", L::U128),
                ("u256_value", L::U256),
            ],
        );

        let json = to_json(
            layout,
            (1000u64, u64::MAX, u128::MAX, U256::from(123456789u128)),
        );

        assert_eq!(json["small_u64"], json!("1000"));
        assert_eq!(json["large_u64"], json!(u64::MAX.to_string()));
        assert_eq!(json["u128_value"], json!(u128::MAX.to_string()));
        assert_eq!(json["u256_value"], json!("123456789"));
    }
}
