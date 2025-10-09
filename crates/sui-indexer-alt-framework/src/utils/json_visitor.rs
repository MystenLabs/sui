// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A visitor implementation that constructs JSON values directly from BCS bytes.
//!
//! This visitor traverses BCS-encoded Move data and builds a `serde_json::Value`
//! without creating intermediate MoveStruct representations, making it more memory
//! efficient for large objects.

use move_core_types::{
    account_address::AccountAddress,
    annotated_visitor::{self, StructDriver, ValueDriver, VariantDriver, VecDriver, Visitor},
    u256::U256,
};
use serde_json::{Map, Value};

/// A visitor that constructs JSON values from BCS bytes.
pub struct JsonVisitor;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Visitor(#[from] annotated_visitor::Error),
}

impl JsonVisitor {
    pub fn new() -> Self {
        Self
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
        Ok(Value::String(value.to_string()))
    }

    fn visit_u16(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u16,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::String(value.to_string()))
    }

    fn visit_u32(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u32,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Value::String(value.to_string()))
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
        let mut elements = Vec::new();
        while let Some(elem) = driver.next_element(self)? {
            elements.push(elem);
        }
        Ok(Value::Array(elements))
    }

    fn visit_struct(
        &mut self,
        driver: &mut StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let mut fields = Map::new();

        // Add all struct fields
        while let Some((field, value)) = driver.next_field(self)? {
            fields.insert(field.name.to_string(), value);
        }

        Ok(Value::Object(fields))
    }

    fn visit_variant(
        &mut self,
        driver: &mut VariantDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let mut fields = Map::new();

        // Include full variant information - essential for understanding enum values
        let type_tag = driver.enum_layout().type_.clone();
        fields.insert("$type".to_string(), Value::String(type_tag.to_string()));
        fields.insert(
            "$variant".to_string(),
            Value::String(driver.variant_name().to_string()),
        );
        fields.insert("$tag".to_string(), Value::String(driver.tag().to_string()));

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

    fn serialize_value(value: MoveValue) -> Vec<u8> {
        value.undecorate().simple_serialize().unwrap()
    }

    fn struct_type(type_str: &str) -> StructTag {
        StructTag::from_str(type_str).unwrap()
    }

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

    fn make_value(type_str: &str, fields: Vec<(&str, MoveValue)>) -> MoveValue {
        MoveValue::Struct(A::MoveStruct {
            type_: struct_type(type_str),
            fields: fields
                .into_iter()
                .map(|(name, value)| (Identifier::new(name).unwrap(), value))
                .collect(),
        })
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
        let bytes = serialize_value(value);
        let mut visitor = JsonVisitor::new();
        let json = MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap();

        let expected = json!({
            "id": "0x0000000000000000000000000000000000000000000000000000000000000001",
            "balance": "1000"
        });
        assert_eq!(json, expected);
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
        let bytes = serialize_value(value);
        let mut visitor = JsonVisitor::new();
        let json = MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap();

        let expected = json!({
            "items": [
                {
                    "value": "10"
                },
                {
                    "value": "20"
                }
            ],
            "count": "2"
        });
        assert_eq!(json, expected);
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

        let bytes = serialize_value(some_value);
        let mut visitor = JsonVisitor::new();
        let json = MoveValue::visit_deserialize(&bytes, &enum_layout, &mut visitor).unwrap();

        let expected = json!({
            "$type": "0x1::option::Option<u64>",
            "$variant": "Some",
            "$tag": "1",
            "value": "42"
        });
        assert_eq!(json, expected);

        // Test "None" variant
        let none_value = V::Variant(A::MoveVariant {
            type_: struct_type("0x1::option::Option<u64>"),
            variant_name: Identifier::new("None").unwrap(),
            tag: 0,
            fields: vec![],
        });

        let bytes = serialize_value(none_value);
        let json = MoveValue::visit_deserialize(&bytes, &enum_layout, &mut visitor).unwrap();

        let expected_none = json!({
            "$type": "0x1::option::Option<u64>",
            "$variant": "None",
            "$tag": "0"
        });
        assert_eq!(json, expected_none);
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
        let bytes = serialize_value(value);
        let mut visitor = JsonVisitor::new();
        let json = MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap();

        assert_eq!(json["small_u64"], json!("1000"));
        assert_eq!(json["large_u64"], json!(u64::MAX.to_string()));
        assert_eq!(json["u128_value"], json!(u128::MAX.to_string()));
        assert_eq!(json["u256_value"], json!("123456789"));
    }
}
