// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    VARIANT_TAG_MAX_VALUE,
    account_address::AccountAddress,
    annotated_visitor::{Error as VError, ValueDriver, Visitor, visit_struct, visit_value},
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
    runtime_value::{self as R, MOVE_STRUCT_FIELDS, MOVE_STRUCT_TYPE},
    u256,
};
use anyhow::Result as AResult;
use serde::{
    Deserialize, Serialize,
    de::Error as DeError,
    ser::{SerializeMap, SerializeSeq, SerializeStruct},
};
use std::{
    collections::BTreeMap,
    fmt::{self, Debug},
    io::Cursor,
};

/// In the `WithTypes` configuration, a Move struct gets serialized into a Serde struct with this name
pub const MOVE_STRUCT_NAME: &str = "struct";

/// In the `WithTypes` configuration, a Move enum/struct gets serialized into a Serde struct with this as the first field
pub const MOVE_DATA_TYPE: &str = "type";

/// In the `WithTypes` configuration, a Move struct gets serialized into a Serde struct with this as the second field
pub const MOVE_DATA_FIELDS: &str = "fields";

/// In the `WithTypes` configuration, a Move enum gets serialized into a Serde struct with this as the second field
/// In the `WithFields` configuration, this is the first field of the serialized enum
pub const MOVE_VARIANT_NAME: &str = "variant_name";

/// Field name for the tag of the variant
pub const MOVE_VARIANT_TAG_NAME: &str = "variant_tag";

/// In the `WithTypes` configuration, a Move enum gets serialized into a Serde struct with this name
pub const MOVE_ENUM_NAME: &str = "enum";

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MoveStruct {
    pub type_: StructTag,
    pub fields: Vec<(Identifier, MoveValue)>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MoveVariant {
    pub type_: StructTag,
    pub variant_name: Identifier,
    pub tag: u16,
    pub fields: Vec<(Identifier, MoveValue)>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum MoveValue {
    U8(u8),
    U64(u64),
    U128(u128),
    Bool(bool),
    Address(AccountAddress),
    Vector(Vec<MoveValue>),
    Struct(MoveStruct),
    Signer(AccountAddress),
    // NOTE: Added in bytecode version v6, do not reorder!
    U16(u16),
    U32(u32),
    U256(u256::U256),
    Variant(MoveVariant),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MoveFieldLayout {
    pub name: Identifier,
    pub layout: MoveTypeLayout,
}

impl MoveFieldLayout {
    pub fn new(name: Identifier, layout: MoveTypeLayout) -> Self {
        Self { name, layout }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MoveStructLayout {
    /// An decorated representation with both types and human-readable field names
    pub type_: StructTag,
    pub fields: Vec<MoveFieldLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MoveEnumLayout {
    pub type_: StructTag,
    pub variants: BTreeMap<(Identifier, u16), Vec<MoveFieldLayout>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoveDatatypeLayout {
    Struct(Box<MoveStructLayout>),
    Enum(Box<MoveEnumLayout>),
}

impl MoveDatatypeLayout {
    pub fn into_layout(self) -> MoveTypeLayout {
        match self {
            Self::Struct(s) => MoveTypeLayout::Struct(s),
            Self::Enum(e) => MoveTypeLayout::Enum(e),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MoveTypeLayout {
    #[serde(rename(serialize = "bool", deserialize = "bool"))]
    Bool,
    #[serde(rename(serialize = "u8", deserialize = "u8"))]
    U8,
    #[serde(rename(serialize = "u64", deserialize = "u64"))]
    U64,
    #[serde(rename(serialize = "u128", deserialize = "u128"))]
    U128,
    #[serde(rename(serialize = "address", deserialize = "address"))]
    Address,
    #[serde(rename(serialize = "vector", deserialize = "vector"))]
    Vector(Box<MoveTypeLayout>),
    #[serde(rename(serialize = "struct", deserialize = "struct"))]
    Struct(Box<MoveStructLayout>),
    #[serde(rename(serialize = "signer", deserialize = "signer"))]
    Signer,

    // NOTE: Added in bytecode version v6, do not reorder!
    #[serde(rename(serialize = "u16", deserialize = "u16"))]
    U16,
    #[serde(rename(serialize = "u32", deserialize = "u32"))]
    U32,
    #[serde(rename(serialize = "u256", deserialize = "u256"))]
    U256,
    #[serde(rename(serialize = "enum", deserialize = "enum"))]
    Enum(Box<MoveEnumLayout>),
}

impl MoveStructLayout {
    /// Returns `true` if and only if the layout is for `type_`.
    pub fn is_type(&self, type_: &StructTag) -> bool {
        self.type_ == *type_
    }
}

impl MoveEnumLayout {
    /// Returns `true` if and only if the layout is for `type_`.
    pub fn is_type(&self, type_: &StructTag) -> bool {
        self.type_ == *type_
    }
}

impl MoveTypeLayout {
    /// Returns `true` if and only if the layout is for `type_`.
    pub fn is_type(&self, type_: &TypeTag) -> bool {
        use MoveTypeLayout as L;
        use TypeTag as T;

        match self {
            L::Bool => matches!(type_, T::Bool),
            L::U8 => matches!(type_, T::U8),
            L::U16 => matches!(type_, T::U16),
            L::U32 => matches!(type_, T::U32),
            L::U64 => matches!(type_, T::U64),
            L::U128 => matches!(type_, T::U128),
            L::U256 => matches!(type_, T::U256),
            L::Address => matches!(type_, T::Address),
            L::Signer => matches!(type_, T::Signer),
            L::Vector(l) => matches!(type_, T::Vector(t) if l.is_type(t)),
            L::Struct(l) => matches!(type_, T::Struct(t) if l.is_type(t)),
            L::Enum(l) => matches!(type_, T::Struct(t) if l.is_type(t)),
        }
    }
}

impl MoveValue {
    /// TODO (annotated-visitor): Port legacy uses of this method to `BoundedVisitor`.
    pub fn simple_deserialize(blob: &[u8], ty: &MoveTypeLayout) -> AResult<Self> {
        Ok(bcs::from_bytes_seed(ty, blob)?)
    }

    /// Deserialize a BCS-encoded blob using a compressed annotated type layout.
    pub fn simple_deserialize_compressed(
        blob: &[u8],
        layout: &compressed_layouts::MoveTypeLayout,
    ) -> AResult<Self> {
        Ok(bcs::from_bytes_seed(layout.as_view(), blob)?)
    }

    /// Deserialize `blob` as a Move value with the given `ty`-pe layout, and visit its
    /// sub-structure with the given `visitor`. The visitor dictates the return value that is built
    /// up during deserialization.
    ///
    /// # Nested deserialization
    ///
    /// Vectors and structs are nested structures that can be met during deserialization. Visitors
    /// are passed a driver (`VecDriver` or `StructDriver` correspondingly) which controls how
    /// nested elements or fields are visited including whether a given nested element/field is
    /// explored, which visitor to use (the visitor can pass `self` to recursively explore them) and
    /// whether a given element is visited or skipped.
    ///
    /// The visitor may leave elements unvisited at the end of the vector or struct, which
    /// implicitly skips them.
    ///
    /// # Errors
    ///
    /// Deserialization can fail because of an issue in the serialized format (data doesn't match
    /// layout, unexpected bytes or trailing bytes), or a custom error expressed by the visitor.
    pub fn visit_deserialize<'b, 'l, V: Visitor<'b, 'l>>(
        blob: &'b [u8],
        ty: &'l MoveTypeLayout,
        visitor: &mut V,
    ) -> Result<V::Value, V::Error>
    where
        V::Error: std::error::Error + Send + Sync + 'static,
    {
        // TODO: Don't simplify error to anyhow::Error
        let mut bytes = Cursor::new(blob);
        let res = visit_value(&mut bytes, ty, visitor)?;
        if bytes.position() as usize == blob.len() {
            Ok(res)
        } else {
            let remaining = blob.len() - bytes.position() as usize;
            Err(VError::TrailingBytes(remaining).into())
        }
    }

    pub fn simple_serialize(&self) -> Option<Vec<u8>> {
        bcs::to_bytes(self).ok()
    }

    pub fn undecorate(self) -> R::MoveValue {
        match self {
            Self::Struct(s) => R::MoveValue::Struct(s.undecorate()),
            Self::Variant(v) => R::MoveValue::Variant(v.undecorate()),
            Self::Vector(vals) => {
                R::MoveValue::Vector(vals.into_iter().map(MoveValue::undecorate).collect())
            }
            MoveValue::U8(u) => R::MoveValue::U8(u),
            MoveValue::U64(u) => R::MoveValue::U64(u),
            MoveValue::U128(u) => R::MoveValue::U128(u),
            MoveValue::Bool(b) => R::MoveValue::Bool(b),
            MoveValue::Address(a) => R::MoveValue::Address(a),
            MoveValue::Signer(s) => R::MoveValue::Signer(s),
            MoveValue::U16(u) => R::MoveValue::U16(u),
            MoveValue::U32(u) => R::MoveValue::U32(u),
            MoveValue::U256(u) => R::MoveValue::U256(u),
        }
    }
}

pub fn serialize_values<'a, I>(vals: I) -> Vec<Vec<u8>>
where
    I: IntoIterator<Item = &'a MoveValue>,
{
    vals.into_iter()
        .map(|val| {
            val.simple_serialize()
                .expect("serialization should succeed")
        })
        .collect()
}

impl MoveStruct {
    pub fn new(type_: StructTag, fields: Vec<(Identifier, MoveValue)>) -> Self {
        Self { type_, fields }
    }

    /// TODO (annotated-visitor): Port legacy uses of this method to `BoundedVisitor`.
    pub fn simple_deserialize(blob: &[u8], ty: &MoveStructLayout) -> AResult<Self> {
        Ok(bcs::from_bytes_seed(ty, blob)?)
    }

    /// Like `MoveValue::visit_deserialize` (see for details), but specialized to visiting a struct
    /// (the `blob` is known to be a serialized Move struct, and the layout is a
    /// `MoveStructLayout`).
    pub fn visit_deserialize<'b, 'l, V: Visitor<'b, 'l>>(
        blob: &'b [u8],
        ty: &'l MoveStructLayout,
        visitor: &mut V,
    ) -> Result<V::Value, V::Error>
    where
        V::Error: std::error::Error + Send + Sync + 'static,
    {
        let mut bytes = Cursor::new(blob);
        let driver = ValueDriver::new(&mut bytes, None);
        let res = visit_struct(driver, ty, visitor)?;
        if bytes.position() as usize == blob.len() {
            Ok(res)
        } else {
            let remaining = blob.len() - bytes.position() as usize;
            Err(VError::TrailingBytes(remaining).into())
        }
    }

    pub fn into_fields(self) -> Vec<MoveValue> {
        self.fields.into_iter().map(|(_, v)| v).collect()
    }

    pub fn undecorate(self) -> R::MoveStruct {
        R::MoveStruct(
            self.into_fields()
                .into_iter()
                .map(MoveValue::undecorate)
                .collect(),
        )
    }
}

impl MoveVariant {
    pub fn new(
        type_: StructTag,
        variant_name: Identifier,
        tag: u16,
        fields: Vec<(Identifier, MoveValue)>,
    ) -> Self {
        Self {
            type_,
            variant_name,
            tag,
            fields,
        }
    }

    pub fn simple_deserialize(blob: &[u8], ty: &MoveEnumLayout) -> AResult<Self> {
        Ok(bcs::from_bytes_seed(ty, blob)?)
    }

    pub fn into_fields(self) -> Vec<MoveValue> {
        self.fields.into_iter().map(|(_, v)| v).collect()
    }

    pub fn undecorate(self) -> R::MoveVariant {
        R::MoveVariant {
            tag: self.tag,
            fields: self
                .into_fields()
                .into_iter()
                .map(MoveValue::undecorate)
                .collect(),
        }
    }
}

impl MoveStructLayout {
    pub fn new(type_: StructTag, fields: Vec<MoveFieldLayout>) -> Self {
        Self { type_, fields }
    }

    pub fn into_fields(self) -> Vec<MoveTypeLayout> {
        self.fields.into_iter().map(|f| f.layout).collect()
    }
}

impl<'d> serde::de::DeserializeSeed<'d> for &MoveTypeLayout {
    type Value = MoveValue;
    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        match self {
            MoveTypeLayout::Bool => bool::deserialize(deserializer).map(MoveValue::Bool),
            MoveTypeLayout::U8 => u8::deserialize(deserializer).map(MoveValue::U8),
            MoveTypeLayout::U16 => u16::deserialize(deserializer).map(MoveValue::U16),
            MoveTypeLayout::U32 => u32::deserialize(deserializer).map(MoveValue::U32),
            MoveTypeLayout::U64 => u64::deserialize(deserializer).map(MoveValue::U64),
            MoveTypeLayout::U128 => u128::deserialize(deserializer).map(MoveValue::U128),
            MoveTypeLayout::U256 => u256::U256::deserialize(deserializer).map(MoveValue::U256),
            MoveTypeLayout::Address => {
                AccountAddress::deserialize(deserializer).map(MoveValue::Address)
            }
            MoveTypeLayout::Signer => {
                AccountAddress::deserialize(deserializer).map(MoveValue::Signer)
            }
            MoveTypeLayout::Struct(ty) => Ok(MoveValue::Struct(ty.deserialize(deserializer)?)),
            MoveTypeLayout::Enum(ty) => Ok(MoveValue::Variant(ty.deserialize(deserializer)?)),
            MoveTypeLayout::Vector(layout) => Ok(MoveValue::Vector(
                deserializer.deserialize_seq(VectorElementVisitor(layout))?,
            )),
        }
    }
}

struct VectorElementVisitor<'a>(&'a MoveTypeLayout);

impl<'d> serde::de::Visitor<'d> for VectorElementVisitor<'_> {
    type Value = Vec<MoveValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Vector")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let mut vals = Vec::new();
        while let Some(elem) = seq.next_element_seed(self.0)? {
            vals.push(elem)
        }
        Ok(vals)
    }
}

struct DecoratedStructFieldVisitor<'a>(&'a [MoveFieldLayout]);

impl<'d> serde::de::Visitor<'d> for DecoratedStructFieldVisitor<'_> {
    type Value = Vec<(Identifier, MoveValue)>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Struct")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let mut vals = Vec::new();
        for (i, layout) in self.0.iter().enumerate() {
            match seq.next_element_seed(layout)? {
                Some(elem) => vals.push(elem),
                None => return Err(A::Error::invalid_length(i, &self)),
            }
        }
        Ok(vals)
    }
}

impl<'d> serde::de::DeserializeSeed<'d> for &MoveFieldLayout {
    type Value = (Identifier, MoveValue);

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        Ok((self.name.clone(), self.layout.deserialize(deserializer)?))
    }
}

impl<'d> serde::de::DeserializeSeed<'d> for &MoveStructLayout {
    type Value = MoveStruct;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        let fields = deserializer
            .deserialize_tuple(self.fields.len(), DecoratedStructFieldVisitor(&self.fields))?;
        Ok(MoveStruct {
            type_: self.type_.clone(),
            fields,
        })
    }
}

impl<'d> serde::de::DeserializeSeed<'d> for &MoveEnumLayout {
    type Value = MoveVariant;
    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        let (variant_name, tag, fields) =
            deserializer.deserialize_tuple(2, DecoratedEnumFieldVisitor(&self.variants))?;
        Ok(MoveVariant {
            type_: self.type_.clone(),
            variant_name,
            tag,
            fields,
        })
    }
}

struct DecoratedEnumFieldVisitor<'a>(&'a BTreeMap<(Identifier, u16), Vec<MoveFieldLayout>>);

impl<'d> serde::de::Visitor<'d> for DecoratedEnumFieldVisitor<'_> {
    type Value = (Identifier, u16, Vec<(Identifier, MoveValue)>);

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Enum")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let tag = match seq.next_element_seed(&MoveTypeLayout::U8)? {
            Some(MoveValue::U8(tag)) if tag as u64 <= VARIANT_TAG_MAX_VALUE => tag as u16,
            Some(MoveValue::U8(tag)) => return Err(A::Error::invalid_length(tag as usize, &self)),
            Some(val) => {
                return Err(A::Error::invalid_type(
                    serde::de::Unexpected::Other(&format!("{val:?}")),
                    &self,
                ));
            }
            None => return Err(A::Error::invalid_length(0, &self)),
        };

        let Some(((variant_name, _), variant_layout)) =
            self.0.iter().find(|((_, v_tag), _)| *v_tag == tag)
        else {
            return Err(A::Error::invalid_length(tag as usize, &self));
        };

        let Some(fields) = seq.next_element_seed(&DecoratedVariantFieldLayout(variant_layout))?
        else {
            return Err(A::Error::invalid_length(1, &self));
        };

        Ok((variant_name.clone(), tag, fields))
    }
}

struct DecoratedVariantFieldLayout<'a>(&'a Vec<MoveFieldLayout>);

impl<'d> serde::de::DeserializeSeed<'d> for &DecoratedVariantFieldLayout<'_> {
    type Value = Vec<(Identifier, MoveValue)>;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_tuple(self.0.len(), DecoratedStructFieldVisitor(self.0))
    }
}

impl serde::Serialize for MoveValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            MoveValue::Struct(s) => s.serialize(serializer),
            MoveValue::Variant(v) => v.serialize(serializer),
            MoveValue::Bool(b) => serializer.serialize_bool(*b),
            MoveValue::U8(i) => serializer.serialize_u8(*i),
            MoveValue::U16(i) => serializer.serialize_u16(*i),
            MoveValue::U32(i) => serializer.serialize_u32(*i),
            MoveValue::U64(i) => serializer.serialize_u64(*i),
            MoveValue::U128(i) => serializer.serialize_u128(*i),
            MoveValue::U256(i) => i.serialize(serializer),
            MoveValue::Address(a) => a.serialize(serializer),
            MoveValue::Signer(a) => a.serialize(serializer),
            MoveValue::Vector(v) => {
                let mut t = serializer.serialize_seq(Some(v.len()))?;
                for val in v {
                    t.serialize_element(val)?;
                }
                t.end()
            }
        }
    }
}

struct MoveFields<'a>(&'a [(Identifier, MoveValue)]);

impl serde::Serialize for MoveFields<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut t = serializer.serialize_map(Some(self.0.len()))?;
        for (f, v) in self.0.iter() {
            t.serialize_entry(f, v)?;
        }
        t.end()
    }
}

impl serde::Serialize for MoveStruct {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Serialize a Move struct as Serde struct type named `struct `with two fields named `type` and `fields`.
        // `fields` will get serialized as a Serde map.
        // Unfortunately, we can't serialize this in the logical way: as a Serde struct named `type` with a field for
        // each of `fields` because serde insists that struct and field names be `'static &str`'s
        let mut t = serializer.serialize_struct(MOVE_STRUCT_NAME, 2)?;
        // serialize type as string (e.g., 0x0::ModuleName::StructName<TypeArg1,TypeArg2>) instead of (e.g.
        // { address: 0x0...0, module: ModuleName, name: StructName, type_args: [TypeArg1, TypeArg2]})
        t.serialize_field(MOVE_STRUCT_TYPE, &self.type_.to_string())?;
        t.serialize_field(MOVE_STRUCT_FIELDS, &MoveFields(&self.fields))?;
        t.end()
    }
}

impl serde::Serialize for MoveVariant {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Serialize an enum as:
        // enum { "type": 0xC::module::enum_type, "variant_name": name, "variant_tag": tag, "fields": { ... } }
        let mut t = serializer.serialize_struct(MOVE_ENUM_NAME, 4)?;
        t.serialize_field(MOVE_DATA_TYPE, &self.type_.to_string())?;
        t.serialize_field(MOVE_VARIANT_NAME, &self.variant_name.to_string())?;
        t.serialize_field(MOVE_VARIANT_TAG_NAME, &MoveValue::U16(self.tag))?;
        t.serialize_field(MOVE_DATA_FIELDS, &MoveFields(&self.fields))?;
        t.end()
    }
}

impl fmt::Display for MoveTypeLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        use MoveTypeLayout::*;
        match self {
            Bool => write!(f, "bool"),
            U8 => write!(f, "u8"),
            U16 => write!(f, "u16"),
            U32 => write!(f, "u32"),
            U64 => write!(f, "u64"),
            U128 => write!(f, "u128"),
            U256 => write!(f, "u256"),
            Address => write!(f, "address"),
            Signer => write!(f, "signer"),
            Vector(typ) if f.alternate() => write!(f, "vector<{typ:#}>"),
            Vector(typ) => write!(f, "vector<{typ}>"),
            Struct(s) if f.alternate() => write!(f, "{s:#}"),
            Struct(s) => write!(f, "{s}"),
            Enum(e) if f.alternate() => write!(f, "{e:#}"),
            Enum(e) => write!(f, "enum {}", e),
        }
    }
}

/// Helper type that uses `T`'s `Display` implementation as its own `Debug` implementation, to allow
/// other `Display` implementations in this module to take advantage of the structured formatting
/// helpers that Rust uses for its own debug types.
pub struct DebugAsDisplay<'a, T>(pub &'a T);
impl<T: fmt::Display> fmt::Debug for DebugAsDisplay<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{:#}", self.0)
        } else {
            write!(f, "{}", self.0)
        }
    }
}

impl fmt::Display for MoveStructLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        use DebugAsDisplay as DD;
        write!(f, "struct ")?;
        write!(f, "{} ", self.type_)?;
        let mut map = f.debug_map();
        for field in &*self.fields {
            map.entry(&DD(&field.name), &DD(&field.layout));
        }
        map.finish()
    }
}

impl fmt::Display for MoveEnumLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        use DebugAsDisplay as DD;
        write!(f, "enum {} ", self.type_)?;
        let mut vmap = f.debug_set();
        for ((variant_name, _), fields) in self.variants.iter() {
            vmap.entry(&DD(&MoveVariantDisplay(variant_name.as_str(), fields)));
        }
        vmap.finish()
    }
}

struct MoveVariantDisplay<'a>(&'a str, &'a [MoveFieldLayout]);

impl fmt::Display for MoveVariantDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        use DebugAsDisplay as DD;
        let mut map = f.debug_struct(self.0);
        for field in self.1 {
            map.field(field.name.as_str(), &DD(&field.layout));
        }
        map.finish()
    }
}

impl From<&MoveTypeLayout> for TypeTag {
    fn from(val: &MoveTypeLayout) -> TypeTag {
        match val {
            MoveTypeLayout::Address => TypeTag::Address,
            MoveTypeLayout::Bool => TypeTag::Bool,
            MoveTypeLayout::U8 => TypeTag::U8,
            MoveTypeLayout::U16 => TypeTag::U16,
            MoveTypeLayout::U32 => TypeTag::U32,
            MoveTypeLayout::U64 => TypeTag::U64,
            MoveTypeLayout::U128 => TypeTag::U128,
            MoveTypeLayout::U256 => TypeTag::U256,
            MoveTypeLayout::Signer => TypeTag::Signer,
            MoveTypeLayout::Vector(v) => {
                let inner_type = &**v;
                TypeTag::Vector(Box::new(inner_type.into()))
            }
            MoveTypeLayout::Struct(v) => TypeTag::Struct(Box::new(v.as_ref().into())),
            MoveTypeLayout::Enum(e) => TypeTag::Struct(Box::new(e.as_ref().into())),
        }
    }
}

impl From<&MoveStructLayout> for StructTag {
    fn from(val: &MoveStructLayout) -> StructTag {
        val.type_.clone()
    }
}

impl From<&MoveEnumLayout> for StructTag {
    fn from(val: &MoveEnumLayout) -> StructTag {
        val.type_.clone()
    }
}

impl fmt::Display for MoveValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoveValue::U8(u) => write!(f, "{}u8", u),
            MoveValue::U16(u) => write!(f, "{}u16", u),
            MoveValue::U32(u) => write!(f, "{}u32", u),
            MoveValue::U64(u) => write!(f, "{}u64", u),
            MoveValue::U128(u) => write!(f, "{}u128", u),
            MoveValue::U256(u) => write!(f, "{}u256", u),
            MoveValue::Bool(false) => write!(f, "false"),
            MoveValue::Bool(true) => write!(f, "true"),
            MoveValue::Address(a) => write!(f, "{}", a.to_hex_literal()),
            MoveValue::Signer(a) => write!(f, "signer({})", a.to_hex_literal()),
            MoveValue::Vector(v) => {
                use DebugAsDisplay as DD;
                write!(f, "vector")?;
                let mut list = f.debug_list();
                for val in v {
                    list.entry(&DD(val));
                }
                list.finish()
            }
            MoveValue::Struct(s) => fmt::Display::fmt(s, f),
            MoveValue::Variant(v) => fmt::Display::fmt(v, f),
        }
    }
}

impl fmt::Display for MoveStruct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use DebugAsDisplay as DD;
        fmt::Display::fmt(&self.type_, f)?;
        write!(f, " ")?;
        let mut map = f.debug_map();
        for (field, value) in &self.fields {
            map.entry(&DD(field), &DD(value));
        }
        map.finish()
    }
}

impl fmt::Display for MoveVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use DebugAsDisplay as DD;
        let MoveVariant {
            type_,
            variant_name,
            tag: _,
            fields,
        } = self;
        write!(f, "{}::{}", type_, variant_name)?;
        let mut map = f.debug_map();
        for (field, value) in fields {
            map.entry(&DD(field), &DD(value));
        }
        map.finish()
    }
}

pub mod compressed_layouts {
    use super::{
        MoveEnumLayout, MoveFieldLayout, MoveStructLayout, MoveTypeLayout as TreeMoveTypeLayout,
    };
    use crate::identifier::Identifier;
    use crate::language_storage::StructTag;
    use crate::runtime_value::compressed_layouts::LayoutIdx;
    use anyhow::Result as AResult;
    use indexmap::IndexSet;
    use serde::{Deserialize, Serialize};

    // =============================================================================
    // Compressed (interned) annotated layout types
    // =============================================================================

    /// Index into an [`MoveTypeLayout`]'s strings table.
    pub type StringIdx = usize;

    /// Index into an [`MoveTypeLayout`]'s tags table.
    pub type TagIdx = usize;

    /// A list of (field_name_idx, layout_idx) pairs for struct/enum fields.
    pub type AnnotatedFieldIndices = Box<[(StringIdx, LayoutIdx)]>;

    /// A single variant entry: (variant_name_idx, tag, field_indices).
    pub type AnnotatedVariantEntry = (StringIdx, u16, AnnotatedFieldIndices);

    /// Annotated struct layout node: type tag + named fields stored as interned indices.
    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct MoveStructNode {
        pub type_: TagIdx,
        pub fields: AnnotatedFieldIndices,
    }

    /// Annotated enum layout node: type tag + named variants with named fields.
    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct MoveEnumNode {
        pub type_: TagIdx,
        pub variants: Box<[AnnotatedVariantEntry]>,
    }

    /// A single layout node in an annotated compressed layout table.
    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum MoveTypeNode {
        Bool,
        U8,
        U16,
        U32,
        U64,
        U128,
        U256,
        Address,
        Signer,
        Vector(LayoutIdx),
        Struct(MoveStructNode),
        Enum(MoveEnumNode),
    }

    /// A deduplicated, flat representation of an annotated [`MoveTypeLayout`] tree.
    /// Strings (field names, variant names) and [`StructTag`]s are interned into
    /// separate side tables.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MoveTypeLayout {
        nodes: Box<[MoveTypeNode]>,
        strings: Box<[Identifier]>,
        tags: Box<[StructTag]>,
        root: LayoutIdx,
    }

    impl MoveTypeLayout {
        /// Number of unique nodes in the compressed layout.
        pub fn node_count(&self) -> usize {
            self.nodes.len()
        }

        /// Number of unique interned strings (field/variant names).
        pub fn string_count(&self) -> usize {
            self.strings.len()
        }

        /// Number of unique interned struct tags.
        pub fn tag_count(&self) -> usize {
            self.tags.len()
        }

        /// Create a borrowed view of this layout.
        pub fn as_view(&self) -> MoveTypeLayoutView<'_> {
            MoveTypeLayoutView {
                nodes: &self.nodes,
                strings: &self.strings,
                tags: &self.tags,
                root: self.root,
            }
        }

        /// Inflate back into a tree-based [`MoveTypeLayout`].
        pub fn inflate(&self) -> AResult<TreeMoveTypeLayout> {
            self.as_view().inflate()
        }
    }

    // =============================================================================
    // View — the primary public API for navigating compressed layouts
    // =============================================================================

    /// A borrowed, `Copy` cursor into an annotated [`MoveTypeLayout`]. Navigate by
    /// calling [`current`](Self::current) to inspect the node, [`view_at`](Self::view_at)
    /// to move to a child, and [`resolve_string`](Self::resolve_string) /
    /// [`resolve_tag`](Self::resolve_tag) to look up interned names and type tags.
    #[derive(Debug, Clone, Copy)]
    pub struct MoveTypeLayoutView<'a> {
        nodes: &'a [MoveTypeNode],
        strings: &'a [Identifier],
        tags: &'a [StructTag],
        root: LayoutIdx,
    }

    impl<'a> MoveTypeLayoutView<'a> {
        /// The node at this view's current position.
        pub fn current(&self) -> AResult<&'a MoveTypeNode> {
            self.nodes.get(self.root).ok_or_else(|| {
                anyhow::anyhow!(
                    "layout index {} out of bounds (table has {} nodes)",
                    self.root,
                    self.nodes.len()
                )
            })
        }

        /// Create a sub-view rooted at the given index. No bounds check is
        /// performed here — it is deferred to [`current`](Self::current).
        pub fn view_at(&self, idx: LayoutIdx) -> MoveTypeLayoutView<'a> {
            MoveTypeLayoutView {
                nodes: self.nodes,
                strings: self.strings,
                tags: self.tags,
                root: idx,
            }
        }

        /// Resolve a string index to an identifier.
        pub fn resolve_string(&self, idx: StringIdx) -> AResult<&'a Identifier> {
            self.strings.get(idx).ok_or_else(|| {
                anyhow::anyhow!(
                    "string index {} out of bounds (table has {} strings)",
                    idx,
                    self.strings.len()
                )
            })
        }

        /// Resolve a tag index to a struct tag.
        pub fn resolve_tag(&self, idx: TagIdx) -> AResult<&'a StructTag> {
            self.tags.get(idx).ok_or_else(|| {
                anyhow::anyhow!(
                    "tag index {} out of bounds (table has {} tags)",
                    idx,
                    self.tags.len()
                )
            })
        }

        /// Inflate back into a tree-based layout.
        pub fn inflate(&self) -> AResult<TreeMoveTypeLayout> {
            inflate_idx(self, self.root)
        }
    }

    // =============================================================================
    // Internal helpers
    // =============================================================================

    fn inflate_idx(view: &MoveTypeLayoutView<'_>, idx: LayoutIdx) -> AResult<TreeMoveTypeLayout> {
        let node = view.view_at(idx).current()?;
        match node {
            MoveTypeNode::Bool => Ok(TreeMoveTypeLayout::Bool),
            MoveTypeNode::U8 => Ok(TreeMoveTypeLayout::U8),
            MoveTypeNode::U16 => Ok(TreeMoveTypeLayout::U16),
            MoveTypeNode::U32 => Ok(TreeMoveTypeLayout::U32),
            MoveTypeNode::U64 => Ok(TreeMoveTypeLayout::U64),
            MoveTypeNode::U128 => Ok(TreeMoveTypeLayout::U128),
            MoveTypeNode::U256 => Ok(TreeMoveTypeLayout::U256),
            MoveTypeNode::Address => Ok(TreeMoveTypeLayout::Address),
            MoveTypeNode::Signer => Ok(TreeMoveTypeLayout::Signer),
            MoveTypeNode::Vector(inner) => Ok(TreeMoveTypeLayout::Vector(Box::new(inflate_idx(
                view, *inner,
            )?))),
            MoveTypeNode::Struct(s) => {
                let type_ = view.resolve_tag(s.type_)?.clone();
                let fields = s
                    .fields
                    .iter()
                    .map(|(name_idx, layout_idx)| {
                        Ok(MoveFieldLayout::new(
                            view.resolve_string(*name_idx)?.clone(),
                            inflate_idx(view, *layout_idx)?,
                        ))
                    })
                    .collect::<AResult<_>>()?;
                Ok(TreeMoveTypeLayout::Struct(Box::new(MoveStructLayout {
                    type_,
                    fields,
                })))
            }
            MoveTypeNode::Enum(e) => {
                let type_ = view.resolve_tag(e.type_)?.clone();
                let variants = e
                    .variants
                    .iter()
                    .map(|(name_idx, tag, fields)| {
                        let variant_name = view.resolve_string(*name_idx)?.clone();
                        let field_layouts = fields
                            .iter()
                            .map(|(fn_idx, l_idx)| {
                                Ok(MoveFieldLayout::new(
                                    view.resolve_string(*fn_idx)?.clone(),
                                    inflate_idx(view, *l_idx)?,
                                ))
                            })
                            .collect::<AResult<_>>()?;
                        Ok(((variant_name, *tag), field_layouts))
                    })
                    .collect::<AResult<_>>()?;
                Ok(TreeMoveTypeLayout::Enum(Box::new(MoveEnumLayout {
                    type_,
                    variants,
                })))
            }
        }
    }

    // =============================================================================
    // Builder
    // =============================================================================

    /// Incrementally builds an annotated [`MoveTypeLayout`] with automatic
    /// deduplication of nodes, field/variant names, and struct tags.
    pub struct MoveTypeLayoutBuilder {
        nodes: IndexSet<MoveTypeNode>,
        strings: IndexSet<Identifier>,
        tags: IndexSet<StructTag>,
    }

    impl MoveTypeLayoutBuilder {
        pub fn new() -> Self {
            Self {
                nodes: IndexSet::new(),
                strings: IndexSet::new(),
                tags: IndexSet::new(),
            }
        }

        fn intern_string(&mut self, s: &Identifier) -> StringIdx {
            let (idx, _) = self.strings.insert_full(s.clone());
            idx
        }

        fn intern_tag(&mut self, tag: &StructTag) -> TagIdx {
            let (idx, _) = self.tags.insert_full(tag.clone());
            idx
        }

        fn intern(&mut self, node: MoveTypeNode) -> LayoutIdx {
            let (idx, _) = self.nodes.insert_full(node);
            idx
        }

        pub fn bool(&mut self) -> LayoutIdx {
            self.intern(MoveTypeNode::Bool)
        }
        pub fn u8(&mut self) -> LayoutIdx {
            self.intern(MoveTypeNode::U8)
        }
        pub fn u16(&mut self) -> LayoutIdx {
            self.intern(MoveTypeNode::U16)
        }
        pub fn u32(&mut self) -> LayoutIdx {
            self.intern(MoveTypeNode::U32)
        }
        pub fn u64(&mut self) -> LayoutIdx {
            self.intern(MoveTypeNode::U64)
        }
        pub fn u128(&mut self) -> LayoutIdx {
            self.intern(MoveTypeNode::U128)
        }
        pub fn u256(&mut self) -> LayoutIdx {
            self.intern(MoveTypeNode::U256)
        }
        pub fn address(&mut self) -> LayoutIdx {
            self.intern(MoveTypeNode::Address)
        }
        pub fn signer(&mut self) -> LayoutIdx {
            self.intern(MoveTypeNode::Signer)
        }

        pub fn vector(&mut self, element: LayoutIdx) -> LayoutIdx {
            self.intern(MoveTypeNode::Vector(element))
        }

        /// Build a struct layout node.
        /// `fields` is a list of (field_name, field_layout) pairs.
        pub fn struct_layout(
            &mut self,
            type_tag: &StructTag,
            fields: &[(&Identifier, LayoutIdx)],
        ) -> LayoutIdx {
            let tag_idx = self.intern_tag(type_tag);
            let field_indices: AnnotatedFieldIndices = fields
                .iter()
                .map(|(name, idx)| (self.intern_string(name), *idx))
                .collect();
            self.intern(MoveTypeNode::Struct(MoveStructNode {
                type_: tag_idx,
                fields: field_indices,
            }))
        }

        /// Build an enum layout node.
        /// Each variant is `(variant_name, tag, fields)` where fields is `[(field_name, layout)]`.
        pub fn enum_layout(
            &mut self,
            type_tag: &StructTag,
            variants: &[(&Identifier, u16, &[(&Identifier, LayoutIdx)])],
        ) -> LayoutIdx {
            let tag_idx = self.intern_tag(type_tag);
            let variant_entries: Box<[AnnotatedVariantEntry]> = variants
                .iter()
                .map(|(vn, tag, fields)| {
                    let vn_idx = self.intern_string(vn);
                    let field_indices: AnnotatedFieldIndices = fields
                        .iter()
                        .map(|(fn_name, idx)| (self.intern_string(fn_name), *idx))
                        .collect();
                    (vn_idx, *tag, field_indices)
                })
                .collect();
            self.intern(MoveTypeNode::Enum(MoveEnumNode {
                type_: tag_idx,
                variants: variant_entries,
            }))
        }

        /// Recursively intern a tree-based annotated layout.
        pub fn intern_tree(&mut self, layout: &TreeMoveTypeLayout) -> LayoutIdx {
            match layout {
                TreeMoveTypeLayout::Bool => self.bool(),
                TreeMoveTypeLayout::U8 => self.u8(),
                TreeMoveTypeLayout::U16 => self.u16(),
                TreeMoveTypeLayout::U32 => self.u32(),
                TreeMoveTypeLayout::U64 => self.u64(),
                TreeMoveTypeLayout::U128 => self.u128(),
                TreeMoveTypeLayout::U256 => self.u256(),
                TreeMoveTypeLayout::Address => self.address(),
                TreeMoveTypeLayout::Signer => self.signer(),
                TreeMoveTypeLayout::Vector(inner) => {
                    let inner_idx = self.intern_tree(inner);
                    self.vector(inner_idx)
                }
                TreeMoveTypeLayout::Struct(s) => {
                    let fields: Vec<(&Identifier, LayoutIdx)> = s
                        .fields
                        .iter()
                        .map(|f| (&f.name, self.intern_tree(&f.layout)))
                        .collect();
                    self.struct_layout(&s.type_, &fields)
                }
                TreeMoveTypeLayout::Enum(e) => {
                    let variants: Vec<(&Identifier, u16, Vec<(&Identifier, LayoutIdx)>)> = e
                        .variants
                        .iter()
                        .map(|((variant_name, tag), field_layouts)| {
                            let fields: Vec<(&Identifier, LayoutIdx)> = field_layouts
                                .iter()
                                .map(|f| (&f.name, self.intern_tree(&f.layout)))
                                .collect();
                            (variant_name, *tag, fields)
                        })
                        .collect();
                    let variant_refs: Vec<(&Identifier, u16, &[(&Identifier, LayoutIdx)])> =
                        variants
                            .iter()
                            .map(|(vn, tag, fields)| (*vn, *tag, fields.as_slice()))
                            .collect();
                    self.enum_layout(&e.type_, &variant_refs)
                }
            }
        }

        pub fn build(self, root: LayoutIdx) -> MoveTypeLayout {
            MoveTypeLayout {
                nodes: self.nodes.into_iter().collect(),
                strings: self.strings.into_iter().collect(),
                tags: self.tags.into_iter().collect(),
                root,
            }
        }
    }

    impl Default for MoveTypeLayoutBuilder {
        fn default() -> Self {
            Self::new()
        }
    }

    impl From<&TreeMoveTypeLayout> for MoveTypeLayout {
        fn from(layout: &TreeMoveTypeLayout) -> Self {
            let mut b = MoveTypeLayoutBuilder::new();
            let root = b.intern_tree(layout);
            b.build(root)
        }
    }

    // -------------------------------------------------------------------------
    // Deserialization — DeserializeSeed for MoveTypeLayoutView
    // -------------------------------------------------------------------------

    use super::{MoveStruct as AnnStruct, MoveValue as AnnValue, MoveVariant as AnnVariant};
    use crate::{VARIANT_TAG_MAX_VALUE, account_address::AccountAddress, u256};
    use serde::de::Error as _;

    impl<'d> serde::de::DeserializeSeed<'d> for MoveTypeLayoutView<'_> {
        type Value = AnnValue;

        fn deserialize<D: serde::de::Deserializer<'d>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            let node = self
                .current()
                .map_err(|e| D::Error::custom(format!("{e}")))?;
            match node {
                MoveTypeNode::Bool => bool::deserialize(deserializer).map(AnnValue::Bool),
                MoveTypeNode::U8 => u8::deserialize(deserializer).map(AnnValue::U8),
                MoveTypeNode::U16 => u16::deserialize(deserializer).map(AnnValue::U16),
                MoveTypeNode::U32 => u32::deserialize(deserializer).map(AnnValue::U32),
                MoveTypeNode::U64 => u64::deserialize(deserializer).map(AnnValue::U64),
                MoveTypeNode::U128 => u128::deserialize(deserializer).map(AnnValue::U128),
                MoveTypeNode::U256 => u256::U256::deserialize(deserializer).map(AnnValue::U256),
                MoveTypeNode::Address => {
                    AccountAddress::deserialize(deserializer).map(AnnValue::Address)
                }
                MoveTypeNode::Signer => {
                    AccountAddress::deserialize(deserializer).map(AnnValue::Signer)
                }
                MoveTypeNode::Struct(s) => {
                    let type_ = self
                        .resolve_tag(s.type_)
                        .map_err(|e| D::Error::custom(format!("{e}")))?
                        .clone();
                    let fields = deserializer.deserialize_tuple(
                        s.fields.len(),
                        CompressedStructFieldVisitor {
                            view: self,
                            fields: &s.fields,
                        },
                    )?;
                    Ok(AnnValue::Struct(AnnStruct { type_, fields }))
                }
                MoveTypeNode::Enum(e) => {
                    let type_ = self
                        .resolve_tag(e.type_)
                        .map_err(|e| D::Error::custom(format!("{e}")))?
                        .clone();
                    let (variant_name, tag, fields) = deserializer.deserialize_tuple(
                        2,
                        CompressedEnumFieldVisitor {
                            view: self,
                            variants: &e.variants,
                        },
                    )?;
                    Ok(AnnValue::Variant(AnnVariant {
                        type_,
                        variant_name,
                        tag,
                        fields,
                    }))
                }
                MoveTypeNode::Vector(inner_idx) => Ok(AnnValue::Vector(
                    deserializer
                        .deserialize_seq(CompressedVectorVisitor(self.view_at(*inner_idx)))?,
                )),
            }
        }
    }

    struct CompressedVectorVisitor<'a>(MoveTypeLayoutView<'a>);

    impl<'d> serde::de::Visitor<'d> for CompressedVectorVisitor<'_> {
        type Value = Vec<AnnValue>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("Vector")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'d>,
        {
            let mut vals = Vec::new();
            while let Some(elem) = seq.next_element_seed(self.0)? {
                vals.push(elem)
            }
            Ok(vals)
        }
    }

    struct CompressedStructFieldVisitor<'a> {
        view: MoveTypeLayoutView<'a>,
        fields: &'a [(StringIdx, LayoutIdx)],
    }

    impl<'d> serde::de::Visitor<'d> for CompressedStructFieldVisitor<'_> {
        type Value = Vec<(Identifier, AnnValue)>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("Struct")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'d>,
        {
            let mut vals = Vec::new();
            for (i, (name_idx, layout_idx)) in self.fields.iter().enumerate() {
                let name = self
                    .view
                    .resolve_string(*name_idx)
                    .map_err(|e| A::Error::custom(format!("{e}")))?
                    .clone();
                match seq.next_element_seed(self.view.view_at(*layout_idx))? {
                    Some(val) => vals.push((name, val)),
                    None => return Err(A::Error::invalid_length(i, &self)),
                }
            }
            Ok(vals)
        }
    }

    struct CompressedEnumFieldVisitor<'a> {
        view: MoveTypeLayoutView<'a>,
        variants: &'a [AnnotatedVariantEntry],
    }

    impl<'d> serde::de::Visitor<'d> for CompressedEnumFieldVisitor<'_> {
        type Value = (Identifier, u16, Vec<(Identifier, AnnValue)>);

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("Enum")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'d>,
        {
            let tag = match seq.next_element::<u8>()? {
                Some(tag) if tag as u64 <= VARIANT_TAG_MAX_VALUE => tag as u16,
                Some(tag) => return Err(A::Error::invalid_length(tag as usize, &self)),
                None => return Err(A::Error::invalid_length(0, &self)),
            };

            let Some((name_idx, _, variant_fields)) =
                self.variants.iter().find(|(_, t, _)| *t == tag)
            else {
                return Err(A::Error::invalid_length(tag as usize, &self));
            };

            let variant_name = self
                .view
                .resolve_string(*name_idx)
                .map_err(|e| A::Error::custom(format!("{e}")))?
                .clone();

            let Some(fields) = seq.next_element_seed(CompressedVariantFieldSeed {
                view: self.view,
                fields: variant_fields,
            })?
            else {
                return Err(A::Error::invalid_length(1, &self));
            };

            Ok((variant_name, tag, fields))
        }
    }

    struct CompressedVariantFieldSeed<'a> {
        view: MoveTypeLayoutView<'a>,
        fields: &'a [(StringIdx, LayoutIdx)],
    }

    impl<'d> serde::de::DeserializeSeed<'d> for CompressedVariantFieldSeed<'_> {
        type Value = Vec<(Identifier, AnnValue)>;

        fn deserialize<D: serde::de::Deserializer<'d>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            deserializer.deserialize_tuple(
                self.fields.len(),
                CompressedStructFieldVisitor {
                    view: self.view,
                    fields: self.fields,
                },
            )
        }
    }
}
