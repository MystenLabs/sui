// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    account_address::AccountAddress,
    annotated_value as A,
    annotated_visitor::{self, StructDriver, ValueDriver, VariantDriver, VecDriver, Visitor},
    language_storage::TypeTag,
    u256::U256,
};

use crate::{base_types::ObjectID, id::UID};

use super::{DynamicFieldInfo, DynamicFieldType};

/// Visitor to deserialize the outer structure of a `0x2::dynamic_field::Field` while leaving its
/// name and value untouched.
pub struct FieldVisitor;

#[derive(Debug, Clone)]
pub struct Field<'b, 'l> {
    pub id: ObjectID,
    pub kind: DynamicFieldType,
    pub name_layout: &'l A::MoveTypeLayout,
    pub name_bytes: &'b [u8],
    pub value_layout: &'l A::MoveTypeLayout,
    pub value_bytes: &'b [u8],
}

pub enum ValueMetadata {
    DynamicField(TypeTag),
    DynamicObjectField(ObjectID),
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Not a dynamic field")]
    NotADynamicField,

    #[error("Not a dynamic object field")]
    NotADynamicObjectField,

    #[error("{0}")]
    Visitor(#[from] annotated_visitor::Error),
}

impl FieldVisitor {
    /// Deserialize the top-level structure from a dynamic field's `0x2::dynamic_field::Field`
    /// without having to fully deserialize its name or value.
    pub fn deserialize<'b, 'l>(
        bytes: &'b [u8],
        layout: &'l A::MoveTypeLayout,
    ) -> anyhow::Result<Field<'b, 'l>> {
        A::MoveValue::visit_deserialize(bytes, layout, &mut FieldVisitor)
    }
}

impl<'b, 'l> Field<'b, 'l> {
    /// If this field is a dynamic field, returns its value's type. If it is a dynamic object
    /// field, it returns the ID of the object the value points to (which must be fetched to
    /// extract its type).
    pub fn value_metadata(&self) -> Result<ValueMetadata, Error> {
        match self.kind {
            DynamicFieldType::DynamicField => Ok(ValueMetadata::DynamicField(TypeTag::from(
                self.value_layout,
            ))),

            DynamicFieldType::DynamicObject => {
                let id: ObjectID =
                    bcs::from_bytes(self.value_bytes).map_err(|_| Error::NotADynamicObjectField)?;
                Ok(ValueMetadata::DynamicObjectField(id))
            }
        }
    }
}

impl<'b, 'l> Visitor<'b, 'l> for FieldVisitor {
    type Value = Field<'b, 'l>;
    type Error = Error;

    fn visit_struct(
        &mut self,
        driver: &mut StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Error> {
        if !DynamicFieldInfo::is_dynamic_field(&driver.struct_layout().type_) {
            return Err(Error::NotADynamicField);
        }

        // Set-up optionals to fill while visiting fields -- all of them must be filled by the end
        // to successfully return a `Field`.
        let mut id = None;
        let mut name_parts = None;
        let mut value_parts = None;

        while let Some(A::MoveFieldLayout { name, layout }) = driver.peek_field() {
            match name.as_str() {
                "id" => {
                    let lo = driver.position();
                    driver.skip_field()?;
                    let hi = driver.position();

                    if !matches!(layout, A::MoveTypeLayout::Struct(s) if s.as_ref() == &UID::layout())
                    {
                        return Err(Error::NotADynamicField);
                    }

                    // HACK: Bypassing `id`'s layout to deserialize its bytes as a Rust type.
                    let bytes = &driver.bytes()[lo..hi];
                    id = Some(ObjectID::from_bytes(bytes).map_err(|_| Error::NotADynamicField)?);
                }

                "name" => {
                    let lo = driver.position();
                    driver.skip_field()?;
                    let hi = driver.position();

                    let (kind, layout) = extract_name_layout(layout)?;
                    name_parts = Some((&driver.bytes()[lo..hi], layout, kind));
                }

                "value" => {
                    let lo = driver.position();
                    driver.skip_field()?;
                    let hi = driver.position();
                    value_parts = Some((&driver.bytes()[lo..hi], layout));
                }

                _ => {
                    return Err(Error::NotADynamicField);
                }
            }
        }

        let (Some(id), Some((name_bytes, name_layout, kind)), Some((value_bytes, value_layout))) =
            (id, name_parts, value_parts)
        else {
            return Err(Error::NotADynamicField);
        };

        Ok(Field {
            id,
            kind,
            name_layout,
            name_bytes,
            value_layout,
            value_bytes,
        })
    }

    // === Empty/default casees ===
    //
    // A dynamic field must be a struct, so if the visitor is fed anything else, it complains.

    fn visit_u8(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: u8) -> Result<Self::Value, Error> {
        Err(Error::NotADynamicField)
    }

    fn visit_u16(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: u16) -> Result<Self::Value, Error> {
        Err(Error::NotADynamicField)
    }

    fn visit_u32(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: u32) -> Result<Self::Value, Error> {
        Err(Error::NotADynamicField)
    }

    fn visit_u64(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: u64) -> Result<Self::Value, Error> {
        Err(Error::NotADynamicField)
    }

    fn visit_u128(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: u128) -> Result<Self::Value, Error> {
        Err(Error::NotADynamicField)
    }

    fn visit_u256(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: U256) -> Result<Self::Value, Error> {
        Err(Error::NotADynamicField)
    }

    fn visit_bool(&mut self, _: &ValueDriver<'_, 'b, 'l>, _: bool) -> Result<Self::Value, Error> {
        Err(Error::NotADynamicField)
    }

    fn visit_address(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: AccountAddress,
    ) -> Result<Self::Value, Error> {
        Err(Error::NotADynamicField)
    }

    fn visit_signer(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: AccountAddress,
    ) -> Result<Self::Value, Error> {
        Err(Error::NotADynamicField)
    }

    fn visit_vector(&mut self, _: &mut VecDriver<'_, 'b, 'l>) -> Result<Self::Value, Error> {
        Err(Error::NotADynamicField)
    }

    fn visit_variant(&mut self, _: &mut VariantDriver<'_, 'b, 'l>) -> Result<Self::Value, Error> {
        Err(Error::NotADynamicField)
    }
}

/// Extract the type and layout of a dynamic field name, from the layout of its `Field.name`.
fn extract_name_layout(
    layout: &A::MoveTypeLayout,
) -> Result<(DynamicFieldType, &A::MoveTypeLayout), Error> {
    let A::MoveTypeLayout::Struct(struct_) = layout else {
        return Ok((DynamicFieldType::DynamicField, layout));
    };

    if !DynamicFieldInfo::is_dynamic_object_field_wrapper(&struct_.type_) {
        return Ok((DynamicFieldType::DynamicField, layout));
    }

    // Wrapper contains just one field
    let [A::MoveFieldLayout { name, layout }] = &struct_.fields[..] else {
        return Err(Error::NotADynamicField);
    };

    // ...called `name`
    if name.as_str() != "name" {
        return Err(Error::NotADynamicField);
    }

    Ok((DynamicFieldType::DynamicObject, layout))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use move_core_types::{
        account_address::AccountAddress, annotated_value as A, language_storage::TypeTag,
    };

    use crate::{
        base_types::ObjectID,
        dynamic_field,
        id::UID,
        object::bounded_visitor::tests::{enum_, layout_, value_, variant_},
    };

    use super::*;

    #[test]
    fn test_dynamic_field_name() {
        for (name, name_layout, name_bcs) in fixtures() {
            for (value, value_layout, value_bcs) in fixtures() {
                let df = serialized_df("0x264", name.clone(), value.clone());
                let df_layout = df_layout(name_layout.clone(), value_layout.clone());
                let field = FieldVisitor::deserialize(&df, &df_layout)
                    .unwrap_or_else(|e| panic!("Failed to deserialize {name} => {value}: {e}"));

                assert_eq!(field.id, oid_("0x264"), "{name} => {value}");
                assert_eq!(field.name_bytes, &name_bcs, "{name} => {value}");
                assert_eq!(field.value_bytes, &value_bcs, "{name} => {value}");

                assert_eq!(
                    field.kind,
                    DynamicFieldType::DynamicField,
                    "{name} => {value}",
                );

                assert_eq!(
                    TypeTag::from(field.name_layout),
                    TypeTag::from(&name_layout),
                    "{name} => {value}",
                );

                assert_eq!(
                    TypeTag::from(field.value_layout),
                    TypeTag::from(&value_layout),
                    "{name} => {value}",
                );
            }
        }
    }

    #[test]
    fn test_dynamic_object_field_name() {
        let addr = A::MoveValue::Address(AccountAddress::ONE);
        let id = value_("0x2::object::ID", vec![("bytes", addr)]);
        let id_bcs = id.clone().undecorate().simple_serialize().unwrap();

        for (name, name_layout, name_bcs) in fixtures() {
            let df = serialized_df("0x264", name.clone(), id.clone());
            let df_layout = dof_layout(name_layout.clone());
            let field = FieldVisitor::deserialize(&df, &df_layout)
                .unwrap_or_else(|e| panic!("Failed to deserialize {name}: {e}"));

            assert_eq!(field.id, oid_("0x264"), "{name}");
            assert_eq!(field.name_bytes, &name_bcs, "{name}");
            assert_eq!(field.value_bytes, &id_bcs, "{name}");

            assert_eq!(field.kind, DynamicFieldType::DynamicObject, "{name}",);

            assert_eq!(
                TypeTag::from(field.name_layout),
                TypeTag::from(&name_layout),
                "{name}",
            );

            assert_eq!(
                TypeTag::from(field.value_layout),
                TypeTag::from(&id_layout()),
                "{name}",
            );
        }
    }

    #[test]
    fn test_name_from_not_dynamic_field() {
        for (value, layout, bytes) in fixtures() {
            let Err(e) = FieldVisitor::deserialize(&bytes, &layout) else {
                panic!("Expected NotADynamicField error for {value}");
            };

            assert_eq!(
                e.to_string(),
                "Not a dynamic field",
                "Unexpected error for {value}"
            );
        }
    }

    /// If the visitor is run over a type that isn't actually a `0x2::dynamic_field::Field`, it
    /// will complain.
    #[test]
    fn test_from_bad_type() {
        for (value, layout, bytes) in fixtures() {
            let Err(e) = FieldVisitor::deserialize(&bytes, &layout) else {
                panic!("Expected NotADynamicField error for {value}");
            };

            assert_eq!(
                e.to_string(),
                "Not a dynamic field",
                "Unexpected error for {value}"
            );
        }
    }

    #[test]
    fn test_from_dynamic_field_missing_id() {
        let bytes = bcs::to_bytes(&(42u8, 43u8)).unwrap();
        let layout = layout_(
            "0x2::dynamic_field::Field<u8, u8>",
            vec![
                ("name", A::MoveTypeLayout::U8),
                ("value", A::MoveTypeLayout::U8),
            ],
        );

        let Err(e) = FieldVisitor::deserialize(&bytes, &layout) else {
            panic!("Expected NotADynamicField error");
        };

        assert_eq!(e.to_string(), "Not a dynamic field");
    }

    #[test]
    fn test_from_dynamic_field_missing_name() {
        let bytes = bcs::to_bytes(&(oid_("0x264"), 43u8)).unwrap();
        let layout = layout_(
            "0x2::dynamic_field::Field<u8, u8>",
            vec![("id", id_layout()), ("value", A::MoveTypeLayout::U8)],
        );

        let Err(e) = FieldVisitor::deserialize(&bytes, &layout) else {
            panic!("Expected NotADynamicField error");
        };

        assert_eq!(e.to_string(), "Not a dynamic field");
    }

    #[test]
    fn test_from_dynamic_field_missing_value() {
        let bytes = bcs::to_bytes(&(oid_("0x264"), 42u8)).unwrap();
        let layout = layout_(
            "0x2::dynamic_field::Field<u8, u8>",
            vec![("id", id_layout()), ("name", A::MoveTypeLayout::U8)],
        );

        let Err(e) = FieldVisitor::deserialize(&bytes, &layout) else {
            panic!("Expected NotADynamicField error");
        };

        assert_eq!(e.to_string(), "Not a dynamic field");
    }

    #[test]
    fn test_from_dynamic_field_weird_id() {
        let bytes = bcs::to_bytes(&(42u8, 43u8, 44u8)).unwrap();
        let layout = layout_(
            "0x2::dynamic_field::Field<u8, u8>",
            vec![
                ("id", A::MoveTypeLayout::U8),
                ("name", A::MoveTypeLayout::U8),
                ("value", A::MoveTypeLayout::U8),
            ],
        );

        let Err(e) = FieldVisitor::deserialize(&bytes, &layout) else {
            panic!("Expected NotADynamicField error");
        };

        assert_eq!(e.to_string(), "Not a dynamic field");
    }

    /// If the name is wrapped in `0x2::dynamic_object_field::Wrapper`, but the wrapper's structure
    /// is somehow incorrect, that will result in an error.
    #[test]
    fn test_from_dynamic_object_field_bad_wrapper() {
        let bytes = bcs::to_bytes(&(oid_("0x264"), 42u8)).unwrap();
        let layout = layout_(
            "0x2::dynamic_field::Field<0x2::dynamic_object_field::Wrapper<u8>, u8>",
            vec![
                ("id", id_layout()),
                (
                    "name",
                    layout_(
                        "0x2::dynamic_object_field::Wrapper<u8>",
                        // In the real type, the field is called "name"
                        vec![("wrapped", A::MoveTypeLayout::U8)],
                    ),
                ),
                ("value", A::MoveTypeLayout::U8),
            ],
        );

        let Err(e) = FieldVisitor::deserialize(&bytes, &layout) else {
            panic!("Expected NotADynamicField error");
        };

        assert_eq!(e.to_string(), "Not a dynamic field");
    }

    /// Various Move values to use as dynamic field names and values.
    fn fixtures() -> Vec<(A::MoveValue, A::MoveTypeLayout, Vec<u8>)> {
        use A::MoveTypeLayout as T;
        use A::MoveValue as V;

        vec![
            fixture(V::U8(42), T::U8),
            fixture(V::Address(AccountAddress::ONE), T::Address),
            fixture(
                V::Vector(vec![V::U32(43), V::U32(44), V::U32(45)]),
                T::Vector(Box::new(T::U32)),
            ),
            fixture(
                value_(
                    "0x2::object::ID",
                    vec![("bytes", V::Address(AccountAddress::TWO))],
                ),
                layout_("0x2::object::ID", vec![("bytes", T::Address)]),
            ),
            fixture(
                variant_(
                    "0x1::option::Option<u64>",
                    "Some",
                    1,
                    vec![("value", V::U64(46))],
                ),
                enum_(
                    "0x1::option::Option<u64>",
                    vec![
                        (("None", 0), vec![]),
                        (("Some", 1), vec![("value", T::U64)]),
                    ],
                ),
            ),
        ]
    }

    fn fixture(
        value: A::MoveValue,
        layout: A::MoveTypeLayout,
    ) -> (A::MoveValue, A::MoveTypeLayout, Vec<u8>) {
        let bytes = value
            .clone()
            .undecorate()
            .simple_serialize()
            .unwrap_or_else(|| panic!("Failed to serialize {}", value.clone()));

        (value, layout, bytes)
    }

    fn oid_(rep: &str) -> ObjectID {
        ObjectID::from_str(rep).unwrap()
    }

    fn serialized_df(id: &str, name: A::MoveValue, value: A::MoveValue) -> Vec<u8> {
        bcs::to_bytes(&dynamic_field::Field {
            id: UID::new(oid_(id)),
            name: name.undecorate(),
            value: value.undecorate(),
        })
        .unwrap()
    }

    fn id_layout() -> A::MoveTypeLayout {
        let addr = A::MoveTypeLayout::Address;
        layout_("0x2::object::ID", vec![("bytes", addr)])
    }

    fn df_layout(name: A::MoveTypeLayout, value: A::MoveTypeLayout) -> A::MoveTypeLayout {
        let uid = layout_("0x2::object::UID", vec![("id", id_layout())]);
        let field = format!(
            "0x2::dynamic_field::Field<{}, {}>",
            TypeTag::from(&name).to_canonical_display(/* with_prefix */ true),
            TypeTag::from(&value).to_canonical_display(/* with_prefix */ true)
        );

        layout_(&field, vec![("id", uid), ("name", name), ("value", value)])
    }

    fn dof_layout(name: A::MoveTypeLayout) -> A::MoveTypeLayout {
        let tag = TypeTag::from(&name);
        let wrapper = format!(
            "0x2::dynamic_object_field::Wrapper<{}>",
            tag.to_canonical_display(/* with_prefix */ true)
        );

        let name = layout_(&wrapper, vec![("name", name)]);
        df_layout(name, id_layout())
    }
}
