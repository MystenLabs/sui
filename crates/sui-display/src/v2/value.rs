// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(unused)]

use std::{borrow::Cow, fmt::Write as _, str};

use async_trait::async_trait;
use base64::engine::{
    Engine,
    general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
};
use chrono::{DateTime, Utc};
use move_core_types::{
    account_address::AccountAddress,
    annotated_value::{MoveFieldLayout, MoveTypeLayout},
    language_storage::{StructTag, TypeTag},
    u256::U256,
};
use serde::{
    Serialize,
    ser::{SerializeSeq as _, SerializeTuple as _, SerializeTupleVariant},
};
use sui_types::{
    MOVE_STDLIB_ADDRESS,
    base_types::{
        RESOLVED_UTF8_STR, STD_OPTION_MODULE_NAME, STD_OPTION_STRUCT_NAME, move_ascii_str_layout,
        move_utf8_str_layout, url_layout,
    },
    id::{ID, UID},
};

use super::{error::FormatError, parser::Transform, writer::BoundedWriter};

/// Dynamically load objects by their ID. The output should be a `Slice` containing references to
/// the raw BCS bytes and the corresponding `MoveTypeLayout` for the object. This implies the
/// `Store` acts as a pool of cached objects.
#[async_trait]
pub trait Store<'s> {
    async fn object(&self, id: AccountAddress) -> anyhow::Result<Option<Slice<'s>>>;
}

/// Value representation used during evaluation by the Display v2 interpreter.
#[derive(Clone)]
pub enum Value<'s> {
    Address(AccountAddress),
    Bool(bool),
    Bytes(Cow<'s, [u8]>),
    Enum(Enum<'s>),
    Slice(Slice<'s>),
    String(Cow<'s, [u8]>),
    Struct(Struct<'s>),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(U256),
    Vector(Vector<'s>),
}

/// Non-aggregate values that can be formatted during string interpolation.
#[derive(Debug, PartialEq, Eq)]
pub enum Atom<'s> {
    Address(AccountAddress),
    Bool(bool),
    Bytes(Cow<'s, [u8]>),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(U256),
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
    pub(crate) type_: Cow<'s, TypeTag>,
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
    /// Write out a formatted representation of this value, transformed by `transform`, to the
    /// provided writer.
    ///
    /// This operation can fail if the transform is not supported for this value, or if the output
    /// is too large. If it succeds, `w` will be modified to include the newly written data.
    pub(crate) fn format(
        self,
        transform: Transform,
        w: &mut BoundedWriter<'_>,
    ) -> Result<(), FormatError> {
        // TODO(amnn): Detect transforms that can't be applied in this context (e.g. 'json' and
        // 'display').
        match transform {
            Transform::Base64 => Atom::try_from(self)?.format_as_base64(&STANDARD, w),
            Transform::Base64NoPad => Atom::try_from(self)?.format_as_base64(&STANDARD_NO_PAD, w),
            Transform::Base64Url => Atom::try_from(self)?.format_as_base64(&URL_SAFE, w),
            Transform::Base64UrlNoPad => {
                Atom::try_from(self)?.format_as_base64(&URL_SAFE_NO_PAD, w)
            }
            Transform::Bcs => Ok(write!(w, "{}", STANDARD.encode(bcs::to_bytes(&self)?))?),
            Transform::Hex => Atom::try_from(self)?.format_as_hex(w),
            Transform::Str => Atom::try_from(self)?.format_as_str(w),
            Transform::Timestamp => Atom::try_from(self)?.format_as_timestamp(w),
            Transform::Url => Atom::try_from(self)?.format_as_url(w),
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

    /// Attempt to coerce this value into a `u64` if that's possible. This works for any numeric
    /// value that can be represented within 64 bits.
    pub(crate) fn as_u64(&self) -> Option<u64> {
        use MoveTypeLayout as L;
        use Value as V;

        match self {
            // Numeric literals in Display
            V::U8(i) => Some(*i as u64),
            V::U16(i) => Some(*i as u64),
            V::U32(i) => Some(*i as u64),
            V::U64(i) => Some(*i),
            V::U128(i) => u64::try_from(*i).ok(),
            V::U256(i) => u64::try_from(*i).ok(),

            // Numeric values sliced out of Move values
            V::Slice(Slice {
                layout,
                bytes: data,
            }) => match layout {
                L::U8 => Some(bcs::from_bytes::<u8>(data).ok()?.into()),
                L::U16 => Some(bcs::from_bytes::<u16>(data).ok()?.into()),
                L::U32 => Some(bcs::from_bytes::<u32>(data).ok()?.into()),
                L::U64 => bcs::from_bytes::<u64>(data).ok(),
                L::U128 => bcs::from_bytes::<u128>(data).ok()?.try_into().ok(),
                L::U256 => bcs::from_bytes::<U256>(data).ok()?.try_into().ok(),
                L::Address | L::Bool | L::Enum(_) | L::Signer | L::Struct(_) | L::Vector(_) => None,
            },

            // Everything else cannot be coerced to u64
            V::Address(_)
            | V::Bool(_)
            | V::Bytes(_)
            | V::Enum(_)
            | V::String(_)
            | V::Struct(_)
            | V::Vector(_) => None,
        }
    }
}

impl Atom<'_> {
    /// Format the atom as a hexadecimal string.
    fn format_as_hex(&self, w: &mut BoundedWriter<'_>) -> Result<(), FormatError> {
        match self {
            Atom::Bool(b) => write!(w, "{:02x}", *b as u8)?,
            Atom::U8(n) => write!(w, "{n:02x}")?,
            Atom::U16(n) => write!(w, "{n:04x}")?,
            Atom::U32(n) => write!(w, "{n:08x}")?,
            Atom::U64(n) => write!(w, "{n:016x}")?,
            Atom::U128(n) => write!(w, "{n:032x}")?,
            Atom::U256(n) => write!(w, "{n:064x}")?,

            Atom::Address(a) => {
                for b in a.into_bytes() {
                    write!(w, "{b:02x}")?;
                }
            }

            Atom::Bytes(bs) => {
                for b in bs.iter() {
                    write!(w, "{b:02x}")?;
                }
            }
        }

        Ok(())
    }

    /// Format the atom as a string.
    fn format_as_str(&self, w: &mut BoundedWriter<'_>) -> Result<(), FormatError> {
        match self {
            Atom::Address(a) => write!(w, "{}", a.to_canonical_display(true))?,
            Atom::Bool(b) => write!(w, "{b}")?,
            Atom::U8(n) => write!(w, "{n}")?,
            Atom::U16(n) => write!(w, "{n}")?,
            Atom::U32(n) => write!(w, "{n}")?,
            Atom::U64(n) => write!(w, "{n}")?,
            Atom::U128(n) => write!(w, "{n}")?,
            Atom::U256(n) => write!(w, "{n}")?,
            Atom::Bytes(bs) => {
                let s = str::from_utf8(bs)
                    .map_err(|_| FormatError::TransformInvalid("expected utf8 bytes"))?;
                write!(w, "{s}")?;
            }
        }

        Ok(())
    }

    /// Coerce the atom into an `i64`, interpreted as an offset in milliseconds since the Unix
    /// epoch, and format it as an ISO8601 timestamp.
    fn format_as_timestamp(&self, w: &mut BoundedWriter<'_>) -> Result<(), FormatError> {
        let ts = self
            .as_i64()
            .and_then(DateTime::from_timestamp_millis)
            .ok_or_else(|| {
                FormatError::TransformInvalid("expected unix timestamp in milliseconds")
            })?;

        write!(w, "{ts:?}")?;
        Ok(())
    }

    /// Like string formatting, but percent-encoding reserved URL characters.
    fn format_as_url(&self, w: &mut BoundedWriter<'_>) -> Result<(), FormatError> {
        match self {
            Atom::Address(a) => write!(w, "{}", a.to_canonical_display(true))?,
            Atom::Bool(b) => write!(w, "{b}")?,
            Atom::U8(n) => write!(w, "{n}")?,
            Atom::U16(n) => write!(w, "{n}")?,
            Atom::U32(n) => write!(w, "{n}")?,
            Atom::U64(n) => write!(w, "{n}")?,
            Atom::U128(n) => write!(w, "{n}")?,
            Atom::U256(n) => write!(w, "{n}")?,
            Atom::Bytes(bs) => {
                for b in bs.iter() {
                    match *b {
                        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                            write!(w, "{}", *b as char)?
                        }
                        b => write!(w, "%{b:02X}")?,
                    }
                }
            }
        }

        Ok(())
    }

    /// Base64-encode the byte representation of this atom.
    fn format_as_base64(
        &self,
        e: &impl Engine,
        w: &mut BoundedWriter<'_>,
    ) -> Result<(), FormatError> {
        let base64 = match self {
            Atom::Address(a) => e.encode(a.into_bytes()),
            Atom::Bool(b) => e.encode([*b as u8]),
            Atom::U8(n) => e.encode([*n]),
            Atom::U16(n) => e.encode(n.to_le_bytes()),
            Atom::U32(n) => e.encode(n.to_le_bytes()),
            Atom::U64(n) => e.encode(n.to_le_bytes()),
            Atom::U128(n) => e.encode(n.to_le_bytes()),
            Atom::U256(n) => e.encode(n.to_le_bytes()),
            Atom::Bytes(bs) => e.encode(bs),
        };

        write!(w, "{base64}")?;
        Ok(())
    }

    /// Attempt to coerce this atom into an `i64`, if possible.
    fn as_i64(&self) -> Option<i64> {
        match self {
            Atom::U8(n) => Some(*n as i64),
            Atom::U16(n) => Some(*n as i64),
            Atom::U32(n) => Some(*n as i64),
            Atom::U64(n) => i64::try_from(*n).ok(),
            Atom::U128(n) => i64::try_from(*n).ok(),
            Atom::U256(n) => u64::try_from(*n).ok().and_then(|v| i64::try_from(v).ok()),
            _ => None,
        }
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
            A::Index(value) => value.as_u64(),
            // All other index types don't represent a numeric index.
            A::DFIndex(_) | A::DOFIndex(_) | A::Field(_) | A::Positional(_) => None,
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
        TypeTag::Vector(Box::new(self.type_.clone().into_owned()))
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
            // Move values cannot serialize to an empty byte stream, so if there are no fields,
            // `dummy_field: bool = false` is injected.
            Fields::Positional(fs) if fs.is_empty() => {
                s.serialize_element(&false)?;
            }

            Fields::Named(fs) if fs.is_empty() => {
                s.serialize_element(&false)?;
            }

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

impl<'s> TryFrom<Value<'s>> for Atom<'s> {
    type Error = FormatError;

    fn try_from(value: Value<'s>) -> Result<Atom<'s>, FormatError> {
        use Atom as A;
        use MoveTypeLayout as L;
        use TypeTag as T;
        use Value as V;

        Ok(match value {
            V::Address(a) => A::Address(a),
            V::Bool(b) => A::Bool(b),
            V::U8(n) => A::U8(n),
            V::U16(n) => A::U16(n),
            V::U32(n) => A::U32(n),
            V::U64(n) => A::U64(n),
            V::U128(n) => A::U128(n),
            V::U256(n) => A::U256(n),

            // Byte arrays and strings are indistinguishable at the Atom level
            V::Bytes(bs) | V::String(bs) => A::Bytes(bs),

            V::Enum(_) => return Err(FormatError::TransformInvalid("unexpected enum")),
            V::Struct(_) => return Err(FormatError::TransformInvalid("unexpected struct")),

            // Vector literals are supported if they are byte vectors.
            V::Vector(Vector { type_, elements }) => {
                if *type_ != T::U8 {
                    return Err(FormatError::TransformInvalid("unexpected vector"));
                }

                let bytes: Result<Vec<_>, _> = elements
                    .into_iter()
                    .map(|e| match e {
                        V::U8(b) => Ok(b),
                        V::Slice(Slice { layout, bytes }) if layout == &L::U8 => {
                            Ok(bcs::from_bytes(bytes)?)
                        }
                        _ => Err(FormatError::TransformInvalid("unexpected vector")),
                    })
                    .collect();

                A::Bytes(Cow::Owned(bytes?))
            }

            V::Slice(Slice { layout, bytes }) => match layout {
                L::Address => A::Address(bcs::from_bytes(bytes)?),
                L::Bool => A::Bool(bcs::from_bytes(bytes)?),
                L::U8 => A::U8(bcs::from_bytes(bytes)?),
                L::U16 => A::U16(bcs::from_bytes(bytes)?),
                L::U32 => A::U32(bcs::from_bytes(bytes)?),
                L::U64 => A::U64(bcs::from_bytes(bytes)?),
                L::U128 => A::U128(bcs::from_bytes(bytes)?),
                L::U256 => A::U256(bcs::from_bytes(bytes)?),

                L::Vector(layout) if layout.as_ref() == &L::U8 => {
                    A::Bytes(Cow::Borrowed(bcs::from_bytes(bytes)?))
                }

                L::Struct(layout)
                    if [
                        move_ascii_str_layout(),
                        move_utf8_str_layout(),
                        url_layout(),
                    ]
                    .contains(layout.as_ref()) =>
                {
                    A::Bytes(Cow::Borrowed(bcs::from_bytes(bytes)?))
                }

                L::Struct(layout) if [UID::layout(), ID::layout()].contains(layout.as_ref()) => {
                    A::Address(bcs::from_bytes(bytes)?)
                }

                L::Signer => return Err(FormatError::TransformInvalid("unexpected signer")),
                L::Enum(_) => return Err(FormatError::TransformInvalid("unexpected enum")),
                L::Struct(_) => return Err(FormatError::TransformInvalid("unexpected struct")),
                L::Vector(_) => return Err(FormatError::TransformInvalid("unexpected vector")),
            },
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;
    use std::str::FromStr;

    use move_core_types::annotated_value::{
        MoveEnumLayout, MoveFieldLayout, MoveStructLayout, MoveTypeLayout as L,
    };
    use move_core_types::identifier::Identifier;
    use sui_types::base_types::{STD_ASCII_MODULE_NAME, STD_ASCII_STRUCT_NAME};
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

    pub fn optional_(layout: MoveTypeLayout) -> MoveTypeLayout {
        let type_ = TypeTag::from(&layout);
        struct_(
            &format!("0x1::option::Option<{type_}>"),
            vec![("vec", vector_(layout))],
        )
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
            bcs::to_bytes(&Value::String(Cow::Borrowed("hello".as_bytes()))).unwrap(),
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
                Value::String(Cow::Borrowed("test".as_bytes())),
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
    fn test_serialize_empty_struct() {
        let type_ = &"0x2::foo::Empty".parse().unwrap();

        let positional = Value::Struct(Struct {
            type_,
            fields: Fields::Positional(vec![]),
        });

        let named = Value::Struct(Struct {
            type_,
            fields: Fields::Named(vec![]),
        });

        assert_eq!(
            bcs::to_bytes(&positional).unwrap(),
            bcs::to_bytes(&false).unwrap()
        );

        assert_eq!(
            bcs::to_bytes(&named).unwrap(),
            bcs::to_bytes(&false).unwrap()
        );
    }

    #[test]
    fn test_serialize_enum() {
        #[derive(Serialize)]
        enum E {
            A(u64, bool),
            B { x: u32, y: u32 },
            C,
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

        // Test enum with no fields
        let enum_ = Value::Enum(Enum {
            type_: &type_,
            variant_name: Some("C"),
            variant_index: 2,
            fields: Fields::Positional(vec![]),
        });

        assert_eq!(
            bcs::to_bytes(&enum_).unwrap(),
            bcs::to_bytes(&E::C).unwrap()
        );
    }

    #[test]
    fn test_serialize_vector() {
        let vec = Value::Vector(Vector {
            type_: Cow::Owned(TypeTag::U64),
            elements: vec![Value::U64(10), Value::U64(20), Value::U64(30)],
        });

        assert_eq!(
            bcs::to_bytes(&vec).unwrap(),
            bcs::to_bytes(&vec![10u64, 20, 30]).unwrap()
        );

        // Test vector of strings
        let vec = Value::Vector(Vector {
            type_: Cow::Owned(TypeTag::Struct(Box::new(StructTag {
                address: MOVE_STDLIB_ADDRESS,
                module: STD_ASCII_MODULE_NAME.to_owned(),
                name: STD_ASCII_STRUCT_NAME.to_owned(),
                type_params: vec![],
            }))),
            elements: vec![
                Value::String(Cow::Borrowed("hello".as_bytes())),
                Value::String(Cow::Borrowed("world".as_bytes())),
            ],
        });

        assert_eq!(
            bcs::to_bytes(&vec).unwrap(),
            bcs::to_bytes(&vec!["hello", "world"]).unwrap()
        );

        // Test empty vector
        let vec = Value::Vector(Vector {
            type_: Cow::Owned(TypeTag::U64),
            elements: vec![],
        });

        assert_eq!(bcs::to_bytes(&vec).unwrap(), &[0x00]);
    }

    #[test]
    fn test_literal_to_atom_conversion() {
        let values = vec![
            Value::Bool(true),
            Value::U8(42),
            Value::U16(1234),
            Value::U32(123456),
            Value::U64(12345678),
            Value::U128(123456),
            Value::U256(U256::from(42u64)),
            Value::Address("0x42".parse().unwrap()),
            Value::String(Cow::Borrowed("hello".as_bytes())),
            Value::Bytes(Cow::Borrowed(&[1, 2, 3])),
            Value::Vector(Vector {
                type_: Cow::Owned(TypeTag::U8),
                elements: vec![
                    Value::U8(4),
                    Value::U8(5),
                    Value::Slice(Slice {
                        layout: &L::U8,
                        bytes: &[6],
                    }),
                ],
            }),
        ];

        let atoms = vec![
            Atom::Bool(true),
            Atom::U8(42),
            Atom::U16(1234),
            Atom::U32(123456),
            Atom::U64(12345678),
            Atom::U128(123456),
            Atom::U256(U256::from(42u64)),
            Atom::Address("0x42".parse().unwrap()),
            Atom::Bytes(Cow::Borrowed("hello".as_bytes())),
            Atom::Bytes(Cow::Borrowed(&[1, 2, 3])),
            Atom::Bytes(Cow::Borrowed(&[4, 5, 6])),
        ];

        assert_eq!(values.len(), atoms.len());
        for (value, expect) in values.into_iter().zip(atoms.into_iter()) {
            let actual = Atom::try_from(value).unwrap();
            assert_eq!(actual, expect);
        }
    }

    #[test]
    fn test_slice_to_atom_converion() {
        let bool_bytes = bcs::to_bytes(&true).unwrap();
        let u8_bytes = bcs::to_bytes(&42u8).unwrap();
        let u16_bytes = bcs::to_bytes(&1234u16).unwrap();
        let u32_bytes = bcs::to_bytes(&123456u32).unwrap();
        let u64_bytes = bcs::to_bytes(&12345678u64).unwrap();
        let u128_bytes = bcs::to_bytes(&123456u128).unwrap();
        let u256_bytes = bcs::to_bytes(&U256::from(42u64)).unwrap();
        let addr_bytes = bcs::to_bytes(&AccountAddress::from_str("0x42").unwrap()).unwrap();
        let str_bytes = bcs::to_bytes("hello").unwrap();
        let vec_bytes = bcs::to_bytes(&vec![1u8, 2, 3]).unwrap();

        let str_layout = L::Struct(Box::new(move_utf8_str_layout()));
        let vec_layout = L::Vector(Box::new(L::U8));

        let values = vec![
            Value::Slice(Slice {
                layout: &L::Bool,
                bytes: &bool_bytes,
            }),
            Value::Slice(Slice {
                layout: &L::U8,
                bytes: &u8_bytes,
            }),
            Value::Slice(Slice {
                layout: &L::U16,
                bytes: &u16_bytes,
            }),
            Value::Slice(Slice {
                layout: &L::U32,
                bytes: &u32_bytes,
            }),
            Value::Slice(Slice {
                layout: &L::U64,
                bytes: &u64_bytes,
            }),
            Value::Slice(Slice {
                layout: &L::U128,
                bytes: &u128_bytes,
            }),
            Value::Slice(Slice {
                layout: &L::U256,
                bytes: &u256_bytes,
            }),
            Value::Slice(Slice {
                layout: &L::Address,
                bytes: &addr_bytes,
            }),
            Value::Slice(Slice {
                layout: &str_layout,
                bytes: &str_bytes,
            }),
            Value::Slice(Slice {
                layout: &vec_layout,
                bytes: &vec_bytes,
            }),
        ];

        let atoms = vec![
            Atom::Bool(true),
            Atom::U8(42),
            Atom::U16(1234),
            Atom::U32(123456),
            Atom::U64(12345678),
            Atom::U128(123456),
            Atom::U256(U256::from(42u64)),
            Atom::Address(AccountAddress::from_str("0x42").unwrap()),
            Atom::Bytes(Cow::Borrowed("hello".as_bytes())),
            Atom::Bytes(Cow::Borrowed(&[1, 2, 3])),
        ];

        for (value, expect) in values.into_iter().zip(atoms.into_iter()) {
            let actual = Atom::try_from(value).unwrap();
            assert_eq!(actual, expect);
        }
    }
}
