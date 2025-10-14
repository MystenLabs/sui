// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(unused)]

use std::{borrow::Cow, fmt::Write as _};

use async_trait::async_trait;
use move_core_types::{
    account_address::AccountAddress,
    annotated_value::MoveTypeLayout,
    language_storage::{StructTag, TypeTag},
    u256::U256,
};
use serde::{
    Serialize,
    ser::{SerializeSeq as _, SerializeTuple as _, SerializeTupleVariant},
};
use sui_types::{
    MOVE_STDLIB_ADDRESS,
    base_types::{RESOLVED_UTF8_STR, STD_OPTION_MODULE_NAME, STD_OPTION_STRUCT_NAME},
};

use super::{error::FormatError, format_visitor::FormatVisitor, writer::BoundedWriter};

/// Dynamically load objects by their ID. The output should be a `Slice` containing references to
/// the raw BCS bytes and the corresponding `MoveTypeLayout` for the object. This implies the
/// `Store` acts as a pool of cached objects.
#[async_trait]
pub trait Store<'s> {
    async fn object(&self, id: AccountAddress) -> anyhow::Result<Option<Slice<'s>>>;
}

/// Value representation for the Display v2 interpreter.
#[derive(Clone)]
pub enum Value<'s> {
    Address(AccountAddress),
    Bool(bool),
    Bytes(Cow<'s, [u8]>),
    Enum(Enum<'s>),
    Slice(Slice<'s>),
    String(Cow<'s, str>),
    Struct(Struct<'s>),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(U256),
    Vector(Vector<'s>),
}

/// A single step in a chain of accesses, with its inner expression (if there is one) evaluated.
pub enum Accessor<'s> {
    Field(&'s str),
    Positional(u8),
    Index(Value<'s>),
    DFIndex(Value<'s>),
    DOFIndex(Value<'s>),
}

/// Bytes extracted from the serialized representation of a Move value, along with its layout.
#[derive(Copy, Clone)]
pub struct Slice<'s> {
    pub(crate) layout: &'s MoveTypeLayout,
    pub(crate) bytes: &'s [u8],
}

/// An evaluated vector literal.
#[derive(Clone)]
pub struct Vector<'s> {
    pub(crate) type_: Option<&'s TypeTag>,
    pub(crate) elements: Vec<Value<'s>>,
}

/// An evaluated struct literal.
#[derive(Clone)]
pub struct Struct<'s> {
    pub(crate) type_: &'s StructTag,
    pub(crate) fields: Fields<'s>,
}

/// An evaluated enum/variant literal.
#[derive(Clone)]
pub struct Enum<'s> {
    pub(crate) type_: &'s StructTag,
    pub(crate) variant_name: Option<&'s str>,
    pub(crate) variant_index: u16,
    pub(crate) fields: Fields<'s>,
}

/// Evaluated fields that are part of a struct or enum literal.
#[derive(Clone)]
pub enum Fields<'s> {
    Positional(Vec<Value<'s>>),
    Named(Vec<(&'s str, Value<'s>)>),
}

impl Value<'_> {
    /// Write out a formatted representation of this value, optionally transformed by `transform`,
    /// to the provided writer.
    ///
    /// This operation can fail if the transform is not supported for this value, or if the output
    /// is too large. If it succeds, `w` will be modified to include the newly written data.
    pub(crate) fn format(
        &self,
        transform: Option<&str>,
        w: &mut BoundedWriter<'_>,
    ) -> Result<(), FormatError> {
        match transform {
            None => self.format_as_str(w),
            // TODO(amnn): Detect transforms that can't be applied in this context (e.g. 'json' and
            // 'display').
            Some(transform) => Err(FormatError::TransformUnrecognized(transform.to_string())),
        }
    }

    /// The Move type of this value.
    pub(crate) fn type_(&self) -> TypeTag {
        match self {
            Value::Address(_) => TypeTag::Address,
            Value::Bool(_) => TypeTag::Bool,
            Value::Bytes(_) => TypeTag::Vector(Box::new(TypeTag::U8)),
            Value::U8(_) => TypeTag::U8,
            Value::U16(_) => TypeTag::U16,
            Value::U32(_) => TypeTag::U32,
            Value::U64(_) => TypeTag::U64,
            Value::U128(_) => TypeTag::U128,
            Value::U256(_) => TypeTag::U256,

            Value::Enum(e) => e.type_.clone().into(),
            Value::Struct(s) => s.type_.clone().into(),

            Value::Slice(s) => s.layout.into(),

            Value::String(_) => {
                let (&address, module, name) = RESOLVED_UTF8_STR;
                TypeTag::Struct(Box::new(StructTag {
                    address,
                    module: module.to_owned(),
                    name: name.to_owned(),
                    type_params: vec![],
                }))
            }

            Value::Vector(v) => v.type_(),
        }
    }

    /// Predicate to check whether this value represents a `None: std::option::Option<T>` value.
    /// Only values sliced out of real Move values are detected as `None`. Literals that are
    /// constructed to look like `None` are not detected as such.
    pub(crate) fn is_none(&self) -> bool {
        let Value::Slice(Slice { layout, bytes }) = self else {
            return false;
        };

        let MoveTypeLayout::Struct(s) = layout else {
            return false;
        };

        s.type_.address == MOVE_STDLIB_ADDRESS
            && s.type_.module.as_ref() == STD_OPTION_MODULE_NAME
            && s.type_.name.as_ref() == STD_OPTION_STRUCT_NAME
            && bytes == &[0x00]
    }

    /// Implementation of 'string' transform, which is the transform used if
    fn format_as_str(&self, w: &mut BoundedWriter<'_>) -> Result<(), FormatError> {
        match self {
            Value::Bytes(_) => return Err(FormatError::TransformInvalid("str", "raw bytes")),
            Value::Enum(_) => return Err(FormatError::TransformInvalid("str", "enum literals")),
            Value::Struct(_) => {
                return Err(FormatError::TransformInvalid("str", "struct literals"));
            }
            Value::Vector(_) => {
                return Err(FormatError::TransformInvalid("str", "vector literals"));
            }

            Value::Address(a) => write!(w, "{}", a.to_canonical_display(true))?,
            Value::Bool(b) => write!(w, "{b}")?,
            Value::U8(n) => write!(w, "{n}")?,
            Value::U16(n) => write!(w, "{n}")?,
            Value::U32(n) => write!(w, "{n}")?,
            Value::U64(n) => write!(w, "{n}")?,
            Value::U128(n) => write!(w, "{n}")?,
            Value::U256(n) => write!(w, "{n}")?,
            Value::String(s) => write!(w, "{s}")?,

            Value::Slice(s) => FormatVisitor::deserialize_slice(*s, w)?,
        }

        Ok(())
    }
}

impl<'s> Accessor<'s> {
    /// Coerce this accessor into a numeric index, if possible, and returns its value.
    ///
    /// Coercion works for all integer literals, as well as `Slice` literals with a numeric layout,
    /// as long as their numeric values fit into a `u64`.
    pub(crate) fn as_numeric_index(&self) -> Option<u64> {
        use Accessor as A;
        use MoveTypeLayout as L;

        match self {
            // Numeric literals in Display
            A::Index(Value::U8(i)) => Some(*i as u64),
            A::Index(Value::U16(i)) => Some(*i as u64),
            A::Index(Value::U32(i)) => Some(*i as u64),
            A::Index(Value::U64(i)) => Some(*i),
            A::Index(Value::U128(i)) => u64::try_from(*i).ok(),
            A::Index(Value::U256(i)) => u64::try_from(*i).ok(),

            // Numeric values sliced out of Move values
            A::Index(Value::Slice(Slice {
                layout,
                bytes: data,
            })) => match layout {
                L::U8 => Some(bcs::from_bytes::<u8>(data).ok()? as u64),
                L::U16 => Some(bcs::from_bytes::<u16>(data).ok()? as u64),
                L::U32 => Some(bcs::from_bytes::<u32>(data).ok()? as u64),
                L::U64 => Some(bcs::from_bytes::<u64>(data).ok()?),
                L::U128 => bcs::from_bytes::<u128>(data).ok()?.try_into().ok(),
                L::U256 => bcs::from_bytes::<U256>(data).ok()?.try_into().ok(),
                _ => None,
            },

            // Everything else
            A::Index(_) | A::DFIndex(_) | A::DOFIndex(_) | A::Field(_) | A::Positional(_) => None,
        }
    }

    /// Coerce this accessor into a field name, if possible, and return its name.
    pub(crate) fn as_field_name(&self) -> Option<Cow<'s, str>> {
        use Accessor as A;
        match self {
            A::Field(f) => Some(Cow::Borrowed(*f)),
            A::Positional(i) => Some(Cow::Owned(format!("pos{i}"))),
            A::Index(_) | A::DFIndex(_) | A::DOFIndex(_) => None,
        }
    }
}

impl Vector<'_> {
    fn type_(&self) -> TypeTag {
        TypeTag::Vector(Box::new(if let Some(explicit) = self.type_ {
            explicit.clone()
        } else if let Some(first) = self.elements.first() {
            first.type_()
        } else {
            unreachable!("SAFETY: vectors either have a type annotation or at least one element")
        }))
    }
}

impl<'s> Fields<'s> {
    /// Attempt to fetch a particular field  from a struct or enum literal's fields based on the
    /// given accessor.
    pub(crate) fn get(self, accessor: &Accessor<'s>) -> Option<Value<'s>> {
        match (self, accessor) {
            (Fields::Positional(mut fs), Accessor::Positional(i)) => {
                let i = *i as usize;
                if i < fs.len() {
                    Some(fs.swap_remove(i))
                } else {
                    None
                }
            }

            (Fields::Named(mut fs), Accessor::Field(f)) => {
                let i = fs.iter().position(|(n, _)| n == f)?;
                Some(fs.swap_remove(i).1)
            }

            _ => None,
        }
    }

    fn len(&self) -> usize {
        match self {
            Fields::Positional(fs) => fs.len(),
            Fields::Named(fs) => fs.len(),
        }
    }
}

/// Serialize implementation for Value to support serializing the Value to BCS bytes.
impl Serialize for Value<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Value::Address(a) => a.serialize(serializer),
            Value::Bool(b) => b.serialize(serializer),
            Value::Bytes(b) => b.serialize(serializer),
            Value::Enum(e) => e.serialize(serializer),
            Value::Slice(s) => s.serialize(serializer),
            Value::String(s) => s.serialize(serializer),
            Value::Struct(s) => s.serialize(serializer),
            Value::U8(n) => n.serialize(serializer),
            Value::U16(n) => n.serialize(serializer),
            Value::U32(n) => n.serialize(serializer),
            Value::U64(n) => n.serialize(serializer),
            Value::U128(n) => n.serialize(serializer),
            Value::U256(n) => n.serialize(serializer),
            Value::Vector(v) => v.serialize(serializer),
        }
    }
}

/// This implementation makes it so that serializing a `Slice` to BCS bytes produces the bytes
/// unchanged (but this property is not guaranteed for any other format).
impl Serialize for Slice<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_tuple(self.bytes.len())?;
        for b in self.bytes {
            s.serialize_element(b)?;
        }

        s.end()
    }
}

impl Serialize for Vector<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.elements.len()))?;
        for e in &self.elements {
            s.serialize_element(e)?;
        }

        s.end()
    }
}

impl Serialize for Struct<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize the struct as a tuple, regardless of whether it has named or positional
        // fields, because `serde`'s field names need to be `&'static str`, which we don't have
        // (and we don't need).
        let mut s = serializer.serialize_tuple(self.fields.len())?;

        match &self.fields {
            Fields::Positional(fs) => {
                for f in fs {
                    s.serialize_element(f)?;
                }
            }
            Fields::Named(fs) => {
                for (_, f) in fs {
                    s.serialize_element(f)?;
                }
            }
        }

        s.end()
    }
}

impl Serialize for Enum<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize the enum as a tuple, with empty names, for similar reasons as `Struct`, above.
        let mut s = serializer.serialize_tuple_variant(
            "",
            self.variant_index as u32,
            "",
            self.fields.len(),
        )?;

        match &self.fields {
            Fields::Positional(fs) => {
                for f in fs {
                    s.serialize_field(f)?;
                }
            }
            Fields::Named(fs) => {
                for (_, f) in fs {
                    s.serialize_field(f)?;
                }
            }
        }

        s.end()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    use move_core_types::annotated_value::{
        MoveEnumLayout, MoveFieldLayout, MoveStructLayout, MoveTypeLayout as L,
    };
    use move_core_types::identifier::Identifier;
    use sui_types::dynamic_field::{DynamicFieldInfo, Field, derive_dynamic_field_id};
    use sui_types::id::{ID, UID};

    use super::*;

    /// Mock Store implementation for testing.
    #[derive(Default)]
    pub struct MockStore {
        data: BTreeMap<AccountAddress, (Vec<u8>, MoveTypeLayout)>,
    }

    impl MockStore {
        /// Add objects representing a dynamic field to the store.
        ///
        /// The dynamic field is owned by `parent` and has the given `name` and `value`, with their
        /// respective layouts.
        pub(crate) fn with_dynamic_field<N: Serialize, V: Serialize>(
            mut self,
            parent: AccountAddress,
            name: N,
            name_layout: MoveTypeLayout,
            value: V,
            value_layout: MoveTypeLayout,
        ) -> Self {
            use Identifier as I;
            use MoveFieldLayout as F;
            use MoveStructLayout as S;
            use MoveTypeLayout as T;

            let name_bytes = bcs::to_bytes(&name).unwrap();
            let name_type = TypeTag::from(&name_layout);
            let value_type = TypeTag::from(&value_layout);
            let df_id = derive_dynamic_field_id(parent, &name_type, &name_bytes).unwrap();

            let field_bytes = bcs::to_bytes(&Field {
                id: UID::new(df_id),
                name,
                value,
            })
            .unwrap();

            let field_layout = L::Struct(Box::new(S {
                type_: DynamicFieldInfo::dynamic_field_type(name_type, value_type),
                fields: vec![
                    F::new(I::new("id").unwrap(), L::Struct(Box::new(UID::layout()))),
                    F::new(I::new("name").unwrap(), name_layout),
                    F::new(I::new("value").unwrap(), value_layout),
                ],
            }));

            self.data.insert(df_id.into(), (field_bytes, field_layout));
            self
        }

        /// Add objects representing a dynamic object field to the store.
        ///
        /// The dynamic object field is owned by `parent` and has the given `name` and `value`,
        /// with their respective layouts. `value` is expected to start with a UID, as it must be
        /// an object (its type must have `key`).
        pub(crate) fn with_dynamic_object_field<N: Serialize, V: Serialize>(
            mut self,
            parent: AccountAddress,
            name: N,
            name_layout: MoveTypeLayout,
            value: V,
            value_layout: MoveTypeLayout,
        ) -> Self {
            use AccountAddress as A;
            use Identifier as I;
            use MoveFieldLayout as F;
            use MoveStructLayout as S;
            use MoveTypeLayout as T;

            let name_bytes = bcs::to_bytes(&name).unwrap();
            let value_bytes = bcs::to_bytes(&value).unwrap();
            let name_type = TypeTag::from(&name_layout);
            let wrap_type = DynamicFieldInfo::dynamic_object_field_wrapper(name_type);
            let val_id = A::from_bytes(&value_bytes[0..AccountAddress::LENGTH]).unwrap();
            let dof_id =
                derive_dynamic_field_id(parent, &wrap_type.clone().into(), &name_bytes).unwrap();

            let field_bytes = bcs::to_bytes(&Field {
                id: UID::new(dof_id),
                name,
                value: val_id,
            })
            .unwrap();

            let wrapper_layout = L::Struct(Box::new(S {
                type_: wrap_type.clone(),
                fields: vec![F::new(I::new("name").unwrap(), name_layout)],
            }));

            let field_layout = L::Struct(Box::new(S {
                type_: DynamicFieldInfo::dynamic_field_type(wrap_type.into(), ID::type_().into()),
                fields: vec![
                    F::new(I::new("id").unwrap(), L::Struct(Box::new(UID::layout()))),
                    F::new(I::new("name").unwrap(), wrapper_layout),
                    F::new(I::new("value").unwrap(), L::Struct(Box::new(ID::layout()))),
                ],
            }));

            self.data.insert(dof_id.into(), (field_bytes, field_layout));
            self.data.insert(val_id, (value_bytes, value_layout));
            self
        }
    }

    #[async_trait]
    impl<'s> Store<'s> for &'s MockStore {
        async fn object(&self, id: AccountAddress) -> anyhow::Result<Option<Slice<'s>>> {
            let Some((bytes, layout)) = self.data.get(&id) else {
                return Ok(None);
            };

            Ok(Some(Slice {
                layout,
                bytes: bytes.as_slice(),
            }))
        }
    }

    pub fn struct_(type_: &str, fields: Vec<(&str, MoveTypeLayout)>) -> MoveTypeLayout {
        let type_: StructTag = type_.parse().unwrap();
        let fields = fields
            .into_iter()
            .map(|(name, layout)| MoveFieldLayout::new(Identifier::new(name).unwrap(), layout))
            .collect();

        MoveTypeLayout::Struct(Box::new(MoveStructLayout { type_, fields }))
    }

    pub fn enum_(
        type_: &str,
        variants: Vec<(&str, Vec<(&str, MoveTypeLayout)>)>,
    ) -> MoveTypeLayout {
        let type_: StructTag = type_.parse().unwrap();
        let variants = variants
            .into_iter()
            .enumerate()
            .map(|(tag, (name, fields))| {
                let fields = fields
                    .into_iter()
                    .map(|(name, layout)| {
                        MoveFieldLayout::new(Identifier::new(name).unwrap(), layout)
                    })
                    .collect();

                ((Identifier::new(name).unwrap(), tag as u16), fields)
            })
            .collect();

        MoveTypeLayout::Enum(Box::new(MoveEnumLayout { type_, variants }))
    }

    pub fn vector_(layout: MoveTypeLayout) -> MoveTypeLayout {
        MoveTypeLayout::Vector(Box::new(layout))
    }

    #[test]
    fn test_slice_serialize_roundtrip() {
        let bytes = &[0x01, 0x02, 0x03, 0x04];
        let slice = Slice {
            layout: &L::U64,
            bytes,
        };

        let serialized = bcs::to_bytes(&slice).unwrap();
        assert_eq!(serialized, bytes);
    }

    #[test]
    fn test_serialize_bool() {
        assert_eq!(
            bcs::to_bytes(&Value::Bool(true)).unwrap(),
            bcs::to_bytes(&true).unwrap()
        );
        assert_eq!(
            bcs::to_bytes(&Value::Bool(false)).unwrap(),
            bcs::to_bytes(&false).unwrap()
        );
    }

    #[test]
    fn test_serialize_u8() {
        assert_eq!(
            bcs::to_bytes(&Value::U8(42)).unwrap(),
            bcs::to_bytes(&42u8).unwrap()
        );
    }

    #[test]
    fn test_serialize_u16() {
        assert_eq!(
            bcs::to_bytes(&Value::U16(1234)).unwrap(),
            bcs::to_bytes(&1234u16).unwrap()
        );
    }

    #[test]
    fn test_serialize_u32() {
        assert_eq!(
            bcs::to_bytes(&Value::U32(123456)).unwrap(),
            bcs::to_bytes(&123456u32).unwrap()
        );
    }

    #[test]
    fn test_serialize_u64() {
        assert_eq!(
            bcs::to_bytes(&Value::U64(12345678901234)).unwrap(),
            bcs::to_bytes(&12345678901234u64).unwrap()
        );
    }

    #[test]
    fn test_serialize_u128() {
        assert_eq!(
            bcs::to_bytes(&Value::U128(123456789012345678901234567890)).unwrap(),
            bcs::to_bytes(&123456789012345678901234567890u128).unwrap()
        );
    }

    #[test]
    fn test_serialize_u256() {
        let val = U256::from(42u64);
        assert_eq!(
            bcs::to_bytes(&Value::U256(val)).unwrap(),
            bcs::to_bytes(&val).unwrap()
        );
    }

    #[test]
    fn test_serialize_address() {
        let addr: AccountAddress = "0x1".parse().unwrap();
        assert_eq!(
            bcs::to_bytes(&Value::Address(addr)).unwrap(),
            bcs::to_bytes(&addr).unwrap()
        );
    }

    #[test]
    fn test_serialize_string() {
        assert_eq!(
            bcs::to_bytes(&Value::String(Cow::Borrowed("hello"))).unwrap(),
            bcs::to_bytes("hello").unwrap()
        );
    }

    #[test]
    fn test_serialize_bytes() {
        let bytes = vec![1u8, 2, 3, 4, 5];
        assert_eq!(
            bcs::to_bytes(&Value::Bytes(Cow::Borrowed(&bytes))).unwrap(),
            bcs::to_bytes(&bytes).unwrap()
        );
    }

    #[test]
    fn test_serialize_positional_struct() {
        let type_ = &"0x2::foo::Bar".parse().unwrap();
        let struct_ = Value::Struct(Struct {
            type_,
            fields: Fields::Positional(vec![
                Value::U64(42),
                Value::Bool(true),
                Value::String(Cow::Borrowed("test")),
            ]),
        });

        assert_eq!(
            bcs::to_bytes(&struct_).unwrap(),
            bcs::to_bytes(&(42u64, true, "test")).unwrap()
        );
    }

    #[test]
    fn test_serialize_named_struct() {
        let type_ = &"0x2::foo::Bar".parse().unwrap();
        let addr = "0x300".parse().unwrap();
        let struct_ = Value::Struct(Struct {
            type_,
            fields: Fields::Named(vec![
                ("x", Value::U32(100)),
                ("y", Value::U32(200)),
                ("z", Value::Address(addr)),
            ]),
        });

        assert_eq!(
            bcs::to_bytes(&struct_).unwrap(),
            bcs::to_bytes(&(100u32, 200u32, addr)).unwrap()
        );
    }

    #[test]
    fn test_serialize_enum() {
        #[derive(Serialize)]
        enum E {
            A(u64, bool),
            B { x: u32, y: u32 },
        }

        let type_: StructTag = "0x1::m::E".parse().unwrap();
        let enum_ = Value::Enum(Enum {
            type_: &type_,
            variant_name: Some("A"),
            variant_index: 0,
            fields: Fields::Positional(vec![Value::U64(42), Value::Bool(true)]),
        });

        assert_eq!(
            bcs::to_bytes(&enum_).unwrap(),
            bcs::to_bytes(&E::A(42, true)).unwrap()
        );

        // Test enum with named fields
        let enum_ = Value::Enum(Enum {
            type_: &type_,
            variant_name: Some("B"),
            variant_index: 1,
            fields: Fields::Named(vec![("x", Value::U32(100)), ("y", Value::U32(200))]),
        });

        assert_eq!(
            bcs::to_bytes(&enum_).unwrap(),
            bcs::to_bytes(&E::B { x: 100, y: 200 }).unwrap()
        );
    }

    #[test]
    fn test_serialize_vector() {
        let vec = Value::Vector(Vector {
            type_: None,
            elements: vec![Value::U64(10), Value::U64(20), Value::U64(30)],
        });

        assert_eq!(
            bcs::to_bytes(&vec).unwrap(),
            bcs::to_bytes(&vec![10u64, 20, 30]).unwrap()
        );

        // Test vector of strings
        let vec = Value::Vector(Vector {
            type_: None,
            elements: vec![
                Value::String(Cow::Borrowed("hello")),
                Value::String(Cow::Borrowed("world")),
            ],
        });

        assert_eq!(
            bcs::to_bytes(&vec).unwrap(),
            bcs::to_bytes(&vec!["hello", "world"]).unwrap()
        );

        // Test empty vector
        let vec = Value::Vector(Vector {
            type_: None,
            elements: vec![],
        });

        assert_eq!(bcs::to_bytes(&vec).unwrap(), &[0x00]);
    }
}
