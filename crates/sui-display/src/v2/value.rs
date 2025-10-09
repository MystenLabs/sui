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
use sui_types::{
    MOVE_STDLIB_ADDRESS,
    base_types::{STD_OPTION_MODULE_NAME, STD_OPTION_STRUCT_NAME},
};

use super::{error::FormatError, format_visitor::FormatVisitor, writer::BoundedWriter};

/// Dynamically load objects by their ID. The output should be a `Slice` containing references to
/// the raw BCS bytes and the corresponding `MoveTypeLayout` for the object. This implies the
/// `Store` acts as a pool of cached objects.
#[async_trait]
pub trait Store<'a> {
    async fn object(&self, id: AccountAddress) -> anyhow::Result<Option<Slice<'a>>>;
}

/// Value representation for the Display v2 interpreter.
#[derive(Clone)]
pub enum Value<'a> {
    Address(AccountAddress),
    Bool(bool),
    Bytes(Cow<'a, [u8]>),
    Enum(Enum<'a>),
    Slice(Slice<'a>),
    String(Cow<'a, str>),
    Struct(Struct<'a>),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(U256),
    Vector(Vector<'a>),
}

/// A single step in a chain of accesses, with its inner expression (if there is one) evaluated.
pub enum Accessor<'a> {
    Field(&'a str),
    Positional(u8),
    Index(Value<'a>),
    DFIndex(Value<'a>),
    DOFIndex(Value<'a>),
}

/// Bytes extracted from the serialized representation of a Move value, along with its layout.
#[derive(Copy, Clone)]
pub struct Slice<'a> {
    pub(crate) layout: &'a MoveTypeLayout,
    pub(crate) bytes: &'a [u8],
}

/// An evaluated vector literal.
#[derive(Clone)]
pub struct Vector<'a> {
    pub(crate) type_: Option<&'a TypeTag>,
    pub(crate) elements: Vec<Value<'a>>,
}

/// An evaluated struct literal.
#[derive(Clone)]
pub struct Struct<'a> {
    pub(crate) type_: &'a StructTag,
    pub(crate) fields: Fields<'a>,
}

/// An evaluated enum/variant literal.
#[derive(Clone)]
pub struct Enum<'a> {
    pub(crate) type_: &'a StructTag,
    pub(crate) variant_name: Option<&'a str>,
    pub(crate) variant_index: u16,
    pub(crate) fields: Fields<'a>,
}

/// Evaluated fields that are part of a struct or enum literal.
#[derive(Clone)]
pub enum Fields<'a> {
    Positional(Vec<Value<'a>>),
    Named(Vec<(&'a str, Value<'a>)>),
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

impl<'a> Accessor<'a> {
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
    pub(crate) fn as_field_name(&self) -> Option<Cow<'a, str>> {
        use Accessor as A;
        match self {
            A::Field(f) => Some(Cow::Borrowed(*f)),
            A::Positional(i) => Some(Cow::Owned(format!("pos{i}"))),
            A::Index(_) | A::DFIndex(_) | A::DOFIndex(_) => None,
        }
    }
}

#[cfg(test)]
impl<'a> Slice<'a> {
    pub fn new_for_test(layout: &'a MoveTypeLayout, bytes: &'a [u8]) -> Self {
        Self { layout, bytes }
    }
}

impl<'a> Fields<'a> {
    /// Attempt to fetch a particular field  from a struct or enum literal's fields based on the
    /// given accessor.
    pub(crate) fn get(self, accessor: &Accessor<'a>) -> Option<Value<'a>> {
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
}

#[cfg(test)]
pub(crate) mod tests {
    use move_core_types::annotated_value::{MoveEnumLayout, MoveFieldLayout, MoveStructLayout};
    use move_core_types::identifier::Identifier;

    use super::*;

    /// Mock Store implementation for testing (does not actually implement object loading, because
    /// this is not required by the implementation yet).
    pub struct MockStore;

    #[async_trait]
    impl<'a> Store<'a> for MockStore {
        async fn object(&self, _id: AccountAddress) -> anyhow::Result<Option<Slice<'a>>> {
            unimplemented!()
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
}
