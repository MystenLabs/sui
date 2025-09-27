// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    balance::Balance,
    base_types::{move_ascii_str_layout, move_utf8_str_layout, RESOLVED_STD_OPTION},
    id::{ID, UID},
    SUI_FRAMEWORK_ADDRESS,
};
use anyhow::bail;
use move_core_types::{
    account_address::AccountAddress,
    annotated_value::{self as A, MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
    annotated_visitor::{self, StructDriver, ValueDriver, VariantDriver, VecDriver, Visitor},
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
    u256::U256,
};
use prost_types::value::Kind;
use prost_types::Struct;
use prost_types::Value;

/// This is the maximum depth of a proto message
/// The maximum depth of a proto message is 100. Given this value may be nested itself somewhere
/// we'll conservitively cap this to ~80% of that.
const MAX_DEPTH: usize = 80;

pub struct ProtoVisitorBuilder {
    /// Budget to spend on visiting.
    bound: usize,

    /// Current level of nesting depth while visiting.
    depth: usize,
}

struct ProtoVisitor<'a> {
    /// Budget left to spend on visiting.
    bound: &'a mut usize,

    /// Current level of nesting depth while visiting.
    depth: &'a mut usize,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Visitor(#[from] annotated_visitor::Error),

    #[error("Deserialized value too large")]
    OutOfBudget,

    #[error("Exceeded maximum depth")]
    TooNested,

    #[error("Unexpected type")]
    UnexpectedType,
}

impl ProtoVisitorBuilder {
    pub fn new(bound: usize) -> Self {
        Self { bound, depth: 0 }
    }

    fn new_visitor(&mut self) -> Result<ProtoVisitor<'_>, Error> {
        ProtoVisitor::new(&mut self.bound, &mut self.depth)
    }

    /// Deserialize `bytes` as a `MoveValue` with layout `layout`. Can fail if the bytes do not
    /// represent a value with this layout, or if the deserialized value exceeds the field/type size
    /// budget.
    pub fn deserialize_value(
        mut self,
        bytes: &[u8],
        layout: &A::MoveTypeLayout,
    ) -> anyhow::Result<Value> {
        let mut visitor = self.new_visitor()?;

        A::MoveValue::visit_deserialize(bytes, layout, &mut visitor)
    }

    /// Deserialize `bytes` as a `MoveStruct` with layout `layout`. Can fail if the bytes do not
    /// represent a struct with this layout, or if the deserialized struct exceeds the field/type
    /// size budget.
    pub fn deserialize_struct(
        mut self,
        bytes: &[u8],
        layout: &A::MoveStructLayout,
    ) -> anyhow::Result<Struct> {
        let mut visitor = self.new_visitor()?;

        let Value {
            kind: Some(Kind::StructValue(struct_)),
        } = A::MoveStruct::visit_deserialize(bytes, layout, &mut visitor)?
        else {
            bail!("Expected to deserialize a struct");
        };
        Ok(struct_)
    }
}

impl Drop for ProtoVisitor<'_> {
    fn drop(&mut self) {
        self.dec_depth();
    }
}

impl<'a> ProtoVisitor<'a> {
    fn new(bound: &'a mut usize, depth: &'a mut usize) -> Result<Self, Error> {
        // Increment the depth since we're creating a new Visitor instance
        Self::inc_depth(depth)?;
        Ok(Self { bound, depth })
    }

    fn inc_depth(depth: &mut usize) -> Result<(), Error> {
        if *depth > MAX_DEPTH {
            Err(Error::TooNested)
        } else {
            *depth += 1;
            Ok(())
        }
    }

    fn dec_depth(&mut self) {
        if *self.depth == 0 {
            panic!("BUG: logic bug in Visitor implementation");
        } else {
            *self.depth -= 1;
        }
    }

    /// Deduct `size` from the overall budget. Errors if `size` exceeds the current budget.
    fn debit(&mut self, size: usize) -> Result<(), Error> {
        if *self.bound < size {
            Err(Error::OutOfBudget)
        } else {
            *self.bound -= size;
            Ok(())
        }
    }

    fn debit_value(&mut self) -> Result<(), Error> {
        self.debit(size_of::<Value>())
    }

    fn debit_string_value(&mut self, s: &str) -> Result<(), Error> {
        self.debit_str(s)?;
        self.debit_value()
    }

    fn debit_str(&mut self, s: &str) -> Result<(), Error> {
        self.debit(s.len())
    }
}

impl<'b, 'l> Visitor<'b, 'l> for ProtoVisitor<'_> {
    type Value = Value;
    type Error = Error;

    fn visit_u8(&mut self, _: &ValueDriver<'_, 'b, 'l>, value: u8) -> Result<Value, Error> {
        self.debit_value()?;
        Ok(Value::from(value))
    }

    fn visit_u16(&mut self, _: &ValueDriver<'_, 'b, 'l>, value: u16) -> Result<Value, Error> {
        self.debit_value()?;
        Ok(Value::from(value))
    }

    fn visit_u32(&mut self, _: &ValueDriver<'_, 'b, 'l>, value: u32) -> Result<Value, Error> {
        self.debit_value()?;
        Ok(Value::from(value))
    }

    fn visit_u64(&mut self, _: &ValueDriver<'_, 'b, 'l>, value: u64) -> Result<Value, Error> {
        let value = value.to_string();
        self.debit_string_value(&value)?;
        Ok(Value::from(value))
    }

    fn visit_u128(&mut self, _: &ValueDriver<'_, 'b, 'l>, value: u128) -> Result<Value, Error> {
        let value = value.to_string();
        self.debit_string_value(&value)?;
        Ok(Value::from(value))
    }

    fn visit_u256(&mut self, _: &ValueDriver<'_, 'b, 'l>, value: U256) -> Result<Value, Error> {
        let value = value.to_string();
        self.debit_string_value(&value)?;
        Ok(Value::from(value))
    }

    fn visit_bool(&mut self, _: &ValueDriver<'_, 'b, 'l>, value: bool) -> Result<Value, Error> {
        self.debit_value()?;
        Ok(Value::from(value))
    }

    fn visit_address(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Value, Error> {
        let value = value.to_canonical_string(true);
        self.debit_string_value(&value)?;
        Ok(Value::from(value))
    }

    fn visit_signer(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Value, Error> {
        let value = value.to_canonical_string(true);
        self.debit_string_value(&value)?;
        Ok(Value::from(value))
    }

    fn visit_vector(&mut self, driver: &mut VecDriver<'_, 'b, 'l>) -> Result<Value, Error> {
        let value = if driver.element_layout().is_type(&TypeTag::U8) {
            // Base64 encode arbitrary bytes
            use base64::{engine::general_purpose::STANDARD, Engine};

            if let Some(bytes) = driver
                .bytes()
                .get(driver.position()..(driver.position() + driver.len() as usize))
            {
                let b64 = STANDARD.encode(bytes);
                self.debit_string_value(&b64)?;
                Value::from(b64)
            } else {
                return Err(Error::Visitor(annotated_visitor::Error::UnexpectedEof));
            }
        } else {
            let mut elems = vec![];
            self.debit_value()?;

            while let Some(elem) =
                driver.next_element(&mut ProtoVisitor::new(self.bound, self.depth)?)?
            {
                elems.push(elem);
            }

            Value::from(elems)
        };

        Ok(value)
    }

    fn visit_struct(&mut self, driver: &mut StructDriver<'_, 'b, 'l>) -> Result<Value, Error> {
        let ty = &driver.struct_layout().type_;
        let layout = driver.struct_layout();

        let value = if layout == &move_ascii_str_layout()
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
            self.debit_string_value(s)?;
            Value::from(s)
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

            self.debit_string_value(&id)?;
            Value::from(id)
        } else if (&ty.address, ty.module.as_ref(), ty.name.as_ref()) == RESOLVED_STD_OPTION {
            // 0x1::option::Option
            self.debit_value()?;
            match OptionVisitor(self).visit_struct(driver)? {
                Some(value) => value,
                None => Kind::NullValue(0).into(),
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
            self.debit_string_value(&balance)?;
            Value::from(balance)
        } else {
            // Arbitrary structs
            let mut map = Struct::default();

            self.debit_value()?;
            for field in &driver.struct_layout().fields {
                self.debit_str(field.name.as_str())?;
            }

            while let Some((field, elem)) =
                driver.next_field(&mut ProtoVisitor::new(self.bound, self.depth)?)?
            {
                map.fields.insert(field.name.as_str().to_owned(), elem);
            }
            Value::from(Kind::StructValue(map))
        };
        Ok(value)
    }

    fn visit_variant(&mut self, driver: &mut VariantDriver<'_, 'b, 'l>) -> Result<Value, Error> {
        let mut map = Struct::default();
        self.debit_value()?;

        self.debit_str("@variant")?;
        self.debit_string_value(driver.variant_name().as_str())?;

        map.fields
            .insert("@variant".to_owned(), driver.variant_name().as_str().into());

        for field in driver.variant_layout() {
            self.debit_str(field.name.as_str())?;
        }

        while let Some((field, elem)) =
            driver.next_field(&mut ProtoVisitor::new(self.bound, self.depth)?)?
        {
            map.fields.insert(field.name.as_str().to_owned(), elem);
        }

        Ok(Value::from(Kind::StructValue(map)))
    }
}

struct OptionVisitor<'a, T>(&'a mut T);

impl<'b, 'l, T> Visitor<'b, 'l> for OptionVisitor<'_, T>
where
    T: Visitor<'b, 'l, Error = Error>,
{
    type Value = Option<T::Value>;
    type Error = Error;

    fn visit_vector(&mut self, driver: &mut VecDriver<'_, 'b, 'l>) -> Result<Self::Value, Error> {
        match driver.len() {
            0 => Ok(None),
            1 => driver.next_element(self.0),
            _ => Err(Error::UnexpectedType),
        }
    }

    fn visit_struct(
        &mut self,
        driver: &mut StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Error> {
        if is_option(driver.struct_layout()) {
            driver
                .next_field(self)?
                .ok_or(Error::UnexpectedType)
                .map(|(_, option)| option)
        } else {
            Err(Error::UnexpectedType)
        }
    }

    // === Empty/default casees ===

    fn visit_u8(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: u8) -> Result<Self::Value, Error> {
        Err(Error::UnexpectedType)
    }

    fn visit_u16(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: u16) -> Result<Self::Value, Error> {
        Err(Error::UnexpectedType)
    }

    fn visit_u32(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: u32) -> Result<Self::Value, Error> {
        Err(Error::UnexpectedType)
    }

    fn visit_u64(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: u64) -> Result<Self::Value, Error> {
        Err(Error::UnexpectedType)
    }

    fn visit_u128(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: u128) -> Result<Self::Value, Error> {
        Err(Error::UnexpectedType)
    }

    fn visit_u256(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: U256) -> Result<Self::Value, Error> {
        Err(Error::UnexpectedType)
    }

    fn visit_bool(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: bool) -> Result<Self::Value, Error> {
        Err(Error::UnexpectedType)
    }

    fn visit_address(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: AccountAddress,
    ) -> Result<Self::Value, Error> {
        Err(Error::UnexpectedType)
    }

    fn visit_signer(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: AccountAddress,
    ) -> Result<Self::Value, Error> {
        Err(Error::UnexpectedType)
    }

    fn visit_variant(&mut self, _: &mut VariantDriver<'_, 'b, 'l>) -> Result<Self::Value, Error> {
        Err(Error::UnexpectedType)
    }
}

fn is_option(struct_layout: &MoveStructLayout) -> bool {
    let ty = &struct_layout.type_;

    if (&ty.address, ty.module.as_ref(), ty.name.as_ref()) != RESOLVED_STD_OPTION {
        return false;
    }

    if ty.type_params.len() != 1 {
        return false;
    }

    let Some(type_param) = ty.type_params.first() else {
        return false;
    };

    if struct_layout.fields.len() != 1 {
        return false;
    }

    let Some(field) = struct_layout.fields.first() else {
        return false;
    };

    if field.name.as_str() != "vec" {
        return false;
    }

    match &field.layout {
        MoveTypeLayout::Vector(elem) => {
            if !elem.is_type(type_param) {
                return false;
            }
        }
        _ => return false,
    }

    true
}

pub const URL_MODULE_NAME: &IdentStr = ident_str!("url");
pub const URL_STRUCT_NAME: &IdentStr = ident_str!("Url");

fn url_layout() -> MoveStructLayout {
    MoveStructLayout {
        type_: StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: URL_MODULE_NAME.to_owned(),
            name: URL_STRUCT_NAME.to_owned(),
            type_params: vec![],
        },
        fields: vec![MoveFieldLayout::new(
            ident_str!("url").to_owned(),
            MoveTypeLayout::Struct(Box::new(move_ascii_str_layout())),
        )],
    }
}

fn is_balance(struct_layout: &MoveStructLayout) -> bool {
    let ty = &struct_layout.type_;

    if !Balance::is_balance(ty) {
        return false;
    }

    if ty.type_params.len() != 1 {
        return false;
    }

    if struct_layout.fields.len() != 1 {
        return false;
    }

    let Some(field) = struct_layout.fields.first() else {
        return false;
    };

    if field.name.as_str() != "value" {
        return false;
    }

    if !matches!(field.layout, MoveTypeLayout::U64) {
        return false;
    }

    true
}

#[cfg(test)]
pub(crate) mod tests {
    use std::str::FromStr;

    use super::*;

    use crate::object::bounded_visitor::tests::layout_;
    use crate::object::bounded_visitor::tests::serialize;
    use crate::object::bounded_visitor::tests::value_;
    use expect_test::expect;
    use move_core_types::annotated_value::MoveFieldLayout;
    use move_core_types::annotated_value::MoveStructLayout;
    use move_core_types::{ident_str, language_storage::StructTag};
    use serde_json::json;

    use A::MoveTypeLayout as L;
    use A::MoveValue as V;

    #[test]
    fn test_simple() {
        let type_layout = layout_(
            "0x0::foo::Bar",
            vec![
                ("a", L::U64),
                ("b", L::Vector(Box::new(L::U64))),
                ("c", layout_("0x0::foo::Baz", vec![("d", L::U64)])),
            ],
        );

        let value = value_(
            "0x0::foo::Bar",
            vec![
                ("a", V::U64(42)),
                ("b", V::Vector(vec![V::U64(43)])),
                ("c", value_("0x0::foo::Baz", vec![("d", V::U64(44))])),
            ],
        );

        let expected = json!({
            "a": "42",
            "b": ["43"],
            "c": {
                "d": "44"
            }
        });
        let bound = required_budget(&expected);

        let bytes = serialize(value.clone());

        let deser = ProtoVisitorBuilder::new(bound)
            .deserialize_value(&bytes, &type_layout)
            .unwrap();

        assert_eq!(expected, proto_value_to_json_value(deser));

        ProtoVisitorBuilder::new(bound - 1)
            .deserialize_value(&bytes, &type_layout)
            .unwrap_err();
    }

    #[test]
    fn test_too_deep() {
        let mut layout = L::U64;
        let mut value = V::U64(42);
        let mut expected = serde_json::Value::from("42");

        const DEPTH: usize = MAX_DEPTH;
        for _ in 0..DEPTH {
            layout = layout_("0x0::foo::Bar", vec![("f", layout)]);
            value = value_("0x0::foo::Bar", vec![("f", value)]);
            expected = json!({
                "f": expected
            });
        }

        let bound = required_budget(&expected);
        let bytes = serialize(value.clone());

        let deser = ProtoVisitorBuilder::new(bound)
            .deserialize_value(&bytes, &layout)
            .unwrap();

        assert_eq!(expected, proto_value_to_json_value(deser));

        // One deeper
        layout = layout_("0x0::foo::Bar", vec![("f", layout)]);
        value = value_("0x0::foo::Bar", vec![("f", value)]);

        let bytes = serialize(value.clone());

        let err = ProtoVisitorBuilder::new(bound)
            .deserialize_value(&bytes, &layout)
            .unwrap_err();

        let expect = expect!["Exceeded maximum depth"];
        expect.assert_eq(&err.to_string());
    }

    fn proto_value_to_json_value(proto: Value) -> serde_json::Value {
        match proto.kind {
            Some(Kind::NullValue(_)) | None => serde_json::Value::Null,
            // Move doesn't support floats so for these tests can do a convert to u32
            Some(Kind::NumberValue(n)) => serde_json::Value::from(n as u32),
            Some(Kind::StringValue(s)) => serde_json::Value::from(s),
            Some(Kind::BoolValue(b)) => serde_json::Value::from(b),
            Some(Kind::StructValue(map)) => serde_json::Value::Object(
                map.fields
                    .into_iter()
                    .map(|(k, v)| (k, proto_value_to_json_value(v)))
                    .collect(),
            ),
            Some(Kind::ListValue(list_value)) => serde_json::Value::Array(
                list_value
                    .values
                    .into_iter()
                    .map(proto_value_to_json_value)
                    .collect(),
            ),
        }
    }

    fn required_budget(json: &serde_json::Value) -> usize {
        size_of::<Value>()
            + match json {
                serde_json::Value::Null => 0,
                serde_json::Value::Bool(_) => 0,
                serde_json::Value::Number(_) => 0,
                serde_json::Value::String(s) => s.len(),
                serde_json::Value::Array(vec) => vec.iter().map(required_budget).sum(),
                serde_json::Value::Object(map) => {
                    map.iter().map(|(k, v)| k.len() + required_budget(v)).sum()
                }
            }
    }

    //
    // Tests for proper format rendering
    //

    fn json<T: serde::Serialize>(layout: A::MoveTypeLayout, data: T) -> serde_json::Value {
        let bcs = bcs::to_bytes(&data).unwrap();
        let proto_value = ProtoVisitorBuilder::new(1024 * 1024)
            .deserialize_value(&bcs, &layout)
            .unwrap();
        proto_value_to_json_value(proto_value)
    }

    macro_rules! struct_layout {
        ($type:literal { $($name:literal : $layout:expr),* $(,)?}) => {
            A::MoveTypeLayout::Struct(Box::new(MoveStructLayout {
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

    fn address(a: &str) -> sui_sdk_types::Address {
        sui_sdk_types::Address::from_str(a).unwrap()
    }

    #[test]
    fn json_bool() {
        let actual = json(L::Bool, true);
        let expect = json!(true);
        assert_eq!(expect, actual);

        let actual = json(L::Bool, false);
        let expect = json!(false);
        assert_eq!(expect, actual);
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
        let l = struct_layout!("0x1::ascii::String" {
            "bytes": vector_layout!(L::U8)
        });
        let actual = json(l, "The quick brown fox");
        let expect = json!("The quick brown fox");
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_utf8_string() {
        let l = struct_layout!("0x1::string::String" {
            "bytes": vector_layout!(L::U8)
        });
        let actual = json(l, "The quick brown fox");
        let expect = json!("The quick brown fox");
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_url() {
        let l = struct_layout!("0x2::url::Url" {
            "url": struct_layout!("0x1::ascii::String" {
                "bytes": vector_layout!(L::U8)
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
        let l = struct_layout!("0x2::object::ID" {
            "bytes": L::Address,
        });
        let actual = json(l, address("0x42"));
        let expect = json!(address("0x42").to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_uid() {
        let l = struct_layout!("0x2::object::UID" {
            "id": struct_layout!("0x2::object::ID" {
                "bytes": L::Address,
            })
        });
        let actual = json(l, address("0x42"));
        let expect = json!(address("0x42").to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_option() {
        let l = struct_layout!("0x42::foo::Bar" {
            "baz": struct_layout!("0x1::option::Option<u8>" { "vec": vector_layout!(L::U8) }),
        });

        let actual = json(l, Option::<Vec<u8>>::None);
        let expect = json!({
            "baz": null,
        });
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_balance() {
        let l = struct_layout!("0x2::balance::Balance<0x2::sui::SUI>" {
            "value": L::U64,
        });

        let actual = json(l, 100u64);
        let expect = json!(100u64.to_string());
        assert_eq!(expect, actual);
    }

    #[test]
    fn json_compound() {
        let l = struct_layout!("0x42::foo::Bar" {
            "baz": struct_layout!("0x1::option::Option<u8>" { "vec": vector_layout!(L::U8) }),
            "qux": vector_layout!(struct_layout!("0x43::xy::Zzy" {
                "quy": L::U16,
                "quz": struct_layout!("0x1::option::Option<0x1::ascii::String>" {
                    "vec": vector_layout!(struct_layout!("0x1::ascii::String" {
                        "bytes": vector_layout!(L::U8),
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
