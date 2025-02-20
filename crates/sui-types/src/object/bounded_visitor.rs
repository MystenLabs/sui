// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::bail;
use move_core_types::{
    account_address::AccountAddress,
    annotated_value as A,
    annotated_visitor::{self, StructDriver, ValueDriver, VecDriver, Visitor},
    language_storage::TypeTag,
    u256::U256,
};
use once_cell::sync::Lazy;
use tracing::info;

/// Visitor to deserialize annotated values or structs, bounding the size budgeted for types and
/// field names in the output. The visitor does not bound the size of values, because they are
/// assumed to already be bounded by execution.
pub struct BoundedVisitor {
    /// Budget left to spend on field names and types.
    bound: usize,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Visitor(#[from] annotated_visitor::Error),

    #[error("Deserialized value too large")]
    OutOfBudget,
}

/// Environment variable to override the default budget for deserialization. This can be set at
/// runtime to change the maximum size of values that can be deserialized.
const MAX_BOUND_VAR_NAME: &str = "MAX_ANNOTATED_VALUE_SIZE";

/// Default budget for deserialization -- we're okay to spend an extra ~1MiB on types and field
/// information per value.
const DEFAULT_MAX_BOUND: usize = 1024 * 1024;

/// Budget for deserialization into an annotated Move value. This sets the numbers of bytes that we
/// are willing to spend on field names, type names (etc) when deserializing a Move value into an
/// annotated Move value.
///
/// Bounded deserialization is intended for use outside of the validator, and so uses a fixed bound
/// that needs to be set at startup rather than one that is configured as part of the protocol.
///
/// If the environment variable `MAX_ANNOTATED_VALUE_SIZE` is unset we default to
/// `DEFAULT_MAX_BOUND` which allows ~1MiB additional space usage on types and field information
/// per value.
///
/// This is read only once and after that the value is cached. To change this value you will need
/// to restart the process with the new value set (or the value unset if you wish to use the
/// `DEFAULT_MAX_BOUND` value).
static MAX_BOUND: Lazy<usize> = Lazy::new(|| {
    let max_bound_opt = std::env::var(MAX_BOUND_VAR_NAME)
        .ok()
        .and_then(|s| s.parse().ok());
    if let Some(max_bound) = max_bound_opt {
        info!(
            "Using custom value for '{}' max bound: {}",
            MAX_BOUND_VAR_NAME, max_bound
        );
        max_bound
    } else {
        info!(
            "Using default value for '{}' -- max bound: {}",
            MAX_BOUND_VAR_NAME, DEFAULT_MAX_BOUND
        );
        DEFAULT_MAX_BOUND
    }
});

impl BoundedVisitor {
    fn new(bound: usize) -> Self {
        Self { bound }
    }

    /// Deserialize `bytes` as a `MoveValue` with layout `layout`. Can fail if the bytes do not
    /// represent a value with this layout, or if the deserialized value exceeds the field/type size
    /// budget.
    pub fn deserialize_value(
        bytes: &[u8],
        layout: &A::MoveTypeLayout,
    ) -> anyhow::Result<A::MoveValue> {
        let mut visitor = Self::default();
        A::MoveValue::visit_deserialize(bytes, layout, &mut visitor)
    }

    /// Deserialize `bytes` as a `MoveStruct` with layout `layout`. Can fail if the bytes do not
    /// represent a struct with this layout, or if the deserialized struct exceeds the field/type
    /// size budget.
    pub fn deserialize_struct(
        bytes: &[u8],
        layout: &A::MoveStructLayout,
    ) -> anyhow::Result<A::MoveStruct> {
        let mut visitor = Self::default();
        let A::MoveValue::Struct(struct_) =
            A::MoveStruct::visit_deserialize(bytes, layout, &mut visitor)?
        else {
            bail!("Expected to deserialize a struct");
        };
        Ok(struct_)
    }

    /// Deduct `size` from the overall budget. Errors if `size` exceeds the current budget.
    fn debit(&mut self, size: usize) -> Result<(), Error> {
        if self.bound < size {
            Err(Error::OutOfBudget)
        } else {
            self.bound -= size;
            Ok(())
        }
    }

    /// Deduct the estimated size of `tag` from the overall budget. Errors if its size exceeds the
    /// current budget. The estimated size is proportional to the representation of that type in
    /// memory, but does not match its exact size.
    fn debit_type_size(&mut self, tag: &TypeTag) -> Result<(), Error> {
        use TypeTag as TT;
        let mut frontier = vec![tag];
        while let Some(tag) = frontier.pop() {
            match tag {
                TT::Bool
                | TT::U8
                | TT::U16
                | TT::U32
                | TT::U64
                | TT::U128
                | TT::U256
                | TT::Address
                | TT::Signer => self.debit(8)?,

                TT::Vector(inner) => {
                    self.debit(8)?;
                    frontier.push(inner);
                }

                TT::Struct(tag) => {
                    self.debit(8 + AccountAddress::LENGTH + tag.module.len() + tag.name.len())?;
                    frontier.extend(tag.type_params.iter());
                }
            }
        }

        Ok(())
    }
}

impl<'b, 'l> Visitor<'b, 'l> for BoundedVisitor {
    type Value = A::MoveValue;
    type Error = Error;

    fn visit_u8(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u8,
    ) -> Result<Self::Value, Self::Error> {
        Ok(A::MoveValue::U8(value))
    }

    fn visit_u16(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u16,
    ) -> Result<Self::Value, Self::Error> {
        Ok(A::MoveValue::U16(value))
    }

    fn visit_u32(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u32,
    ) -> Result<Self::Value, Self::Error> {
        Ok(A::MoveValue::U32(value))
    }

    fn visit_u64(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u64,
    ) -> Result<Self::Value, Self::Error> {
        Ok(A::MoveValue::U64(value))
    }

    fn visit_u128(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: u128,
    ) -> Result<Self::Value, Self::Error> {
        Ok(A::MoveValue::U128(value))
    }

    fn visit_u256(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: U256,
    ) -> Result<Self::Value, Self::Error> {
        Ok(A::MoveValue::U256(value))
    }

    fn visit_bool(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: bool,
    ) -> Result<Self::Value, Self::Error> {
        Ok(A::MoveValue::Bool(value))
    }

    fn visit_address(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(A::MoveValue::Address(value))
    }

    fn visit_signer(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(A::MoveValue::Signer(value))
    }

    fn visit_vector(
        &mut self,
        driver: &mut VecDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let mut elems = vec![];
        while let Some(elem) = driver.next_element(self)? {
            elems.push(elem);
        }

        Ok(A::MoveValue::Vector(elems))
    }

    fn visit_struct(
        &mut self,
        driver: &mut StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let tag = driver.struct_layout().type_.clone().into();

        self.debit_type_size(&tag)?;
        for field in driver.struct_layout().fields.iter() {
            self.debit(field.name.len())?;
        }

        let mut fields = vec![];
        while let Some((field, elem)) = driver.next_field(self)? {
            fields.push((field.name.clone(), elem));
        }

        let TypeTag::Struct(type_) = tag else {
            unreachable!("SAFETY: tag was derived from a StructTag.");
        };

        Ok(A::MoveValue::Struct(A::MoveStruct {
            type_: *type_,
            fields,
        }))
    }

    fn visit_variant(
        &mut self,
        driver: &mut annotated_visitor::VariantDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let type_ = driver.enum_layout().type_.clone().into();

        self.debit_type_size(&type_)?;
        self.debit(driver.variant_name().len())?;

        for field in driver.variant_layout() {
            self.debit(field.name.len())?;
        }

        let mut fields = vec![];
        while let Some((field, elem)) = driver.next_field(self)? {
            fields.push((field.name.clone(), elem));
        }

        let TypeTag::Struct(type_) = type_ else {
            unreachable!("SAFETY: type_ was derived from a StructTag.");
        };

        Ok(A::MoveValue::Variant(A::MoveVariant {
            type_: *type_,
            fields,
            variant_name: driver.variant_name().to_owned(),
            tag: driver.tag(),
        }))
    }
}

impl Default for BoundedVisitor {
    fn default() -> Self {
        Self::new(*MAX_BOUND)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::str::FromStr;

    use super::*;

    use expect_test::expect;
    use move_core_types::{identifier::Identifier, language_storage::StructTag};

    #[test]
    fn test_success() {
        use A::MoveTypeLayout as T;
        use A::MoveValue as V;

        let type_layout = layout_(
            "0x0::foo::Bar",
            vec![
                ("a", T::U64),
                ("b", T::Vector(Box::new(T::U64))),
                ("c", layout_("0x0::foo::Baz", vec![("d", T::U64)])),
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

        let bytes = serialize(value.clone());

        let mut visitor = BoundedVisitor::new(1000);
        let deser = A::MoveValue::visit_deserialize(&bytes, &type_layout, &mut visitor).unwrap();
        assert_eq!(value, deser);
    }

    #[test]
    fn test_env_variable_override() {
        use A::MoveTypeLayout as T;
        use A::MoveValue as V;

        let type_layout = layout_(
            "0x0::foo::Bar",
            vec![
                ("a", T::U64),
                ("b", T::Vector(Box::new(T::U64))),
                ("c", layout_("0x0::foo::Baz", vec![("d", T::U64)])),
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

        let bytes = serialize(value.clone());

        let before_value = std::env::var(MAX_BOUND_VAR_NAME).ok();

        std::env::set_var(MAX_BOUND_VAR_NAME, "10");
        let mut visitor = BoundedVisitor::default();
        let err = A::MoveValue::visit_deserialize(&bytes, &type_layout, &mut visitor).unwrap_err();
        let expect = expect!["Deserialized value too large"];
        expect.assert_eq(&err.to_string());

        // Should be unaffected as we already set the value, so this should still fail.
        std::env::set_var(MAX_BOUND_VAR_NAME, "1000");
        let mut visitor = BoundedVisitor::default();
        let err = A::MoveValue::visit_deserialize(&bytes, &type_layout, &mut visitor).unwrap_err();
        let expect = expect!["Deserialized value too large"];
        expect.assert_eq(&err.to_string());

        // set the value back to what it was before if it was previously set, otherwise unset it.
        if let Some(previous_value) = before_value {
            std::env::set_var(MAX_BOUND_VAR_NAME, previous_value);
        } else {
            std::env::remove_var(MAX_BOUND_VAR_NAME);
        }

        // Should still fail as the static value is already set.
        let mut visitor = BoundedVisitor::default();
        let err = A::MoveValue::visit_deserialize(&bytes, &type_layout, &mut visitor).unwrap_err();
        let expect = expect!["Deserialized value too large"];
        expect.assert_eq(&err.to_string());
    }

    #[test]
    fn test_too_deep() {
        use A::MoveTypeLayout as T;
        use A::MoveValue as V;

        let mut layout = T::U64;
        let mut value = V::U64(42);

        const DEPTH: usize = 10;
        for _ in 0..DEPTH {
            layout = layout_("0x0::foo::Bar", vec![("f", layout)]);
            value = value_("0x0::foo::Bar", vec![("f", value)]);
        }

        let bound = DEPTH * (8 + 32 + "foo".len() + "Bar".len() + "f".len());
        let bytes = serialize(value.clone());

        let mut visitor = BoundedVisitor::new(bound);
        let deser = A::MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap();
        assert_eq!(deser, value);

        let mut visitor = BoundedVisitor::new(bound - 1);
        let err = A::MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap_err();

        let expect = expect!["Deserialized value too large"];
        expect.assert_eq(&err.to_string());
    }

    #[test]
    fn test_too_wide() {
        use A::MoveTypeLayout as T;
        use A::MoveValue as V;

        const WIDTH: usize = 10;
        let mut idents = vec![];
        let mut fields = vec![];
        let mut values = vec![];

        for i in 0..WIDTH {
            idents.push(format!("f{}", i));
        }

        for (i, id) in idents.iter().enumerate() {
            let layout = layout_("0x0::foo::Baz", vec![("f", T::U64)]);
            let value = value_("0x0::foo::Baz", vec![("f", V::U64(i as u64))]);

            fields.push((id.as_str(), layout));
            values.push((id.as_str(), value));
        }

        let layout = layout_("0x0::foo::Bar", fields);
        let value = value_("0x0::foo::Bar", values);

        let outer = 8 + 32 + "foo".len() + "Bar".len();
        let inner = WIDTH * ("fx".len() + 8 + 32 + "foo".len() + "Baz".len() + "f".len());
        let bound = outer + inner;

        let bytes = serialize(value.clone());

        let mut visitor = BoundedVisitor::new(bound);
        let deser = A::MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap();
        assert_eq!(deser, value);

        let mut visitor = BoundedVisitor::new(bound - 1);
        let err = A::MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap_err();

        let expect = expect!["Deserialized value too large"];
        expect.assert_eq(&err.to_string());
    }

    #[test]
    fn test_big_types() {
        use A::MoveTypeLayout as T;
        use A::MoveValue as V;

        let big_mod_ = "m".repeat(128);
        let big_name = "T".repeat(128);
        let big_type = format!("0x0::{big_mod_}::{big_name}");

        let layout = layout_(big_type.as_str(), vec![("f", T::U64)]);
        let value = value_(big_type.as_str(), vec![("f", V::U64(42))]);

        let bound = 8 + 32 + big_mod_.len() + big_name.len() + "f".len();
        let bytes = serialize(value.clone());

        let mut visitor = BoundedVisitor::new(bound);
        let deser = A::MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap();
        assert_eq!(deser, value);

        let mut visitor = BoundedVisitor::new(bound - 1);
        let err = A::MoveValue::visit_deserialize(&bytes, &layout, &mut visitor).unwrap_err();

        let expect = expect!["Deserialized value too large"];
        expect.assert_eq(&err.to_string());
    }

    type Variant<'s> = (&'s str, u16);
    type FieldLayout<'s> = (&'s str, A::MoveTypeLayout);

    fn ident_(name: &str) -> Identifier {
        Identifier::new(name).unwrap()
    }

    /// Create a struct value for test purposes.
    pub(crate) fn value_(rep: &str, fields: Vec<(&str, A::MoveValue)>) -> A::MoveValue {
        let type_ = StructTag::from_str(rep).unwrap();
        let fields = fields
            .into_iter()
            .map(|(name, value)| (ident_(name), value))
            .collect();

        A::MoveValue::Struct(A::MoveStruct::new(type_, fields))
    }

    /// Create a struct layout for test purposes.
    pub(crate) fn layout_(rep: &str, fields: Vec<FieldLayout<'_>>) -> A::MoveTypeLayout {
        let type_ = StructTag::from_str(rep).unwrap();
        let fields = fields
            .into_iter()
            .map(|(name, layout)| A::MoveFieldLayout::new(ident_(name), layout))
            .collect();

        A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout { type_, fields }))
    }

    /// Create a variant value for test purposes.
    pub(crate) fn variant_(
        rep: &str,
        name: &str,
        tag: u16,
        fields: Vec<(&str, A::MoveValue)>,
    ) -> A::MoveValue {
        let type_ = StructTag::from_str(rep).unwrap();
        let fields = fields
            .into_iter()
            .map(|(name, value)| (ident_(name), value))
            .collect();

        A::MoveValue::Variant(A::MoveVariant {
            type_,
            variant_name: ident_(name),
            tag,
            fields,
        })
    }

    /// Create an enum layout for test purposes.
    pub(crate) fn enum_(
        rep: &str,
        variants: Vec<(Variant<'_>, Vec<FieldLayout<'_>>)>,
    ) -> A::MoveTypeLayout {
        let type_ = StructTag::from_str(rep).unwrap();
        let variants = variants
            .into_iter()
            .map(|((name, tag), fields)| {
                let fields = fields
                    .into_iter()
                    .map(|(name, layout)| A::MoveFieldLayout::new(ident_(name), layout))
                    .collect();
                ((ident_(name), tag), fields)
            })
            .collect();

        A::MoveTypeLayout::Enum(Box::new(A::MoveEnumLayout { type_, variants }))
    }

    /// BCS encode Move value.
    fn serialize(value: A::MoveValue) -> Vec<u8> {
        value.clone().undecorate().simple_serialize().unwrap()
    }
}
